# ESPower and PortableScope run review — 2026-10-01

Analysis only; production solver behavior was not changed. The requested Scope run is recorded as `PortableScope` and contains the reported locked-primitive exception.

## Evidence and measurement boundaries

- Revision: `1fe735fef65934b429db62d9d82831aed93e6aa1`.
- Release addon SHA-256: `2f5a1918176c33988f24797a8547c1a41644272afc90c8fb388e27427116c4ee` (saved runs and current artifact).
- ESPower: `debugging/pcb-layout/runs/ESPower/2026-10-01T20-18-47-561Z/`.
- PortableScope: `debugging/pcb-layout/runs/PortableScope/2026-10-01T20-22-51-860Z/`.
- Supplemental evidence: `debugging/scope-esp-review-2026-10-01/`: `locked-replay.json`, `clearance.json`, `esp-profile/result.json`, `esp.cpuprofile`, profile summary and scripts.
- Reused both saved complete-run attempts. Replayed only the failing one-component Scope call once (1.030 ms). Ran ESPower placement once with V8 CPU profiling because saved native captures left roughly 12 s unattributed. Did not repeat the full Scope run or run a CPU/GPU median suite.
- Hypotheses in one native batch overlap. Batch wall is counted once, not once per hypothesis. Detailed Rust spans are accumulated across workers and nested; they are attribution evidence, not additive wall-time stages.

| Latest saved run | Full harness wall | Result | Native block backends |
| --- | ---: | --- | --- |
| ESPower | 50.620 s | Exit 0, but `placementOk=false` | 38 CPU, 0 GPU |
| PortableScope | 727.516 s | Exception, no completed board placement | 100 CPU, 5 GPU |

The closed F32 roadmap is the current baseline. These findings do not reopen the removed physical-interval expansion or establish an F64/F32 regression comparison.

## PortableScope: false movement metadata on a locked component

The failure belongs to the first `display` block, capture `native/block/process-31908-thread-0/00001-display-bdde495f70/`. The native call took only 2.478 ms in the saved run.

Input and returned U6 placement are identical: x=5.716000000000001, y=0.1035, rotation=180, top layer. Nevertheless, all returned checkpoints and the final state report rotation=0, translationX=0, translationY=-0.0010000000474974513. The numeric frame is zero. A direct replay with the current addon reproduced the exception in the existing TypeScript apply function, with `samePose=true`.

Cause: `block_solver.rs::solution` (around line 915) derives transform metadata from the first source/output placement. It calls `geometry.rs::rotate_point` even for a zero rotation. That helper rounds the point to the placement grid. At this half-grid input:

```text
F32 input y                  0.10350000113248825
rotate_point(..., angle=0)   0.10400000214576721
unchanged output - rotated  -0.0005000010132789612
round_placement(difference) -0.0010000000474974513
```

`apply-board-solution.ts:25` correctly rejects a nonzero declared transform for a locked primitive. The message is misleading about actual motion: the solver did not move U6. The defect is inconsistent transform metadata. Locked candidate generation already keeps the original primitive, and move stages skip locked primitives.

Recommended fix: preserve the identity transform for an unchanged locked primitive, verify its actual placements, and retain the strict locked-transform guard. Test positive/negative half-grid coordinates, orientations and every checkpoint. Do not hide this by allowing a 0.001 mm movement tolerance or removing the guard.

### Why the exception appeared after twelve minutes

`tree-solver.ts:298` builds child promises with `.map(async ...)`. In this run the children use the synchronous `solveNode` branch inside `solveChildNodeAsync` (around line 370). A thrown exception becomes a rejected promise, but `.map` continues running subsequent children synchronously. `Promise.all` is reached only after that expensive work has finished.

Consequently, the first display failure does not stop the later DDR/FPGA work. All saved native calls are in the same process/thread capture, and there are no board-packer calls. Fix immediate propagation in the synchronous branch and bounded scheduling/cancellation for asynchronous jobs. This avoids wasting work on known failure; it is separate from accelerating a successful run.

