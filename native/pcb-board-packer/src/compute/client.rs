//! A backend is selected once; handles never migrate between clients.
use cubecl::{client::ComputeClient,Runtime,server::Handle,bytes::Bytes,future::DynFut,ir::{DeviceProperties,features::Features}};
use cubecl_runtime::server::ServerError;
#[derive(Clone)]
pub(crate) enum Client {
 Vulkan(ComputeClient<super::f32_runtime::PcbRuntime>),
 Cuda(ComputeClient<super::cuda_runtime::PcbCudaRuntime>),
}
macro_rules! with_client {
 ($source:expr, |$client:ident,$runtime:ident| $body:block) => {{
  match &$source {
   $crate::compute::client::Client::Vulkan($client)=>{type $runtime=$crate::compute::f32_runtime::PcbRuntime;$body},
   $crate::compute::client::Client::Cuda($client)=>{type $runtime=$crate::compute::cuda_runtime::PcbCudaRuntime;$body},
  }
 }};
}
pub(crate) use with_client;
impl Client {
 pub fn backend(&self)->&'static str {match self {Self::Cuda(_)=>"cuda",Self::Vulkan(_)=>"vulkan"}}
 pub fn properties(&self)->&DeviceProperties {match self {Self::Cuda(c)=>c.properties(),Self::Vulkan(c)=>c.properties()}}
 pub fn features(&self)->&Features {match self {Self::Cuda(c)=>c.features(),Self::Vulkan(c)=>c.features()}}
 pub fn create_from_slice(&self,data:&[u8])->Handle {match self {Self::Cuda(c)=>c.create_from_slice(data),Self::Vulkan(c)=>c.create_from_slice(data)}}
 pub fn empty(&self,size:usize)->Handle {match self {Self::Cuda(c)=>c.empty(size),Self::Vulkan(c)=>c.empty(size)}}
 pub fn read_async(&self,handles:Vec<Handle>)->DynFut<Result<Vec<Bytes>,ServerError>> {match self {Self::Cuda(c)=>{let c=c.clone();Box::pin(async move{c.read_async(handles).await})},Self::Vulkan(c)=>{let c=c.clone();Box::pin(async move{c.read_async(handles).await})}}}
 pub fn read_one(&self,handle:Handle)->Result<Bytes,ServerError> {super::cpu::waiting(||cubecl::future::block_on(self.read_async(vec![handle])).map(|mut b|b.remove(0)))}
 pub fn memory_cleanup(&self) {match self {Self::Cuda(c)=>c.memory_cleanup(),Self::Vulkan(c)=>c.memory_cleanup()}}
}
pub(crate) fn read_one<R:Runtime>(client:&ComputeClient<R>,handle:Handle)->Result<Bytes,ServerError> {
 super::cpu::waiting(||client.read_one(handle))
}
