# Adaptive GPU manager: final implementation and acceptance, 2026-10-02

## Delivered behavior

- No CPU-core-count restriction on GPU eligibility. Existing input/capability and
  minimum useful-work guards remain.
- One production Board Packager call with pair-normalized weak positional
  alignment. Full beam/local/repair phases remain; no second cosmetic search.
- One shared admission manager, FIFO within a process, initial depth four,
  adaptive range one to eight, owned in-flight buffers, observed driver memory
  headroom and load-based CPU admission only in `auto`.
- CUDA before Vulkan, with actual strict-F32 execution probes. CUDA requires the
  installed driver, NVRTC and headers. Missing dependencies allow Vulkan; no
  usable GPU means full CPU execution. Explicit runtime override is diagnostic.
- Block batch and board beam waiters lend bounded CPU slots to independent jobs.
  Refiner pipelines GPU scoring and CPU route evaluation even with one CPU slot.
  These are bounded compensating threads, not movable Rust async continuations.
- Soft memory pressure reduces batch size/admission and trims idle workspaces.
  Actual device/allocation failure discards GPU results and restarts the original
  call on CPU, with a fresh refiner time budget. No repeated GPU retries.

CUDA and Vulkan share domain kernels through the same runtime-dispatch adapter.
The small vendored CUDA patch and installation requirements are described in
[CUDA_INTEGRATION.md](CUDA_INTEGRATION.md).

## Checks

The first diagnostic release revealed two integration defects, both corrected:
LLVM folded subnormal CPU probe constants without respecting runtime FTZ, and
CubeCL emitted invalid C++ reference bitcasts for infinity constants. Failed
runs performed no accepted GPU search and were repeated after the fixes.

Before the final frontier-wait/CPU-fallback-lane corrections:
- Board CUDA: five selected tests passed (full results, compound geometry,
  six CPU workers, CUDA/Vulkan discovery, allocation failure recovery).
- Block CUDA: all five focused tests passed (full cycle, one-slot batch
  compensation, world/locked geometry, disabled GPU and failure recovery).
- Refiner CUDA: five selected tests passed, including one/two/four workers,
  different chunks, full-score verification and fresh-budget recovery.
- Alignment: 15 tests passed, including weak pair-normalized production policy.
- Rust: 97 passed / five explicit hardware tests ignored; CPU-only Rust:
  76 passed / two ignored. TypeScript typecheck passed.

Final release evidence and saved one-pass replay measurements follow below.

## Interpretation and limits

Queue/service/readback times accumulate over workers and overlap; they are not
additive components of wall time. NVRTC compilation is included in first-launch
host preparation; it must not be added to that preparation time again.

A GPU has no stable externally observable count of free hardware "workers".
The controller adapts admitted operations using useful throughput, latency and
memory observations. It does not equate four operations to four hardware cores.
Headroom is a sampled driver estimate with a dynamic margin, not a reservation.
Unknown telemetry stays unknown. One operation remains admissible for progress
when cached allocations occupy the reported budget.

Other processes are coordinated by an OS ownership lock at whole-call granularity;
there is no detached broker process, cross-process strict FIFO guarantee, or
externally cancellable native-call API. Panics/failures join scoped workers and
release operation/call guards. Other applications can still consume VRAM after
an observation; injected allocation failure verifies recovery, not an actual
machine-wide VRAM exhaustion experiment.

Validation here is Windows / RTX 3060 Laptop GPU. Linux/macOS, HIP and Metal are
not certified. This package implements CUDA and Vulkan, with CPU as recovery.
Adding another strict-F32 CubeCL backend requires its own probes and domain tests.

The historical esp32c3 integration capture passed geometry and used one board
call, but its assertion requiring a particular R4/R5 swap failed. The same saved
refiner input reproduced that outcome on CPU. Its fixture was not edited and its
quality acceptance remains open. The old PortableScope native board reference
already contains nine hard violations; preserving it is regression evidence,
not proof that the complete board is physically valid. No new PortableScope
capture or router migration belongs to this validation.

## Large-input allocation failure and correction

The first saved PortableScope CUDA attempt completed beam in 53.555 s (six CPU
slots, 14 waiting lanes, peak six active CPU slots, 8,976 suspensions), then failed
at a 402,620,416-byte allocation in local improvement. It is a failed attempt,
not an accepted time or a performance sample. No CPU rerun was started manually.

Inspection found CubeCL CUDA's default 128 stream-local allocator pools combined
with short-lived beam worker threads, and an `unwrap` on failed allocation that
left an unbound handle. Subsequent handle resolution panicked repeatedly and the
process aborted. The prior manager-level injection did not cover this backend
failure path; it is insufficient as the only OOM test.

The CUDA patch now limits its allocator streams to the queue's maximum eight,
cleans unused pages across those pools, and latches allocation failure before any
unbound handle is consumed. Read/sync returns that error to whole-call CPU recovery.
A separate backend-level allocation injection exercises this exact path after live
arithmetic probes. Board phase boundaries release obsolete parent caches, resident
copies and idle scratch before the next full phase; scoped live jobs remain owned.
The failed replay is repeated only after these changes, using the same saved input.