## PortableScope performance

Captured block batches account for 711.220 s; 96 local refiner calls account for 2.809 s. Board packing and final board refinement were never reached.

| Block family | Batch wall, s | Explanation |
| --- | ---: | --- |
| DDR caps / termination / core | 474.571 | 43 components; both grouped and released hypotheses CPU |
| FPGA supplies / core | 55.281 | CPU grouped hypothesis determines completion despite GPU released hypothesis |
| Protected analog front end | 53.029 | Three block calls |
| Analog bipolar supply | 26.391 | CPU work |
| MCU supply / core | 20.323 | CPU work |
| AFE 3V3 / FDA | 15.616 | CPU work |
| ADC family | 14.162 | CPU work |

DDR grouped hypothesis: beam 125.705 s, singles 96.985 s, total 222.693 s. Released hypothesis: beam 225.164 s, singles 249.363 s, total 474.531 s. They share one batch, so do not sum their totals as elapsed time. DDR alone accounts for about 65% of the failed run.

FPGA demonstrates the coverage problem directly: grouped 5 primitives / 19 components takes 55.250 s on CPU; released 19 primitives / 19 components takes 21.086 s on GPU (873 batches, 5,121,535 candidates). These are different hypotheses, not a controlled speedup comparison. The overall batch still waits for the CPU hypothesis.

`block_solver/cubecl.rs::supported` limits the current evaluator to at most 20 components and 20 primitives, single-component primitives, supported geometry and terms. Composite primitives, path ports and several signal-corridor/relation cases reject GPU execution. DDR exceeds the component limit even when grouped. Grouped FPGA fails the composite-primitive restriction. Merely forcing the backend or raising a constant does not implement the missing representations and score terms.

Detailed accumulated CPU spans:

| Span | Accumulated time, s | Calls |
| --- | ---: | ---: |
| score_block_with_overlap_matrix | 685.288 | 15,789,900 |
| direct_pad_crossing_penalty | 479.418 | 14,230,755 |
| frontier_scarcity | 110.730 | 22,180 |
| block candidate generation | 74.658 | — |
| convex hull | 43.038 | — |
| long-net penalties | 35.789 | — |
| micro-route wrapper | 31.273 | — |

Pad-crossing time is nested inside scoring; these rows must not be added. The micro-route wrapper is not proof that this much time was spent in A*: block scoring here uses the geometric metric. GPU readback spans also include waiting for computation, not just transfer bandwidth.

## ESPower performance and invalid geometry

Saved phases: auto-place 47.550 s, footprint resolution 1.958 s, remaining pipeline phases below 0.2 s. Captured native block batch wall is 30.515 s; board call 4.779 s; all 19 refiner calls 0.579 s. Final refiner takes about 0.465 s. All these calls use CPU: block forms are unsupported, while board/refiner auto selection retains CPU for this workload/settings.

The MCU family accounts for 23.590 s of block batch wall across initial hypotheses and pairs. It has 15 components, but both representations retain composites and path ports. Other block families: charger 2.736 s, USB port 2.399 s, battery ADC 0.756 s, current-monitor family 0.704 s. Detailed block score spans accumulate 30.564 s, including 13.116 s in pad crossings; candidate generation accumulates 7.336 s.

The supplemental placement-only pass takes **44.658 s** and produces exactly the same component poses and the same invalid report. Captured block/board/refiner walls are 37.218 / 4.135 / 0.614 s, leaving 2.691 s outside them. V8 sampling attributes most time to the blocking native entry calls. Their JavaScript stack location does not mean the work runs in JavaScript. This does not retrospectively explain the saved run's entire unattributed interval, but gives no evidence for a dominant TypeScript arithmetic bottleneck. It is not a full DSL-run speedup measurement.

### Confirmed geometry and fallback defects

U1–X1 actual body/collision-box gap is **1.0964999198913574 mm**, versus required **1.125 mm** (deficit approximately **0.0285 mm**). The report rounds the displayed gap to 1.10 mm. The same violation exists from `01-v2-tree.json` through final output; post-place and board alignment did not introduce it.

