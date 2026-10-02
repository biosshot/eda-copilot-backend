//! Strict CUDA runtime. NVRTC FTZ/FMA options are pinned in the small vendored
//! CubeCL CUDA patch. Probes additionally validate the live device arithmetic.
use std::sync::Arc;
use cubecl::{client::ComputeClient, device::{DeviceId, DeviceService, ServerUtilitiesHandle},
    future::DynFut, ir::{StorageType, TargetProperties}, prelude::*, Runtime,
    stream_id::StreamId, cuda::{CudaCompiler, CudaDevice, CudaRuntime, CudaServer},
    CompilationError, CubeTask};
use cubecl_runtime::{
    allocator::PitchedMemoryLayoutPolicy,
    logging::ServerLogger, memory_management::{ManagedMemoryHandle, MemoryUsage, MemoryAllocationMode},
    server::{ComputeServer, ServerCommunication, ServerUtilities, Binding, CopyDescriptor,
        KernelArguments, ServerError, ProfilingToken, ProfileError},
    storage::{ComputeStorage, ManagedResource}};
use cubecl::{profile::ProfileDuration, bytes::Bytes};

#[derive(Clone, Debug)]
pub(crate) struct PcbCudaRuntime;
#[derive(Debug)]
pub(crate) struct PcbCudaServer {
    inner: CudaServer,
    seen: std::collections::HashSet<KernelId>,
    utilities: Arc<ServerUtilities<Self>>,
}
impl DeviceService for PcbCudaServer {
    fn init(id: DeviceId) -> Self {
        let inner = <CudaServer as DeviceService>::init(id);
        let source = <CudaServer as ComputeServer>::utilities(&inner);
        let utilities = ServerUtilities::new(source.properties.clone(), source.logger.clone(), source.info,
            PitchedMemoryLayoutPolicy::new(source.properties.memory.alignment as usize));
        Self { inner, seen:Default::default(), utilities: Arc::new(utilities) }
    }
    fn utilities(&self) -> ServerUtilitiesHandle { self.utilities.clone() }
}
impl ServerCommunication for PcbCudaServer { const SERVER_COMM_ENABLED: bool = false; }
impl ComputeServer for PcbCudaServer {
    type Kernel = Box<dyn CubeTask<CudaCompiler>>;
    type Info = <CudaServer as ComputeServer>::Info;
    type Storage = <CudaServer as ComputeServer>::Storage;
    type MemoryLayoutPolicy = PitchedMemoryLayoutPolicy;
    fn initialize_memory(&mut self,m:ManagedMemoryHandle,n:u64,s:StreamId) { self.inner.initialize_memory(m,n,s); }
    fn staging(&mut self,n:&[usize],s:StreamId)->Result<Vec<Bytes>,ServerError> {self.inner.staging(n,s)}
    fn logger(&self)->Arc<ServerLogger> { self.inner.logger() }
    fn utilities(&self)->Arc<ServerUtilities<Self>> { self.utilities.clone() }
    fn read(&mut self,d:Vec<CopyDescriptor>,s:StreamId)->DynFut<Result<Vec<Bytes>,ServerError>> {self.inner.read(d,s)}
    fn write(&mut self,d:Vec<(CopyDescriptor,Bytes)>,s:StreamId) {self.inner.write(d,s)}
    fn sync(&mut self,s:StreamId)->DynFut<Result<(),ServerError>> {self.inner.sync(s)}
    fn get_resource(&mut self,b:Binding,s:StreamId)->Result<ManagedResource<<Self::Storage as ComputeStorage>::Resource>,ServerError> {self.inner.get_resource(b,s)}
    unsafe fn launch(&mut self,k:Self::Kernel,n:CubeCount,b:KernelArguments,m:ExecutionMode,s:StreamId) {
        let mut id=k.id();id.mode(m);
        let _cold=if self.seen.insert(id) {Some(super::startup::ColdLaunch::new(k.name()))} else {None};
        self.inner.launch(Box::new(StrictTask(k)),n,b,m,s);
    }
    fn flush(&mut self,s:StreamId)->Result<(),ServerError> {self.inner.flush(s)}
    fn memory_usage(&mut self,s:StreamId)->Result<MemoryUsage,ServerError> {self.inner.memory_usage(s)}
    fn memory_cleanup(&mut self,s:StreamId) {self.inner.memory_cleanup(s)}
    fn start_profile(&mut self,s:StreamId)->Result<ProfilingToken,ServerError> {self.inner.start_profile(s)}
    fn end_profile(&mut self,s:StreamId,t:ProfilingToken)->Result<ProfileDuration,ProfileError> {self.inner.end_profile(s,t)}
    fn allocation_mode(&mut self,m:MemoryAllocationMode,s:StreamId) {self.inner.allocation_mode(m,s)}
}
impl Runtime for PcbCudaRuntime {
    type Compiler=CudaCompiler; type Server=PcbCudaServer; type Device=CudaDevice;
    fn client(d:&CudaDevice)->ComputeClient<Self> {ComputeClient::load(d)}
    fn name(_: &ComputeClient<Self>)->&'static str {"cuda<pcb-f32-strict>"}
    fn require_array_lengths()->bool {CudaRuntime::require_array_lengths()}
    fn max_cube_count()->(u32,u32,u32) {CudaRuntime::max_cube_count()}
    fn can_read_tensor(s:&cubecl::zspace::Shape,t:&cubecl::zspace::Strides)->bool {CudaRuntime::can_read_tensor(s,t)}
    fn target_properties()->TargetProperties {CudaRuntime::target_properties()}
    fn enumerate_devices(t:u16,i:&())->Vec<DeviceId> {CudaRuntime::enumerate_devices(t,i)}
    fn enumerate_all_devices(i:&())->Vec<DeviceId> {CudaRuntime::enumerate_all_devices(i)}
}
struct StrictTask(Box<dyn CubeTask<CudaCompiler>>);
impl KernelMetadata for StrictTask {
 fn name(&self)->&'static str {self.0.name()}
 fn id(&self)->KernelId {let id=self.0.id();id.clone().info((id,"pcb-cuda-f32-ftz-no-fma-v1",env!("PCB_KERNEL_BUILD_KEY")))}
 fn address_type(&self)->StorageType {self.0.address_type()}
}
impl CubeTask<CudaCompiler> for StrictTask {
 fn compile(&self,c:&mut CudaCompiler,o:&<CudaCompiler as cubecl_runtime::compiler::Compiler>::CompilationOptions,m:ExecutionMode,a:StorageType)->Result<CompiledKernel<CudaCompiler>,CompilationError> {
  let mut result=self.0.compile(c,o,m,a)?;
  // CubeCL 0.10 emits a reference reinterpret_cast of a uint32 temporary for
  // constant f32 bitcasts (including infinity sentinels). C++ rejects that
  // rvalue reference binding. CUDA's intrinsic preserves exactly the same bits.
  result.source=result.source.replace("reinterpret_cast<float const&>(uint32(", "__uint_as_float(uint32(");
  if let Some(path)=std::env::var_os("PCB_F32_SHADER_AUDIT_DIR") {
   let path=std::path::PathBuf::from(path);std::fs::create_dir_all(&path).expect("shader audit directory");
   std::fs::write(path.join(format!("{}-{:x}.cu",result.entrypoint_name,self.id().stable_hash())),&result.source).expect("CUDA shader audit");
  }
  Ok(result)
 }
}

