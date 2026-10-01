# Post-place / Refiner GPU evidence

2026-10-01. Implemented and accepted for the measured workload after Board
Packager commit `c17b564`. The complete native Telemetry refiner improves
1.37x with one CPU worker and 1.25x with four workers and a ready shared GPU.
This is not a claim of universal full-board acceleration. Artifacts live under
`debugging/post-place-gpu-2026-10-01/`; `baseline/` preserves the original
addon and source hashes. CPU F64 remains the reference.

The user requires one pass per test/version, reusing compatible saved CPU
references. No median suite or repeated warm-up series is required. Full CPU
recovery restarts the original input with its original timeout budget anew.

## Final result and provenance

Windows, Ryzen 5 5600H (6 cores / 12 logical CPUs), NVIDIA RTX 3060 Laptop GPU.
`build-final/manifest.json` records all native source hashes and addon SHA-256
`4bf83e7e051fe83c0cd38b2c6b924885c3927c9811eaaec1159e2fc1a266799c`.
The source hashes and installed addon were checked again after testing with no
mismatch. The build is based on `c17b564` plus the Refiner working-tree changes;
that commit alone does not contain this implementation.

One `.node`: **20,088,320 bytes**, 20.088 MB / 19.158 MiB. This slightly exceeds
a strict decimal 20 MB, but remains within the approximate size target. No
new package/runtime dependencies or helper binaries. `imports-final.json`
records the same Windows system DLL imports as before. GPU and CPU stay F64.

| Complete native Telemetry call | CPU wall | GPU wall | CPU / GPU | GPU condition |
|---|---:|---:|---:|---|
| 1 worker | 17.633576 s | 12.886720 s | **1.368x** | Cold runtime, all startup included; final build, auto |
| 4 workers | 7.053376 s | 5.649389 s | **1.249x** | Shared runtime initialized by one Board call; final build, auto |
| 6 workers, admission experiment | 4.823792 s | 5.180139 s | 0.931x | Build 3, ready runtime; auto therefore keeps CPU |

Every row uses identical original native inputs within its CPU/GPU pair,
completes five passes / five accepted moves without timeout, and retains exact
results and hard-constraint validity. References: `Telemetry-cpu/`,
`Telemetry-cpu4/`, `Telemetry-cpu6/`; GPU: `final-Telemetry-gpu1/`,
`final-Telemetry-gpu4/`, `Telemetry-gpu6-budget/`. Each CPU input was run once;
the final GPU build reuses those results. Four-worker initialization was a
separately logged existing esp32c3 Board call (`warmBoard`), excluded from the
refiner timer, representing the shared runtime after board packing. It is not
a cold-start result. A one-pass measurement has no statistical confidence
interval; route-stage timing changes are not attributed to GPU kernels.

Single-worker stage totals (milliseconds):

| Stage | CPU | GPU | Meaning |
|---|---:|---:|---|
| Generation | 3.097 | 2.429 | Original CPU moves |
| Candidate legality/geometry | 189.950 | 133.509 | Original CPU violation identities |
| Score | 4831.273 | 2597.419 | About 1.86x; GPU includes startup/dispatch/readback |
| Route baseline + comparisons | 12447.210 | 10017.970 | CPU remains; one-pass variation, not a GPU route speedup |
| Iteration evaluation wall, sum | 17497.024 | 12765.609 | Includes all five evaluations |
| Full public native call | 17633.576 | 12886.720 | Includes initial/final work and boundary overhead |

Four-worker score time decreases from 5755.204 summed CPU worker ms to
1637.680 GPU-stage ms; these are overlapping resources, **not a stage wall
speedup**. Summed iteration wall is 6928.725 -> 5521.466 ms. GPU operations
are 1602.595 ms, 20 batches / 2175 legal scores; generation 2.689 ms, resident
static data 331,036 bytes, shared workspace high-water mark 14,076,440 bytes.
The remaining majority is Micro-A*, so this module cannot inherit the block
solver's much larger multiplier merely by accelerating score.

## Admission and recovery

`PCB_POST_PLACE_BACKEND=cpu|cubecl|auto`, default `auto`, applies to the native
refiner; standalone scalar scoring remains CPU. Auto requires at least 154
components, 660 pads and 235 MST segments, positive iterations, at least 2000 ms
budget, and either one worker or at most four workers with a ready shared GPU.
These conservative thresholds come from the measured workload; they are not
a model guaranteeing acceleration on every board or device. Six-worker CPU
is faster than the tested four/six-worker GPU configurations, so auto does not
reduce the available CPU budget simply to force GPU use. Small local blocks
and ESPower/esp32c3 stay CPU automatically. Explicit `cubecl` still applies
capability, numeric and capacity guards.

Each Engine retains static topology/templates and uploads current poses plus
compact changes. Parallel materialization, independent net/candidate MST and
segment rows feed ordered F64 reduction. Default chunks contain 128 candidates
(configurable 1..256), bounded by memory and dispatch capacity. Scores stream
to original CPU route groups. Shared CPU execution permits cover the producer
and route workers; GPU readback releases the producer's CPU permit. Original
candidate order, group baseline caches and serial incumbent rules are retained.