The grouped MCU block's initial native solution (`00032-...-61e55d4eeb`) already has this gap but returns `hardCount=0`. Its input correctly has pair clearance 1.125 and conflict=1.

`block_solver.rs::rebuild_component_geometry` (around line 1551) rebuilds body boxes from width/height centered on the component placement. This discards an asymmetric footprint body's offset relative to that placement. X1's authored local body spans y=-1.817..1.7392. At the returned 180-degree pose, centering it incorrectly produces a U1–X1 gap of about 1.1354 mm instead of 1.0965 mm. This explains the false native hard rank; it is not an epsilon-scale F32 discrepancy. Preserve the transformed body offset when repairing this function.

The subsequent admission checks **do reject** candidates: saved MCU diagnostics say `Block checkpoint portfolio: 0 legal candidates, 0 retained`. However, `tree-solver.ts` explicitly returns `groupedFallback ?? fallback` when the admitted pool is empty. That invalid grouped layout proceeds into board packing. Therefore the problem is not simply that every validator missed the overlap: native geometry is wrong, and the empty-portfolio fallback continues with a known-invalid candidate. Any repair must preserve a clear failure/repair status rather than treating that fallback as accepted geometry.

The final report also contains a critical-pair hint U1.1–L1.1: distance 5.623 mm, expected at most 4 mm. Do not confuse that hint with the U1–X1 physical-clearance failure. Exit code 0 is not proof of valid placement. The preceding ESPower run at 20:11 also had `placementOk=false`; no regression origin is established here.

## Recommended implementation order

1. Correct locked identity metadata and early failure propagation; reproduce with the tiny display input rather than rerunning the whole Scope board.
2. Correct asymmetric body-box transformation and make the empty legal-portfolio outcome explicit. Verify both native hard rank and the final independent geometry report on saved MCU checkpoints before a full board run.
3. Extend GPU block representation/evaluation to the real critical workloads: DDR's 43 components and grouped FPGA/MCU composites, followed by required path-port/signal-corridor terms. Preserve full beam, singles and applicable pairs. Evaluate both hypotheses and their shared batch wall.
4. Optimize pad-crossing evaluation: reuse resident fixed geometry and existing incremental caches, batch changed-segment/obstacle work, and avoid rebuilding invariant net data per candidate. First inspect existing cache hit/miss behavior; do not create another competing scorer.
5. Optimize/cache frontier-scarcity and candidate construction, then examine hull and long-net work if still significant. Measure the complete affected block cycle, not kernel time alone.
6. After a valid Scope run reaches board packing, profile its remaining stages. Telemetry's earlier routing bottleneck must not be assumed to dominate these different boards. Router GPU work remains a separate opportunity, not the first explanation for these runs.

Thirty-second end-to-end execution is an objective, not established feasibility. Even eliminating the entire DDR batch leaves approximately 253 s in this failed Scope attempt, and the still-unreached board stage adds further work. Broad coverage and reduced repeated scoring are needed; a faster float format or one accelerated hypothesis alone cannot deliver that target.

## Follow-up — 2026-10-02

At the user's request, TypeScript now accepts locked-primitive translation metadata residuals up to F32(0.01) mm on each axis and returns the original primitive, preserving authored coordinates exactly. Rotation, non-finite translations and larger residuals still fail. This supersedes the strict-zero recommendation above for the immediate Scope mitigation; the native metadata computation is unchanged.

Verification: `npm run typecheck`; the focused `native transforms preserve` test in `pcb-anchored-spacing.test.ts` (signed threshold/residuals, rejected larger/non-finite shifts and rotation); applying the saved display solution through the updated function succeeds and returns the identical original U6 primitive. `git diff --check` passes. No new native or full-board run was needed.

