# Local CubeCL CUDA integration (0.10.0)

The source under `native/pcb-board-packer/vendor/cubecl-cuda` is the published
`cubecl-cuda 0.10.0` crate (Apache-2.0 OR MIT); both upstream licenses are retained.
The Cargo patch is deliberately limited to this crate. No CUDA DLL is redistributed.

Local changes:
- Export `CudaServer` so the application can wrap it with strict task identity,
  shared startup telemetry and the existing owned-operation interface.
- Disable CubeCL CUDA fast math. Compile NVRTC programs with `--fmad=false`,
  `--ftz=true`, `--prec-div=true`, `--prec-sqrt=true`.
- Destroy every NVRTC program through RAII, including failed compilations, and
  expose compilation count/time. These times overlap first-launch host time.

The application wrapper in `compute/cuda_runtime.rs` also corrects CubeCL 0.10's
invalid C++ constant bitcast (`reinterpret_cast<float const&>(uint32(...))`):
CUDA `__uint_as_float(uint32(...))` preserves the exact bits while accepting an
rvalue. This retains infinity sentinels; it does not substitute finite penalties.
The replacement is restricted to this exact scalar uint32-to-float expression.
Generated CUDA may be audited with `PCB_F32_SHADER_AUDIT_DIR`.

Task/cache identity includes all native and vendored sources, dependency lock,
toolchain, target and build flags. A rebuilt strict compiler cannot reuse stale
kernels. Live release probes check rounding, signed FTZ and non-contraction before
a solver is admitted. CPU probe operands are black-boxed to prevent LLVM constant
folding from bypassing the thread's runtime FP environment.

Windows CUDA discovery uses installed driver/NVRTC libraries and toolkit headers,
including CUDA 13's `bin/x64` directory. A driver alone is insufficient for this
CubeCL NVRTC path. Missing dependencies reject CUDA and allow Vulkan selection;
if no compatible runtime remains, the original call runs on CPU. Linux follows
the library loader and CubeCL header discovery; it has not been tested here.

The large PortableScope replay additionally exposed upstream's allocation unwrap
and excessive default stream-pool count. Local `compute/server.rs` changes cap
CUDA allocator streams at eight (the shared admission maximum), clean all those
pools, and latch reserve failures before any unbound handle can be resolved.
`PCB_CUDA_FAIL_ALLOCATION_AFTER` injects this server-level error after a specified
allocation count, independently of the manager-level failure test. It is diagnostic
only; no buffers are allocated merely to exhaust the user's machine.
