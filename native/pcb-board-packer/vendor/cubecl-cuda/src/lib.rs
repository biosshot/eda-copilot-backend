#[macro_use]
extern crate derive_new;
extern crate alloc;

mod compute;
mod device;
mod runtime;

pub use device::*;
pub use runtime::*;
pub use compute::CudaServer;

#[cfg(feature = "ptx-wmma")]
pub(crate) type WmmaCompiler = cubecl_cpp::cuda::mma::PtxWmmaCompiler;

#[cfg(not(feature = "ptx-wmma"))]
pub(crate) type WmmaCompiler = cubecl_cpp::cuda::mma::CudaWmmaCompiler;

pub mod install {
    use std::path::PathBuf;

    pub fn include_path() -> PathBuf {
        let mut path = cuda_path().expect("
        CUDA installation not found.
        Please ensure that CUDA is installed and the CUDA_PATH environment variable is set correctly.
        Note: Default paths are used for Linux (/usr/local/cuda) and Windows (C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/), which may not be correct.
    ");
        path.push("include");
        path
    }

    pub fn cccl_include_path() -> PathBuf {
        let mut path = include_path();
        path.push("cccl");
        path
    }

    pub fn cuda_path() -> Option<PathBuf> {
        if let Ok(path) = std::env::var("CUDA_PATH") {
            return Some(PathBuf::from(path));
        }

        #[cfg(target_os = "linux")]
        {
            // If it is installed as part of the distribution
            return if std::fs::exists("/usr/local/cuda").is_ok_and(|exists| exists) {
                Some(PathBuf::from("/usr/local/cuda"))
            } else if std::fs::exists("/opt/cuda").is_ok_and(|exists| exists) {
                Some(PathBuf::from("/opt/cuda"))
            } else if std::fs::exists("/usr/bin/nvcc").is_ok_and(|exists| exists) {
                // Maybe the compiler was installed within the user path.
                Some(PathBuf::from("/usr"))
            } else {
                None
            };
        }

        #[cfg(target_os = "windows")]
        {
            return Some(PathBuf::from(
                "C:/Program Files/NVIDIA GPU Computing Toolkit/CUDA/",
            ));
        }

        #[allow(unreachable_code)]
        None
    }
}

#[cfg(test)]
#[allow(unexpected_cfgs)]
mod tests {
    pub type TestRuntime = crate::CudaRuntime;

    pub use half::{bf16, f16};

    cubecl_core::testgen_all!(f32: [f16, bf16, f32, f64], i32: [i8, i16, i32, i64], u32: [u8, u16, u32, u64]);
    cubecl_std::testgen!();
    cubecl_std::testgen_tensor_identity!([f16, bf16, f32, u32]);
    cubecl_std::testgen_quantized_view!(f16);
}

// PCB integration: NVRTC compilation includes native compilation, not just IR generation.
static PCB_NVRTC_NS:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
static PCB_NVRTC_COUNT:std::sync::atomic::AtomicU64=std::sync::atomic::AtomicU64::new(0);
pub fn pcb_nvrtc_statistics()->(u64,u64) {
 use std::sync::atomic::Ordering::Relaxed;
 (PCB_NVRTC_COUNT.load(Relaxed),PCB_NVRTC_NS.load(Relaxed))
}
pub(crate) struct PcbNvrtcProgram(pub cudarc::nvrtc::sys::nvrtcProgram,pub std::time::Instant);
impl Drop for PcbNvrtcProgram {
 fn drop(&mut self) {
  let _=unsafe {cudarc::nvrtc::result::destroy_program(self.0)};
  PCB_NVRTC_NS.fetch_add(self.1.elapsed().as_nanos().min(u64::MAX as u128) as u64,std::sync::atomic::Ordering::Relaxed);
  PCB_NVRTC_COUNT.fetch_add(1,std::sync::atomic::Ordering::Relaxed);
 }
}
