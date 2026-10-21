# Optional native Metal runtime (2026-10-20)

Apple M5 Max (macOS 27), CubeCL 0.10 / wgpu 29, release addon built without `VULKAN_SDK`. Six layout fixtures with `node scripts/test-pcb-layout.mjs <fixture>` in default mode. "CPU" is the new default (Metal disabled); "Metal" sets `PCB_GPU_METAL_DISABLED=0`. Configurations were interleaved per fixture, two passes each, on a quiet machine. No fixed-hasher patch: CubeCL's MSL is not deterministic between processes, so Metal's own shader cache misses for most kernels and every process prepares its kernels again.

## Wall time (seconds)

| Fixture | CPU | Metal | Change |
|---|---:|---:|---:|
| H743 | 131.5 / 132.0 | 136.1 / 136.0 | +3.3 % |
| ICM20948_v2 | 61.0 / 60.5 | 46.1 / 46.2 | −24.0 % |
| ThunderF722 | 86.6 / 88.5 | 80.2 / 81.2 | −7.8 % |
| ELRS | 16.3 / 15.7 | 16.3 / 15.9 | 0 (no GPU work) |
| ESpower | 19.5 / 19.6 | 19.4 / 19.8 | 0 (no GPU work) |
| STM32G0_CAN_RS485 | 34.5 / 35.1 | 34.3 / 34.3 | 0 (no GPU work) |
| **Total** | **349.4 / 351.4** | **332.4 / 333.4** | **−4.9 %** |

`placement.json` and `layout.json` are identical between CPU and Metal on all six fixtures in both passes (ignoring the "N ms" text).

## Start-up and shader preparation (one profiled Metal pass, `PCB_BLOCK_SOLVER_PROFILE=1`)

| Fixture | Runtime initializations | Kernels | MSL generation | First-launch host time |
|---|---:|---:|---:|---:|
| H743 | 1 | 17 | 0.24 s | 2.39 s |
| ICM20948_v2 | 1 | 17 | 0.25 s | 1.11 s |
| ThunderF722 | 1 | 17 | 0.24 s | 1.31 s |

The first-launch host time includes Metal's library and pipeline compilation. ELRS, ESpower and STM32G0_CAN_RS485 have no block above the Metal threshold, so the runtime is never initialized there. On H743 this preparation is larger than what the GPU saves, which is why it is slightly slower. With the fixed-hasher patch (not included) the cached preparation took 0.66 s per process.

## Policy (`auto` on Metal only)

- A block goes to Metal only in the initial stage (`deferPairs`) and with at least 80 netted connection points, counted before the group endpoints are attached.
- In a batch, only the eligible hypothesis with the most netted points (then more movable primitives) gets the GPU; the others run on the CPU threads.
- Board packing and post-place refinement stay on the CPU in `auto`.
- CUDA and Vulkan, including Vulkan on Apple hardware, keep their existing policies. An explicit `cubecl` backend is unaffected.
