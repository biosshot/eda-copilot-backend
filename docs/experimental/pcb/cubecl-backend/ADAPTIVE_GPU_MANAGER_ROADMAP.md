# Adaptive GPU manager and single board search

Status: active, approved 2026-10-02. Do not mark complete without evidence.

## Agreed scope

One shared manager for block, board and post-place GPU work: device/backend discovery,
availability, queue admission, dynamic memory headroom, per-workload timing, batching,
completion and CPU recovery. Prefer compatible CUDA, then Vulkan, then other supported
CubeCL backends; Vulkan must not be mandatory. Preserve the F32 contract, native CPU
replay, one platform addon and the approximately 20 MiB size goal. Backend availability
includes compiler/runtime libraries and numerical conformance, not just device name.
No router migration or switch to geometric routing in this roadmap.

Start from a useful middle-sized configuration derived from task/device limits, not
one tiny batch. Adapt on comparable useful work per second and completion latency;
unchanged throughput plus growing latency is not a reason to add concurrency. Separate
batch size, submitted depth and CPU worker count. Observe actual backend memory budget
when available; unknown headroom must remain unknown, never total VRAM masquerading as
free VRAM. Reuse allocations; retain dynamic safety headroom for concurrent system use.

## Ordered implementation and acceptance

- [x] P1: Remove CPU-worker-count/readiness restrictions from GPU eligibility and the
  board auto four-worker cap. Keep input/device guards, positive work checks and CPU
  budget bounds. Test admission with more than four workers and absent GPU.
- [ ] P2: Run exactly one complete Board Packager with a weak alignment contribution,
  normalized against pair count; preserve ordinary candidate coverage and hard rules.
  No second full search or expensive atomic pair search solely for cosmetic alignment.
  Retain local improvement/repair and existing block portfolio. Test one invocation,
  hard geometry, locked poses, weak alignment and full-cycle saved input.
- [ ] P3: Implement backend-neutral runtime selection and strict F32 implementations.
  Audit pinned CubeCL 0.10 compiler requirements, driver discovery and deployment.
  Prefer CUDA when usable, then Vulkan and supported platform alternatives. Actual
  execution probes and unsupported-backend fallback must be tested. No silent claim
  that building one backend validates other OSes. Record addon size/dependencies.
- [ ] P4: Central GPU manager with explicit bounded queue, owned per-flight scratch,
  completion handles, cancellation/failure cleanup and fair admission. Protect buffer
  lifetimes until readback. Account for subprocess ownership, not only Rust threads.
  No mutex held around all GPU execution/readback; no detached work escaping recovery.
- [ ] P5: Cooperative CPU task scheduling: preparing independent work/CPU scoring while
  GPU requests are pending; deterministic results independent of completion order.
  Preserve dependencies between beam levels and existing full-call recovery.
- [ ] P6: Adaptive admission from a middle starting point using memory and measured
  throughput/latency. Record queued, running, completed, queue/service/readback time,
  byte estimates, available budget and each controller decision. Controlled tests for
  congestion, changing workloads, unavailable memory telemetry and bounded growth.
- [ ] P7: Load-based CPU selection only in auto. Explicit cubecl queues GPU work; device
  absence/real failure still uses agreed full original-input CPU recovery. Avoid
  per-candidate thrashing or automatic retries after partial GPU failure.
- [ ] P8: One-pass complete validation on representative saved inputs, plus full board
  quality/time when appropriate. Reuse saved CPU references; do not run median suites.
  Distinguish GPU execution, waiting, worker accumulation and full pipeline wall time.
  Record all remaining limitations instead of closing incomplete milestones.

## Starting evidence

PortableScope 2026-10-02T08-30-42-388Z: 824.728 s complete capture, invalid final geometry
(11 overlaps). Board ordinary 157.180 s, aligned 175.168 s; aligned proposal rejected.
Block gpu_readback 212.790 s over 15096 calls includes wait/execution, not pure transfer.
Existing shared scratch requires holding runtime mutex through readback. Refiner already
pipelines GPU production with CPU route consumers; reuse this rather than duplicate it.
Current implementation explicitly requires Vulkan/SPIR-V. These are starting constraints,
not completed cross-platform support or proof of future speedup.