`PCB_POST_PLACE_GPU_VERIFY=1` enables independent geometry/MST/term/aggregate
checks, excluded from performance measurements. `PCB_POST_PLACE_GPU_FAIL` accepts
`batch`, `after_move`, `diagnostics` for recovery tests. Runtime failure discards
the GPU attempt, releases its lease/scratch and repeats the complete original
native input once on CPU with the **full original timeout anew**. Partial GPU
poses/moves never seed that replay. No-device, disabled, busy, unsupported layer,
negative MST weights, duplicate net names, unsafe numbers and capacity limits
retain the CPU path. Final logs report selected backend and recovery reason.

## Correctness, budget and integration acceptance

- `final-ESPower-verify/`: full final native refinement, 595 legal candidates,
  seven batches/passes, all geometry/MST/term checks, exact CPU result, valid
  placement. Diagnostic 2.393 s includes independent CPU validation and is not
  a speed measurement.
- `final-FPGA-local-verify/`: complete nineteen-primitive FPGA local refinement,
  198 legal candidates, two batches/passes, one accepted move, exact CPU result
  and valid geometry. Diagnostic 0.589 s vs saved CPU 0.070 s is not a speed
  claim; small locals remain CPU in auto. Three USB native inputs and the
  six-primitive FPGA capacitor group are separately preserved.
- `final-budget-cpu/` / `final-budget-gpu/`: one geometric-mode Telemetry probe,
  one worker, eight requested passes, 2000 ms native budget. CPU: 2.074 s public
  wall, 1102 candidates, three moves; GPU: 2.079 s, 2800 candidates, six moves.
  Both time out with valid hard constraints. Independent CPU final scores are
  **15,857,011.150 -> 15,851,113.650** (lower is better), recorded in
  `budget-quality.json`. This is a controlled geometric workload on a saved
  board input, not an actual 154-component local block. It proves more useful
  work / better score at the same budget; it is not a wall-time multiplier.
  Timeout is cooperative, not a promise to interrupt an in-flight kernel.
- `final-ESPower-board/`: complete 53-component assembly with forced Board and
  Refiner GPU, placementOk=true; final placements and SVG exactly match saved
  `board-gpu-2026-10-01/optimized-final-ESPower-board/`. Eighteen local/final
  native refine calls total 2.398 s, final call 1.033 s; all use CubeCL without
  fallback on one shared runtime. Full assembly is 46.532 s vs saved 45.408 s:
  forcing tiny locals onto GPU adds overhead, so **no full-board speedup is
  claimed** here. See `full-board-comparison.json`.
- `final-esp32c3-board/`: complete 20-component default-auto assembly, 8.408 s,
  placementOk=true; final placements and SVG exactly match the saved CPU
  capture. All seven local/final refine calls select CPU as intended. See
  `esp32c3-comparison.json`. Neither full-board run repeats the old CPU build.

Validation performed against the final addon:

- 29 focused refiner/route-aware/local/capture tests, then one forced actual-GPU
  Micro-A* route-only winner test, three existing Board/block shared-runtime
  regressions and one additional domain/no-device recovery test: **34 passing
  test executions**. Chunk sizes 1/17/128 and workers 1/2/4 retain selection;
  failures at three stages, zero/small timeout, fixed permissions and opposite
  pad sides are covered. Process contention and release while the failed owner
  remains alive are exercised. No-device uses an unavailable Vulkan driver;
  this does not certify every real unsupported adapter.
- Rust GPU build: **64 passed, 2 GPU tests ignored**; CPU-only build:
  **60 passed, 1 ignored**. Actual device coverage comes from the Node tests.
- `npm run native:build`, `npm run typecheck`, `npm run build`, source/addon
  provenance, imports and whitespace checks. Relevant logs are `final-*.log`
  and `final-verification.json` under the artifact root.

The focused tests replace broad repeated block/board benchmark suites under
the user's one-pass policy. Full Telemetry/PortableScope TypeScript assembly
was not repeated for this Refiner change. A timed 30-second route-heavy full
board, exhaustive extreme-number boundary matrices and other GPU hardware
remain unmeasured; this report does not certify them or fix historical RF/USB
placement debt. The native same-work improvement and the two complete-board
quality checks above are the accepted scope.

## Existing contract and first baseline

Both callers use the same native refiner. Local blocks request geometric
scoring, eight passes and a two-second budget; final boards use route-aware
scoring and the adaptive iteration count within 30 seconds. The scalar scorer
remains CPU. Refiner geometry permits only a subset of original violation
identities, not merely a non-increasing count. Routing baselines, admissible
improvement bounds, feasibility ordering and stable candidate reduction stay
on CPU; there is no arbitrary geometric top-K before route evaluation.