Final-artifact validation note: the six-worker degenerate fixture has only one
beam state, so it legitimately records zero scheduler suspensions. An added test
assertion requiring a positive count there was removed after it failed; CPU cap,
zero leaked slots and the complete CPU/GPU output comparison remain checked.
The saved PortableScope beam provides the independent-work suspension evidence
(8,976 suspensions at peak six active CPU slots). Six other final board scenarios
passed, including multi-process explicit admission, stage recovery, complex terms,
shortlisting and deterministic 1/2/4-worker results; the corrected budget assertion
passed separately. No geometry assertion or fixture was weakened.

## Saved USB diagnostic

The next saved two-resistor USB block exposed an existing CubeCL DSL issue:
`let side = if condition { 0usize } else { 1usize }` compiled to constant side one.
For an external relation with the present endpoint on side zero, this selected
missing primitive -1. CUDA Compute Sanitizer identified an out-of-bounds read in
`cheap`; CPU recovery returned the exact saved result, but this was not accepted
as a GPU replay. The branch now assigns an explicit runtime scalar. A focused
regression covers external relations in both directions with full-score checking.
This changes the block kernel only; the successful PortableScope board timing
above is retained without repeating the unaffected large-board benchmark.

## Accepted PortableScope native Board Packager replay

One successful replay after the allocation fix, input SHA256
`c3c65eb88b9fa1e304f52be0f63a38aa7d3fad5083f4a5c4ad330920b7f511fe`:
`debugging/pcb-layout/replays/board/2026-10-02T16-28-45-082Z-board`.

| Measurement | Result |
|---|---:|
| Saved previous GPU native call | 157,271.528 ms |
| New CUDA complete native call | 60,071.566 ms |
| Observed ratio, previous GPU / new GPU | 2.62x |
| Beam | 51.108 s |
| Local improvement | 8.431 s |
| Hard repair | 0.107 s |
| GPU score batches / candidates | 2,202 / 5,203,014 |
| NVRTC compilation, included in native wall time | 1.729 s / 14 kernels |
| First-launch host preparation (overlaps compilation) | 1.866 s |
| CPU active cap / measured peak / waiting lanes | 6 / 6 / 14 |
| CPU slot suspensions | 9,021 |
| Admission initial/final depth / decisions | 4 / 6 / 68 |
| Accumulated queue wait across workers | 97.487 s |
| Accumulated beam readback across workers | 106.803 s |
| Driver usage at final sample / free headroom | 1.096 / 5.346 GB |

The complete solution exactly equals the saved solution: hardCount=9,
hardSeverity=31.619892120361328, score=31773864, all placements and locked poses
unchanged. No fallback, state ready, zero active/queued operations at completion.
The 2.62x figure is an observed comparison to the old GPU capture, not a new CPU
comparison or a controlled warm/cold microbenchmark. It includes backend and
manager changes together. The old capture had a reused runtime; the new call
includes startup. Neither number is total PortableScope assembly time, and the
baseline's pre-existing violations are not resolved by this infrastructure task.

Measured addon: build key `0b7de24522ee5b1d`, 23,447,552 bytes (22.36 MiB), SHA256
`14f7627f786ccbbfca696cf92aea0c24b700290913dc71e344f4d1ef4d973874`.
The subsequent USB-only endpoint-index correction produces a different final
artifact; this large-board measurement is not repeated merely for a new build key.

## Final checks and artifact

- Final USB replay under NVIDIA Compute Sanitizer: `2026-10-02T16-39-10-934Z-USB_Termination`,
  12 score batches / 644 candidates, CUDA throughout, exact saved output,
  hardCount=0, score=236.94454956054688, **0 sanitizer errors**. Instrumented
  native wall time 12,811.160 ms includes 7,638.122 ms NVRTC compilation; it is
  correctness evidence, not a normal execution-time benchmark.
- Final block regressions passed: one-slot two-job compensation/recovery and
  external relations in both directions with complete GPU score verification.
- Final refiner replay `2026-10-02T16-41-04-698Z-refine`: 1,553.514 ms cold native
  call, CUDA throughout, three batches / 129 candidates, same moves and exactly
  the same placements as saved. Final score differs by one F32 ULP:
  54751.796875 -> 54751.80078125 (the previously saved CPU reference value).
  The JSON equality flag is therefore false; no geometric change or search
  decision change occurred. NVRTC 927.205 ms overlaps first-launch preparation
  979.745 ms; this small cold call is not claimed faster than a warmed reference.
- Final runtime tests pass for CUDA without Vulkan, Vulkan without CUDA, and
  CUDA-server allocation failure returning the complete CPU result without
  invalid-handle panics. The backend error test passed both before and after the
  block-only correction.
- Final Rust: 97 passed / five hardware-only tests ignored; CPU-only Rust earlier
  in this change: 76 passed / two ignored. TypeScript typecheck and diff whitespace
  check pass. Release build passes; existing unrelated warnings remain.
- Final one-addon artifact: 23,448,576 bytes (22.36 MiB), build key
  `c67e87a607b9c22a`, SHA256
  `fe98a2fc65152a67f9566e4576c7347e6ad5806ec48f65bf2cdaaef3c801d6f5`.
  This exceeds the preferred approximately 20 MiB goal; no helper DLL is shipped.
  CUDA/NVRTC still must be installed to select CUDA.

No fresh CPU run of the saved USB, PortableScope or refiner input was requested;
existing references were reused. No PortableScope capture was run. Diagnostic
reruns are tied above to concrete failures/fixes, not a median campaign.
