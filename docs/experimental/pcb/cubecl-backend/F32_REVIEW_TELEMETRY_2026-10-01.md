# F32 review and complete Telemetry GPU run — 2026-10-01

**Historical review:** subsequent user-directed cleanup removed the attempted
physical-interval geometry implementation and withdrew that added scope. See
`F32_MIGRATION_RESULTS.md` for the current changes and checks. The measurement
below belongs only to its recorded artifact.

The migration is implemented in substantial part but is **not ready for unconditional
acceptance**. This review changes documentation only. It does not fix solver code,
alter inputs, reduce search stages, or declare the migration roadmap complete.

## Current complete run

One complete Telemetry pass, current rebuilt release addon, **197.8401432 s**
(3 min 17.84 s). Native solve cache disabled; forced CubeCL for board, block and
refiner where supported. Full pipeline includes block portfolios, ordinary and
aligned board searches, local refiners and final refiner. Final refiner completes
all five passes without timeout. `placementOk=true`.

Artifact: `debugging/f32-migration-2026-10-01/review/Telemetry-full-gpu/`.
Source/addon manifest: `review/manifest.json`. Addon SHA-256
`03549f62b910462d2b587003393f364d952af1949cea8dbbdcff5f2762af363e`,
20,335,104 bytes (20.335 decimal MB, approximately 19.393 MiB).
Sources in the manifest were checked unchanged after the run.

```powershell
$env:PCB_BOARD_PACKER_NATIVE_PATH=(Resolve-Path 'debugging/f32-migration-2026-10-01/review/pcb-f32.node').Path
$env:PCB_POST_PLACE_BACKEND='cubecl'
$env:PCB_BLOCK_SOLVER_PROFILE='1'
node --import tsx scripts/experiment-board-gpu.mjs capture fixture=Telemetry backend=cubecl block=cubecl workers=4 detail=1 sourceReport=debugging/f32-migration-2026-10-01/review/manifest.json out=debugging/f32-migration-2026-10-01/review/Telemetry-full-gpu
```

The output directory is immutable; do not rerun into it. The harness sets one
layout process, no subtree processes and one final-refiner thread. The explicit
four-thread budget also controls native **block hypothesis batches**, through
`block-solver-engine.ts`; it is not exclusive to board packing.

| Full-cycle part | Current wall time |
|---|---:|
| Native block batches, 90 individual calls | 47.853 s |
| Ordinary board solve | 55.892 s |
| Aligned board solve | 64.745 s |
| All 56 native local/final refiner calls | 13.131 s |
| Remaining pipeline time, approximately | 16.15 s |
| Complete measured pipeline | **197.840 s** |

The two board capture wrappers total 120.703 s. Block timing divides each
recorded batch wall by its member count before summing, avoiding duplicate batch
accounting. The final refiner alone takes 11.641 s. Rendering/writing final
artifacts occurs after the harness's pipeline timer; build time is excluded.
Detailed profiling is enabled, so this is a diagnostic single-pass wall measurement.

## Why the previous run took 510 seconds

The saved v7 run really took 510.271 s. Its report incorrectly stated four board
threads. Both `results.json.settings` and the native log show **one**. The command
omitted `workers=4`; the harness overwrites `PCB_BOARD_PACKER_THREADS` with its
default `1`. This also serialized block hypotheses. The report has been corrected.

Its board solves took 184.883 + 192.138 = 377.021 s, about 74% of total wall.
Block capture batches took 99.243 s. In its 47 unsupported-block fallback calls,
98.462 s of CPU work was recorded. Final refiner took 13.154 s.
Board GPU operations totalled approximately 69.2 s within the board solves;
the rest was principally host-side work, not evidence of a GPU outage.

Current/previous observed wall ratio is 2.579x. **This is not an isolated F32
speedup**: both runs use F32, concurrency changed and v8 removed the redundant
hard-count score offset. Board candidate trajectories and 58 final poses changed.
The original input hash matches. No compatible full CPU reference for the claimed
approximately 1,200 seconds was established; that historical number must not be
used to publish a measured CPU/GPU multiplier. No additional full CPU pass was run.

## Measured bottlenecks and next optimizations

1. **CPU micro-routing inside board search is the largest measured cost.**
   `solver.rs::rerank_candidates` calls `board_micro_route_penalty` after GPU
   shortlist selection. The ordinary/aligned profiles show 23,219 / 26,551 route
   calls and 140.042 / 162.480 seconds of accumulated worker time. Those worker
   totals overlap across four threads and are not wall times. Generation totals
   only 4.095 seconds of worker time in the ordinary solve. Focus next on reuse of
   route grids/obstacle queries and cached evaluations, route-call reduction with
   proven bounds, then batching suitable route work. Preserve the current search
   and routing budgets; do not obtain a speedup by skipping required checks.
2. **Telemetry block assembly does not use GPU in this run.** All 90 logged
   calls report CPU; 47 record the unsupported-feature rejection (other calls
   are trivial/early paths). The unsupported calls total 93.040 worker seconds;
   parallel batching reduces block wall to 47.853 s. Existing eligibility rejects
   `pathPorts`, composite primitives, through-hole geometry and several relation
   forms. It also rejects non-power primitives with at most 240 endpoints because
   their signal-corridor terms are not implemented in the complete GPU scorer.
   Heavy ADC/supply captures contain excluded features. Supporting these terms
   and forms is a stronger target than optimizing already-fast supported FPGA
   examples again. These exclusions predate this review and are not GPU failures.