`ESPower-cpu/` and `Telemetry-cpu/` encode the unchanged saved pre-refinement
placements from `capture-*-cpu/placement/stages/02-v2-legalize.json`. They run
one exact native call with one worker, without rebuilding their boards.

| Scope | CPU wall | Score worker time | Route worker time | Work completed |
|---|---:|---:|---:|---|
| ESPower final | 1.327 s | 0.135 s | 1.150 s | 833 candidates, 7 passes, no improvement |
| Telemetry final | 17.634 s | 4.831 s | 12.447 s | 2325 candidates, 5 passes, iteration limit |

`local-cpu/` preserves three complete geometric USB inputs from the existing
block-post-refiner tests (5.905 / 7.043 / 3.643 ms native wall). These tiny
inputs establish correctness and overhead, not an expected GPU speedup.
Neither worker-time sum nor scorer-only speed is a full-call speedup.

## Score contract / GPU representation

| Term | CPU semantics | GPU plan/implementation |
|---|---|---|
| Geometry | Exact orientation template plus unrounded placement translation | Resident orientation templates; current poses and compact changes; GPU materialization |
| MST | Connected point 0, ordered from/to scan, 0.001 tie rule | Independent net/candidate lanes, same sequential decisions within each net |
| Length | Linear and quadratic terms, net/edge order | F64 length array and ordered accumulation |
| Crossings | Foreign nets, endpoint-distance exemption, strict oriented intersection | Parallel segment rows, bit masks; accumulate qualifying weights in original pair order |
| Pad hits | Layer and same-net exemptions, internal IC owner, one hit per physical pad per segment | Static physical-pad groups, transformed boxes/layers; count hits then add each weight in original segment order |
| Distance hints | Resolved targets only, linear then max/min quadratic terms | Static target bindings, GPU derived point/group geometry |
| Clearance | Source box, explicit target or all components, source component exemption | Expanded static target bindings; GPU gap and penalty |
| Fixed penalties | Same-side and preferred-layer depend on candidate poses | GPU layer comparisons; ordered penalty sum |
| Edge hints | Box edge distance in existing left/right/top/else-bottom order | GPU target box and resident board bounds |
| Paths | Ordered/deduplicated ports, zero edges, detour/backtrack/turns/facing, placement rounding | Static port order/bindings; GPU transformed points, normals and topology penalty |

CPU retains generation, validity checks, route comparison and final diagnostics.
Production must not call the CPU score for every GPU candidate; optional
`PCB_POST_PLACE_GPU_VERIFY=1` independently compares scores outside timing.
Unsafe numbers, unsupported layers, geometry ownership and capacity failures
must fall back for the entire native call. Small scalar initial/diagnostic
scores remain CPU. New kernels and scratch belong to post-place and use the
existing process-wide runtime/lease; they do not introduce another device.

## First implementation evidence and integration correction

`build-1/` passes the focused GPU tests and the complete ESPower score validation
(`ESPower-verify-1/`, 595 valid candidates). Its single-worker Telemetry full call
in `Telemetry-gpu-1/` completes all five passes in 13.455 s versus the saved
17.634 s CPU, with exact outputs and valid constraints. This is an intermediate
1.31x full-call result. GPU operation time is 2.522 s including initialization;
GPU scoring remains F64 and CPU route policy is unchanged.

The first four-worker comparison does not pass performance acceptance:
`Telemetry-cpu4/` takes 7.053 s, `Telemetry-gpu4-1/` takes 7.819 s. Both outputs
are exact. The GPU path originally waited for scores for the entire iteration
before starting CPU routes. A streaming producer/consumer variant was then
implemented so completed groups can reach CPU route workers immediately.
`Telemetry-gpu4-stream/` still took 8.580 s with only three route workers under
a four-CPU budget, despite an already initialized shared GPU. It is recorded as
a failed performance experiment, not an accepted speedup.

The next correction shares CPU execution permits between the GPU producer and
route workers. The producer holds one during CPU geometry/encoding/dispatch and
releases it during device readback; up to the original worker budget can then
execute routes. Each route group retains one worker and its baseline cache;
results reduce in original candidate order. Single-worker mode retains its
original serial route incumbent and does not borrow an extra CPU core.

`build-2/` adds independent GPU geometry and MST-edge validation. Exact geometry,
chosen MST endpoints and lengths are checked against existing CPU functions in
validation mode. The four focused tests pass, covering mixed terms/path scoring,
chunk sizes 1/17/128, workers 1/2/4, three failure points, disabled GPU and zero
timeout. Final evidence follows separately; intermediate build timings must not
be assigned to later addon hashes.

## Local native captures

`FPGA-local-cpu/` is the six-primitive FPGA capacitor group; the historical block
name is not the complete FPGA. `FPGA-full-local-cpu/` is the complete nineteen-
primitive FPGA from saved block `00091`: one CPU native refinement takes
69.717 ms and accepts one move. The input comes from the original saved
PortableScope `placement/input.json` and the saved native block solution;
no block or board search was rerun. These are local refinement checks, not
acceptance of the historical full PortableScope placement.