/// CUDA 13 Windows places NVRTC in bin/x64. Load only installed toolkit DLLs
/// by absolute path and keep them loaded for the process; no bundled helper DLL.
#[cfg(windows)]
pub(super) fn prepare_libraries() {
 use std::os::windows::ffi::OsStrExt;
 #[link(name="kernel32")] extern "system" {fn LoadLibraryExW(path:*const u16,file:*mut std::ffi::c_void,flags:u32)->*mut std::ffi::c_void;}
 static LIBRARIES:std::sync::OnceLock<Vec<usize>>=std::sync::OnceLock::new();
 LIBRARIES.get_or_init(|| {
  let Some(root)=std::env::var_os("CUDA_PATH") else{return vec![];};
  let root=std::path::PathBuf::from(root);let mut loaded=Vec::new();
  for dir in [root.join("bin/x64"),root.join("bin")] {
   let Ok(entries)=std::fs::read_dir(dir) else{continue;};
   for entry in entries.flatten() {
    let name=entry.file_name().to_string_lossy().to_lowercase();
    if !name.starts_with("nvrtc") || !name.ends_with(".dll") || name.contains(".alt.") {continue;}
    let path:Vec<u16>=entry.path().as_os_str().encode_wide().chain(Some(0)).collect();
    let handle=unsafe {LoadLibraryExW(path.as_ptr(),std::ptr::null_mut(),0x1100)};
    if !handle.is_null(){loaded.push(handle as usize);}
   }
  }loaded
 });
}
#[cfg(not(windows))]
pub(super) fn prepare_libraries() {}
