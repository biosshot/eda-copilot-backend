//! Optional persisted strict SPIR-V. Cache I/O must never fail a solve.
//! Reuses CubeCL's serde representation; no separate compiler or GPU dependency.
use std::{fs::{self,File,OpenOptions},io::{Read,Write},path::{Path,PathBuf},
    sync::{OnceLock,atomic::{AtomicU64,Ordering}}};
use cubecl::{prelude::*,wgpu::{AutoCompiler,AutoRepresentation}};
const MAX_ENTRY:u64=64*1024*1024;
static ROOT:OnceLock<Option<PathBuf>>=OnceLock::new();
static HITS:AtomicU64=AtomicU64::new(0);
static MISSES:AtomicU64=AtomicU64::new(0);
static ERRORS:AtomicU64=AtomicU64::new(0);
static WRITES:AtomicU64=AtomicU64::new(0);
static NEXT:AtomicU64=AtomicU64::new(0);
const SOURCE:&str="PCB F32 SPIR-V: RTE, explicit FTZ, SignedZeroInfNanPreserve, NoContraction";
#[derive(serde::Serialize,serde::Deserialize)]
struct Entry {key:String,entrypoint:String,cube:[u32;3],kernel:serde_json::Value}
fn root()->Option<&'static PathBuf> {
    ROOT.get_or_init(|| {
        if std::env::var_os("PCB_GPU_KERNEL_CACHE_DISABLED").is_some() || std::env::var_os("PCB_F32_SHADER_AUDIT_DIR").is_some(){return None;}
        let base=std::env::var_os("PCB_GPU_KERNEL_CACHE_DIR").map(PathBuf::from).unwrap_or_else(|| {
            if cfg!(target_os="windows") {std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir)}
            else if cfg!(target_os="macos") {std::env::var_os("HOME").map(|p|PathBuf::from(p).join("Library/Caches")).unwrap_or_else(std::env::temp_dir)}
            else {std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(||std::env::var_os("HOME").map(|p|PathBuf::from(p).join(".cache"))).unwrap_or_else(std::env::temp_dir)}
        });
        Some(base.join("eda-copilot/gpu-kernels").join(env!("PCB_KERNEL_BUILD_KEY")))
    }).as_ref()
}
fn checksum(bytes:&[u8])->u64 {bytes.iter().fold(0xcbf29ce484222325u64,|h,b|(h^*b as u64).wrapping_mul(0x100000001b3))}
fn read_entry(path:&Path,key:&str)->Option<Entry> {
    let file=File::open(path).ok()?;if file.metadata().ok()?.len()>MAX_ENTRY{return None;}
    let mut bytes=Vec::new();file.take(MAX_ENTRY+1).read_to_end(&mut bytes).ok()?;
    if bytes.len() as u64>MAX_ENTRY || bytes.len()<8{return None;}
    let expected=u64::from_le_bytes(bytes[..8].try_into().ok()?);
    if checksum(&bytes[8..])!=expected{return None;}
    let entry:Entry=serde_json::from_slice(&bytes[8..]).ok()?;
    (entry.key==key && entry.cube.iter().all(|v|*v>0)).then_some(entry)
}
fn write_entry(path:&Path,entry:&Entry)->std::io::Result<bool> {
    let payload=serde_json::to_vec(entry)?;
    if payload.len() as u64+8>MAX_ENTRY{return Ok(false);}
    let parent=path.parent().unwrap();fs::create_dir_all(parent)?;
    let temp=path.with_extension(format!("{}-{}.tmp",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
    let result=(|| {
        let mut file=OpenOptions::new().write(true).create_new(true).open(&temp)?;
        file.write_all(&checksum(&payload).to_le_bytes())?;file.write_all(&payload)?;drop(file);
        // The device lease normally prevents competing writers; rename remains
        // atomic even if another compatible process fills the same entry first.
        if path.exists() {return Ok(false);}
        fs::rename(&temp,path).map(|_|true)
    })();
    let _=fs::remove_file(&temp);result
}
pub(super) fn load(key:&str,name:&'static str)->Option<CompiledKernel<AutoCompiler>> {
    let root=root()?;let path=root.join(format!("{key}.json"));
    let cached=(|| {
        let e=read_entry(&path,key)?;
        let kernel=serde_json::from_value(e.kernel).ok()?;
        let repr=AutoRepresentation::SpirV(kernel);
        let words=&repr.as_spirv()?.assembled_module;
        if words.len()<5 || words[0]!=0x07230203{return None;}
        Some(CompiledKernel{entrypoint_name:e.entrypoint,debug_name:Some(name),source:SOURCE.into(),repr:Some(repr),
            cube_dim:CubeDim::new_3d(e.cube[0],e.cube[1],e.cube[2]),debug_info:None})
    })();
    if cached.is_some(){HITS.fetch_add(1,Ordering::Relaxed);}else{
        MISSES.fetch_add(1,Ordering::Relaxed);
        if path.exists(){ERRORS.fetch_add(1,Ordering::Relaxed);let _=fs::remove_file(path);}
    }
    cached
}
pub(super) fn save(key:&str,result:&CompiledKernel<AutoCompiler>) {
    let Some(root)=root() else{return;};
    let Some(AutoRepresentation::SpirV(kernel))=&result.repr else{return;};
    let saved=(|| {
        let entry=Entry{key:key.into(),entrypoint:result.entrypoint_name.clone(),
            cube:[result.cube_dim.x,result.cube_dim.y,result.cube_dim.z],kernel:serde_json::to_value(kernel)?};
        write_entry(&root.join(format!("{key}.json")),&entry)
    })();
    match saved {Ok(true)=>{WRITES.fetch_add(1,Ordering::Relaxed);},Err(_)=>{ERRORS.fetch_add(1,Ordering::Relaxed);},Ok(false)=>{}}
}
pub(super) fn statistics()->serde_json::Value {
    serde_json::json!({"enabled":root().is_some(),"buildKey":env!("PCB_KERNEL_BUILD_KEY"),
        "hits":HITS.load(Ordering::Relaxed),"misses":MISSES.load(Ordering::Relaxed),
        "writes":WRITES.load(Ordering::Relaxed),"errors":ERRORS.load(Ordering::Relaxed)})
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_rejects_wrong_keys_truncation_and_corruption() {
        let root=std::env::temp_dir().join(format!("pcb-kernel-cache-test-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        let path=root.join("entry.json");let e=Entry{key:"build-device-mode".into(),entrypoint:"main".into(),cube:[128,1,1],kernel:serde_json::json!({})};
        write_entry(&path,&e).unwrap();assert!(read_entry(&path,&e.key).is_some());assert!(read_entry(&path,"stale").is_none());
        let mut bytes=fs::read(&path).unwrap();bytes[8]^=1;fs::write(&path,&bytes).unwrap();assert!(read_entry(&path,&e.key).is_none());
        fs::write(&path,&bytes[..6]).unwrap();assert!(read_entry(&path,&e.key).is_none());
        fs::remove_file(path).unwrap();fs::remove_dir(root).unwrap();
    }
    #[test]
    fn unavailable_cache_directory_returns_io_error_without_panicking() {
        let root=std::env::temp_dir().join(format!("pcb-kernel-cache-file-{}-{}",std::process::id(),NEXT.fetch_add(1,Ordering::Relaxed)));
        fs::write(&root,b"not a directory").unwrap();
        let e=Entry{key:"test".into(),entrypoint:"main".into(),cube:[1,1,1],kernel:serde_json::json!({})};
        assert!(write_entry(&root.join("entry.json"),&e).is_err());assert!(read_entry(&root.join("entry.json"),"test").is_none());
        fs::remove_file(root).unwrap();
    }
}