Git history establishes that asymmetric body-box recentering **predates F32 migration**. `git blame` attributes the width/height and `target +/- size/2` lines to the earliest available commit, `0079c7d` (2026-09-12, standalone backend extraction). The exact same recentering is present in the last F64 revision `51274d5`. The F32 commit `1fe735f` changes the adjacent rotation-difference helper but does not introduce recentering. The invalid grouped fallback is also older: it appears in `f212e52` (2026-09-27). History here cannot establish when the defect was first authored before extraction, or whether F32's changed candidate selection first exposed this particular output.

## Body-offset correction — 2026-10-02

Implemented at the user's request: `rebuild_component_geometry` now rotates the original body bounds around the source component placement and translates them to the target placement using the existing native box-transform helpers, retaining the authored offset. It no longer synthesizes a centered rectangle from width/height. GPU moving-body translation uses placement-grid rounding to match that shared native transform policy. The contract and F32 precision remain unchanged. The empty-portfolio fallback and delayed error propagation described above are separate, still-unmodified issues.

Two Rust regressions cover asymmetric bodies under translation and all four quarter-turn orientations, and the X1 body at the saved offending U1–X1 poses. The latter now detects approximately 0.0285 mm insufficient clearance rather than incorrectly accepting the pair.

Release addon SHA-256: `aa2204f763790eb2745955fa40dda3ef5be42eb70a0fdf61b393acf8547da634`, 20,342,272 bytes (19.40 MiB). Build succeeded with existing compiler warnings.

One complete replay of the exact grouped MCU input `00032-...-61e55d4eeb` ran initial beam/singles and its deferred pair continuation. Initial wall: 6.063 s; pairs: 5.939 s. These are diagnostic replay timings, not a full-board performance result. Checking returned placements against original TypeScript footprint geometry gives:

| Output | U1–X1 gap, mm | Required, mm | Component-pair clearance violations |
| --- | ---: | ---: | ---: |
| Previous initial output | 1.096500 | 1.125 | At least U1–X1 |
| Corrected initial output | 1.125500 | 1.125 | 0 |
| Corrected pair output | 1.203300 | 1.125 | 0 |

Both corrected native outputs have hardCount=0. This checks component clearances, not every electrical hint or complete ESPower acceptance. Evidence and replay helper: `debugging/scope-esp-review-2026-10-01/body-fix/` and `verify-body-fix.mjs`.

A small three-component asymmetric-body fixture completed beam/singles/pairs on CPU and actual CubeCL GPU, with exactly identical final solutions (score 390.40234375, hardCount=0). Independent body-clearance checks pass. GPU used 24 batches / 2,829 candidates without fallback. This is a correctness smoke check, not a speed measurement.

### GPU diagnostic limitation discovered during validation

Strict `PCB_BLOCK_GPU_VERIFY=1` did **not** pass for every intermediate candidate. The initial synthetic input, derived from a historical USB fixture with off-grid primitive bboxes, exposed a compact-materialization bbox difference of 0.001 mm and a score difference 461.70273 vs 461.7691 during singles. The failure dump is retained as `body-fix/gpu-failure.json`, and the input as `gpu-off-grid-problem.json`. A historical v8 F32 addon also fails strict verification on this input, but at a different hard-count check; that does not establish the precise origin of the current discrepancy.

For an isolated body-offset check, primitive bboxes were put on the placement grid while keeping body offsets fractional. Strict verification then passed beam/singles but stopped in pairs at score 859.48145 vs 859.48395 (difference about 0.0025). The final normal-mode CPU/GPU runs on that revised fixture match exactly and pass geometry checks. Strict candidate-score/compact-geometry parity remains open; no diagnostic tolerance was widened and no claim of complete per-candidate parity is made. Small retries were prompted by these failures; the heavy MCU replay ran only once, and no full-board run was repeated.

Checks: final-source `cargo test` (81 passed, 4 ignored), release addon build, TypeScript typecheck, focused locked-pose and pair-continuation tests (2 passed), block-search-stage tests (3 passed), saved MCU full-cycle replay, small CPU/GPU full-cycle comparison and `git diff --check`. Strict GPU diagnostics are the explicit exception above.