## Implementation checkpoint (2026-10-02)

Implemented P1 admission/worker-budget changes and P2 single-search orchestration.
Weak alignment uses weight 1 divided by pair count, tolerance 0.15 mm and zero
orientation weight. Explicit orientation policies retain their native atomic pair
path; the production cosmetic positional policy does not activate it.

P4/P6 foundations are implemented, not complete: one process-wide FIFO admission
queue, initial depth 4, exploratory depth range 1..8, throughput/latency comparison
windows, per-flight scratch ownership and reuse. Device-state mutex is released
before dispatch/readback. Operations retain the device lease until completion;
a concurrent runtime failure invalidates sibling results and clears idle scratch.
The manager reports active/queued/completed work, depth, decision reason, queue wait
and accumulated service time. These are not hardware execution timestamps.

Remaining required work: backend-neutral CUDA/platform execution and conformance;
complete device-memory reservation accounting (board/refiner domain caps still apply); async
continuations/cooperative CPU worker scheduling; board Engine mutex removal through
safe domain job ownership; multi-process coordination; auto load-based CPU selection;
representative full-board performance/quality acceptance. Workers still wait in the
admission/readback calls. Do not describe this checkpoint as completion of the roadmap
or claim that four in-flight operations means four physical GPU queues.

Pinned CubeCL 0.10 CUDA uses NVRTC and CUDA/CCCL include discovery. Selecting CUDA
requires more than detecting an NVIDIA GPU; driver, compiler and numerical policy
must all pass. Current production runtime still uses Vulkan/SPIR-V. No CUDA support
is claimed by this checkpoint.

Validation so far:
- Rust: 83 passed, 5 GPU tests ignored in normal suite.
- CPU-only Rust: 76 passed, 2 ignored.
- Two explicit real-GPU runtime tests passed: four overlapping operations using
  identical scratch keys retain independent outputs; scratch growth/reuse and full
  runtime failure cleanup preserve the existing contract.
- TypeScript typecheck passed; 15 alignment tests passed.
- Three focused board GPU tests passed, including auto with six CPU workers and
  original-input CPU recovery after failures at each board stage.
- Release addon: 21,153,792 bytes (about 20.17 MiB), SHA256
  `2fb47c7405bb33d33db523d06cbae81b4bf6f5496581db2116b8680de93143bb`.
  Slightly above the preferred 20 MiB target; no new runtime dependency added.
- One esp32c3 integration capture `2026-10-02T11-38-15-622Z`: 102.63 s, exactly
  one board native request, placementOk=true and zero geometry errors. Exit 1:
  fixture expects the R4/R5 swap, but the changed search produces header and C2/C3
  swaps instead. Do not mark this fixture green or P2 acceptance complete.
- One CPU replay of that exact final refiner input (new reference, 387.712 ms)
  produced identical placements and the same two moves as GPU; final score differs
  by 0.00390625 (CPU 54751.80078125, GPU 54751.796875). This isolates the missing R4/R5
  move from GPU-only execution, but does not settle the fixture's acceptance criteria.
- No PortableScope rerun and no median benchmark suite. Logs remain under ignored
  `debugging/adaptive-gpu-*`. No whole-board speedup is claimed.


## Memory-budget checkpoint (2026-10-02, after 361338c)