3. **Board GPU dispatch is synchronous and serialized across host workers.**
   GPU operation wall totals are 19.624 / 20.119 s. Accumulated host waiting for
   the shared board-engine mutex is 26.660 / 32.044 worker seconds. Consider
   combining independent parent batches and scheduling work to overlap routing
   with GPU execution. Do not simply remove the lock: current mutable engine
   and scratch ownership rely on it. `read_ms` includes waiting for kernels, so
   it must not be described as pure PCIe transfer time.
4. **Final refiner also spends most time on CPU routes.** Its five passes total
   10.627 s of route work versus 0.823 s of global scoring. Reusing unaffected
   baseline route evaluations is worth investigating; GPU score arithmetic alone
   cannot substantially reduce this stage further.

Explicit shader FTZ canonicalization adds integer bit operations around float
results/loads. Its isolated cost has not been measured; it is a possible secondary
optimization, not a demonstrated explanation for the 510-second run. Arithmetic
policy must not be silently relaxed to improve a benchmark.

## F32 review findings

### High: localization happens too late in some TypeScript paths

`pcb-auto-place/geometry.ts::getLocalPointWorld` and box helpers use `fp.add/sub`
on absolute placement coordinates. `encode-board-problem.ts` constructs component
boxes using those helpers before Rust `Frame::localize` receives the DTO.
Thus correct native subtraction-before-narrowing does not protect the whole path.

Reproduction: pose `(1_000_000, 1_000_000)`, local points `(0,0)` and `(0.001,0)`.
Both returned world points are `(1_000_000,1_000_000)`. The pad separation is lost.
The script and result are `review/reproduce-early-narrowing.mjs` and
`review/early-narrowing.json`. The existing large-offset test bypasses this TS
encoding by translating an already-built native DTO, so it misses the defect.
Move localization ahead of the first TS geometry operation and keep one frame
through encoding and validation; add an end-to-end encoder test.
Ordinary centered Telemetry does not exercise this failure.

### High: physical hard checks do not implement the planned error chain

Native `geometry.rs` still uses one fixed `1e-6` for determinant, squared-length,
coordinate and segment-parameter comparisons, despite their different units.
`solver.rs::primitive_hard_overlap` uses `1e-9`; refiner legality uses its existing
domain `0.001`. None of these tracks narrowing/rotation/transform/output errors.
`Interval` is used for score diagnostics and route pruning, not for this physical
predicate chain. Restoring original locked poses at the boundary does not ensure
that the geometry accepted inside the solver encloses those restored poses.

This is an unimplemented acceptance requirement, not proof that the tested
Telemetry placement is invalid. Preserve physical tolerances while implementing
the specified uncertainty treatment. Do not solve it by globally enlarging epsilon.

### Medium: compatibility and quality acceptance remain open

The adapter rejects any localized absolute coordinate beyond 1024 mm. It checks
source placements as well as board extent, so even small objects initially far
outside a board can be rejected. This is documented but is a new supported-domain
restriction, not evidence that all previously valid input geometries migrated.
Finish the schema/entrypoint and supported-frame acceptance tests.

Saved ESPower v8 and v6 full-GPU poses are identical. Their historical isolated
route comparison still has unresolved/budget-exhausted obligations 3 -> 4, with
USB D+/D- changing from found to budget_exhausted. Exhaustion is not proof of an
impossible route, but it remains a quality review item. A close score or a passing
collision report does not close route-quality acceptance.

Some TS integer counters/pin numbering were also mechanically wrapped in F32
(e.g. `placement-input.ts` pin-number sum and `placement-graph.ts::countTreeNodes`).
Ordinary boards are below the exact-integer limit, so this is not an observed
Telemetry failure; these operations should retain integer semantics as required.

The implementation's strengths are real: native models/buffers are F32, quarter
turns and common coefficients are explicit, CPU environments are scoped, emitted
GPU arithmetic is controlled, rank sorting uses a total order, and whole-call CPU
recovery remains F32. These do not eliminate the findings above.

## Validation and limitations

- Current release build passed; 79 Rust tests passed, 4 hardware/diagnostic tests
  intentionally ignored. Typecheck passed. Three focused F32 numeric tests passed.
- One complete GPU-enabled Telemetry pipeline passed. Actual board and refiner
  logs show CubeCL and one shared runtime. Block exclusions remain CPU as above.
- The unchanged external F64 validator accepted raw final poses: zero reported
  overlaps, outline/hole/region/layer violations or missing components.
- External HPWL: 2860.231464 mm v7 -> 2869.211364 mm current (+0.314%). Minimum
  raw clearance margin is -0.000600587 mm, accepted by the unchanged existing
  domain tolerance; do not describe it as mathematically zero penetration.
- Native board hardCount remains 3. Those native constraints have different
  coverage from the physical report; identity classification remains open.
- Full route completion and all-F32 migration acceptance are not proved by this
  check. No additional heavy CPU reference, repeated median run or shortened
  search was used. GPU-only suites from v8 remain saved evidence, not claimed as
  newly rerun on this review artifact.

Evidence: `review/Telemetry-full-gpu/results.json`, `stderr.log`, `review-summary.json`,
`external-f64/measurement.json`, `review/rust-tests.log`, `typecheck.log`,
`numerics-tests.log`, and `review-native-build.log` in the parent experiment folder.
