use std::{hash::{DefaultHasher,Hash,Hasher},path::Path};
fn hash_sources(path:&Path,hash:&mut DefaultHasher) {
    let mut paths:Vec<_>=std::fs::read_dir(path).expect("native source directory").map(|e|e.unwrap().path()).collect();
    paths.sort();
    for path in paths {
        path.file_name().unwrap().hash(hash);
        if path.is_dir() {hash_sources(&path,hash)} else {std::fs::read(path).unwrap().hash(hash)}
    }
}
fn main() {
    napi_build::setup();
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=Cargo.toml");
    let mut hash=DefaultHasher::new();hash_sources(Path::new("src"),&mut hash);
    println!("cargo:rerun-if-changed=vendor");hash_sources(Path::new("vendor"),&mut hash);
    for file in ["Cargo.lock","Cargo.toml","build.rs"] {std::fs::read(file).unwrap().hash(&mut hash);}
    for name in ["TARGET","PROFILE","CARGO_ENCODED_RUSTFLAGS"] {std::env::var(name).ok().hash(&mut hash);}
    let mut features:Vec<_>=std::env::vars().filter(|(name,_)|name.starts_with("CARGO_FEATURE_")).collect();features.sort();features.hash(&mut hash);
    if let Ok(output)=std::process::Command::new(std::env::var_os("RUSTC").unwrap()).arg("-vV").output() {output.stdout.hash(&mut hash);}
    println!("cargo:rustc-env=PCB_KERNEL_BUILD_KEY={:016x}",hash.finish());
}