Added a backend-neutral `BudgetSource` / cached `Monitor`. The current runtime
implements this using Vulkan `VK_EXT_memory_budget` on the largest device-local
heap; unsupported telemetry stays unknown rather than substituting physical VRAM.
The driver reports a changing per-process allocation budget and estimated usage,
not a reservation or a guarantee that a future allocation will succeed. See the
[Vulkan budget contract](https://docs.vulkan.org/refpages/latest/refpages/source/VkPhysicalDeviceMemoryBudgetPropertiesEXT.html).

Readings refresh at most every 250 ms. Usable headroom subtracts a dynamic 5% of
the driver budget as a race margin, not a fixed application quota. Queue admission
uses per-workload observed scratch peaks and in-flight reservations to reduce
concurrent jobs when memory pressure rises. Unknown readings retain depth-based
admission. One operation can proceed when no other job is active, avoiding a
permanent stall when existing reusable buffers already occupy the available budget.
Allocation guards and original-call CPU recovery remain necessary.

Block engine initialization now derives its allocation ceiling from that reading
and the actual backend allocation limit instead of the fixed 64 MiB ceiling.
This does not remove dispatch limits or increase candidate coverage. Board/refiner
resident/cache limits are still unchanged. Full dynamic sizing, accounting of all
resident/uploads and releasing idle allocations under pressure remain open.
Scratch estimates are conservative: pooled buffers may include older workloads,
and a driver usage reading may already include bytes counted as reservations.
Do not call this precise global free-memory accounting, CPU scheduling, CUDA
support, or completion of P4/P6. Those milestones remain unchecked.

Validation:
- Rust normal suite: 86 passed, 5 hardware tests ignored.
- Real GPU four-operation scratch isolation/readback test passed. Driver budget
  5,479,858,176 bytes; usage 76,791,808 bytes; headroom 5,403,066,368 bytes;
  usable headroom after margin 5,129,073,460 bytes at the sampled instant.
- Unit checks cover unknown telemetry, saturating headroom, overlap restriction
  under pressure, overflow and forward progress with retained scratch.
- Release build succeeded (3m 53s): Windows addon 21,186,560 bytes (~20.21 MiB),
  SHA256 `0981971d4104aa16009319f3445e6ac9c6e898a529a6761538b707dc66fa31cc`.
  Still slightly above the preferred 20 MiB; no new dependency/helper DLL.
- Four focused block GPU tests passed: full-cycle composite score/geometry,
  absent-GPU recovery, injected failure after singles, locked/world constraints.
- Exact saved MCU input `4312292b68` replayed once in 1,749.429 ms with exact
  baseline output, but the existing supported-input guard selected CPU. This is
  recovery evidence, not GPU performance evidence.
- A different supported saved USB_Termination input `c2780c5c38` replayed once:
  GPU, 57,562.822 ms cold process, exact baseline output and rank (zero hard
  violations), 12 GPU batches / 644 candidates. Saved capture had 44.704 ms for
  this call in an already-running GPU process. These are NOT comparable warm/cold
  performance samples and establish no speedup. Current queue wait totals only
  0.0059 ms; almost all elapsed time is in first beam evaluation (57,165.603 ms),
  while singles take 8.844 ms. Cold compilation/pipeline creation is a strong
  hypothesis, not a separately timed measurement in this replay. Preserve the
  shared runtime across work and measure compilation separately in subsequent
  manager work; do not try to hide startup cost using warm-only timings.
- Logs: `debugging/adaptive-memory-*`; exact replays at
  `2026-10-02T12-52-25-289Z-MCU_Decoupling_Reset_Support_Boot_Support_USB_Termination_MCU`
  and `2026-10-02T12-53-20-922Z-USB_Termination`. No input was replayed twice;
  existing baseline outputs were reused and PortableScope was not rerun.


## Cold-start diagnosis (in progress)

The pinned CubeCL 0.10 `CompilationConfig` defaults to no persistent compilation
cache. Its in-memory pipeline map survives only as long as its process/runtime.
The runtime now measures `kernelPreparation.firstLaunchHostMs` (host compilation,
pipeline creation, binding and enqueue on first use) and its nested
`sourceCompilationMs` (CubeCL source compilation plus strict F32 transformation).
They overlap and must not be added; neither is GPU execution time. Optional
`PCB_GPU_PROFILE_KERNELS=1` reports individual first-use kernel names/timings.

A preparation generation plus active-preparation count invalidates controller
windows overlapping cold starts, even when another in-flight job triggered them.
`coldSamplesExcluded` records this exclusion; actual service and wall times remain
reported. Initial admission depth remains four. Rust tests: 87 passed / 5 ignored.
Diagnostic replay `2026-10-02T13-16-32-671Z-USB_Termination` ran once after
instrumentation changed: 68,310.437 ms total, exact baseline output. First-use
host preparation totals 67,540.927 ms; nested source compilation 59,588.116 ms.
`cheap` first-use preparation alone takes 62,878.182 ms; `full` takes 4,064.230 ms.
Queue wait is 0.0056 ms. The driver/pipeline/binding remainder is not separately
attributed as pure driver time. Two startup-overlapping samples were excluded
from controller training and admission depth stayed four.

The next implementation persists CubeCL's serialized SPIR-V representation after
strict F32 transformation, with build fingerprint, device-properties hash, kernel
specialization/execution mode, address type and compilation options in its identity.
The fingerprint covers native sources, Cargo.lock/Cargo.toml, build script, Rust
version, target, profile, Rust flags and enabled Cargo features. In-memory/pinned
CubeCL cache IDs also include the fingerprint, preventing stale code reuse when
an external CubeCL configuration enables its own cache.

The adapter reuses CubeCL's kernel serialization, but owns the small persistence
layer: pinned CubeCL's cache has `expect` on chunk reads/writes and no application
source fingerprint. Here entries are checksummed, size bounded and atomically
renamed; miss/corruption/I/O failure compiles normally instead of failing the solve.
Existing disk entries are only performance data. Cache misses still incur cold
compilation; this does not eliminate first-installation startup or driver pipeline
creation. Shader audits bypass this cache to retain emitted-IR verification.

Cache normally lives under the user's platform cache root in
`eda-copilot/gpu-kernels/<build-key>`. Diagnostics can override the root using
`PCB_GPU_KERNEL_CACHE_DIR` or disable it with `PCB_GPU_KERNEL_CACHE_DISABLED=1`.
No helper binary/dependency is added. Rust cache tests: 89 passed / 5 ignored.
Cache-state validation (each one pass, existing output reused):

| Exact USB_Termination input | Native wall time | Source compilation | Cache |
| --- | ---: | ---: | --- |
| Empty cache, new process | 63,939.011 ms | 54,689.188 ms | 11 misses, 11 writes |
| Populated cache, another new process | 516.819 ms | 0 ms | 11 hits, zero misses |

Both runs preserve the complete saved output, including rank and checkpoints.
No CPU reference rerun. Numeric runtime probes also run successfully from cached
kernels. First-launch host preparation drops from 63,116.969 to 52.021 ms;
these figures include nested compilation and are not pure GPU execution times.
The warmed-on-disk cache contains 11 entries / 1,289,929 bytes. No cache errors.
This proves avoiding repeated cold preparation for this input; it does not establish
whole-board speedup, eliminate the initial cache fill, or benchmark other GPU models.

Replay artifacts:
- `2026-10-02T13-28-01-044Z-USB_Termination` (empty cache).
- `2026-10-02T13-29-40-756Z-USB_Termination` (persisted cache in a new process).
- Logs and isolated cache: `debugging/adaptive-cache-*` and
  `debugging/adaptive-kernel-cache/`; no PortableScope capture rerun.

Release addon: 21,261,824 bytes (~20.28 MiB), SHA256
`e04844f45691ecc0a476bff50a22ee64da537820d05cc6296ef45fc2f2a22ede`.
Build fingerprint `ce83f754148296ab`; no new runtime dependency or binary.
Focused GPU block test preserves locked poses and world bounds and passes.
The explicit real-GPU four-operation scratch isolation/readback test also passes.
CPU-only Rust: 76 passed / 2 ignored. Full GPU-feature Rust: 89 passed / 5 ignored.
The original P3/P4/P5/P6/P7/P8 milestones remain open; this checkpoint adds startup
cost control and cross-process kernel reuse, not CUDA or cooperative CPU scheduling.
