# Board Packager GPU: implementation evidence

2026-10-01. **Board Packager implementation and optimization are complete for the measured scope; work proceeds to Post-place / Refiner.** CPU F64 formulas and search policy remain the reference.

Current release evidence is in [Final optimization build and one-pass performance](#final-optimization-build-and-one-pass-performance) and [Full-board integration after optimization](#full-board-integration-after-optimization):

- Complete Telemetry ordinary + aligned native calls: 341.976 s saved CPU, 330.183 s previous GPU, **162.887 s optimized GPU (2.099x CPU)**; exact outputs, no fallback.
- Full ESPower pipeline: 45.408 s, all 53 components, `placementOk=true`, exact CPU placements and byte-identical SVG.
- Final addon: 19,256,320 bytes, SHA-256 `2dbcddb15a43b5f19db26cb0a2c0bd9a712c063406e300d3a6d8b203383909d7`; default `auto`, GPU host budget at most four, batch ceiling 4096.
- Final checks: 20 hardware GPU tests, 24 native/assembly integration tests, 64 GPU / 60 CPU-only Rust tests, typecheck; same Windows system DLL imports.
- Testing policy: one pass per test/version, reuse compatible CPU references; no new median series. The 2.099x figure covers the native packer, not the entire PCB pipeline. Full Telemetry pipeline timing, PortableScope completion and the pre-existing RF fixture failure remain limitations, not claimed successes.

The sections before **Optimization follow-up** preserve chronological development evidence. Their references to defaults, final builds and stopped work describe those earlier snapshots and do not supersede the current summary. See [the roadmap](BOARD_PACKAGER_GPU_ROADMAP.md) for retained evidence limits.

## Source baseline and reproducibility

The starting tree is `feat/block-solver-cubecl`, HEAD `6d1f9c8`, with the
uncommitted shared `compute` extraction. The original addon, source copies,
source/addon SHA-256 hashes, binary Git diff and initial status are saved in
`debugging/board-gpu-2026-10-01/baseline/`. Existing changes were retained.

`scripts/experiment-board-gpu.mjs` has four modes: `snapshot`, `build`, `capture`, and
`replay`. Managed `build` saves the source tree before compilation and verifies
that those sources did not change before associating the addon hash. It rejects existing output directories, disables native solve cache,
preserves captured special numbers, verifies capture checksums and compares
complete native outputs outside the timed interval. `sourceReport` associates
a saved addon with its build's source manifest instead of claiming that later
working-tree edits are compiled into it.

Example commands from the backend root:

```powershell
node scripts/experiment-board-gpu.mjs snapshot out=debugging/board-gpu-2026-10-01/baseline
node --import tsx scripts/experiment-board-gpu.mjs capture fixture=esp32c3 backend=cpu
node scripts/experiment-board-gpu.mjs replay inputs=<native-board-capture> backend=cpu runs=1 detail=1
node scripts/experiment-board-gpu.mjs replay inputs=<same-capture> backend=cubecl runs=1 reference=<cpu-results.json>
```

Capture fixes block backend to `auto`, board workers to 1, layout workers to 1,
subtree workers to 0 and refiner workers to 1. Those parameters must be retained
for full-board comparisons. Exact native replay excludes block/refiner and TS
portfolio work; full capture includes it. Capture and replay are separate
evidence. Historical invalid PortableScope captures are profiling/stress inputs,
not valid-board acceptance references.

## Term and support matrix before kernels

The GPU column describes the implemented representation. Individual terms and
geometry are verified by opt-in diagnostics; real-board coverage is recorded
below. A whole-input guard rejects unknown component layers, unsafe numbers,
invalid orientations, duplicate primitive IDs and capacities before dispatch;
an active term cannot be ignored. Geometry formulas are in `solver.rs`, except
the named shared modules.

The final guard also checks box ordering, dimensions, placement scores, edge
offsets, compiled endpoints and all weights before search. Numeric geometry is
limited to finite values with magnitude at most 1e8. Component conflict and
clearance matrices must be symmetric: the existing CPU hard cache uses an
unordered pair key, so directional matrices retain the original CPU path.
Unknown component layers, empty edge alternatives and excessive input sizes
likewise use CPU without initializing GPU. This is a domain guard, not a change
to CPU validation or scoring.

| Input / term | CPU reference | Required GPU representation / checks |
|---|---|---|
| version, grid, searchWidth, compactness | model validation; generation; board_score | Version/size guard; unchanged CPU search/generator; exact normal/high coefficients |
| bounds, fullBoardBounds | fit_to_bounds; edge_bias_penalty; component_board_bounds | Separate search and physical board rectangles; do not conflate |
| boardOutline, edgeClearance | primitive_outside; polygon_board_outside_severity; geometry::box_inside_polygon_board | Resident polygon; corner inclusion, segment intersections, edge distances and original epsilons |
| primitive bbox and multiple collisionBoxes | packing_boxes; packing_box; hard_count; board_score | Orientation templates; exact union and ordered box loops, including empty-box behavior |
| compound components | primitive_hard_overlap | Ordered component indices, body boxes and opposite-side/through-hole boxes |
| componentConflict, componentPairClearance, clearance | primitive_clearance; primitive_hard_overlap; primitive_can_conflict | Resident integer conflict matrix and F64 clearances; broad-phase maximum then exact component pairs |
| component layer, throughHoleBoxes | primitive_hard_overlap; layer_envelope | Preserve same-layer body/body and hole/hole versus opposite-layer body/hole rules; hole boxes also contain opposite-side silk/SMD geometry |
| component edgeClearance, boardOverflow | component_outside; component_outside_severity | Four independent overflow sides; overflow switches to bounds-only test; no blanket polygon bypass |
| locked, allowedOrientations, canRotate | initialization; orientation_variants; local_improve | CPU proposal policy and fixed poses retained; soft spacing skips two locked primitives |
| obstacles / holes | hard_count; hard_severity; candidate_hard_count | Resident boxes; one count per overlapping obstacle and max box depth per obstacle |
| constraintRegions: layers, allowBlocks | component_region_overlap; constraint_violation_count/severity | One count per violated region, severity sums every disallowed component; preserve layer/allow rules |
| edgePlace | edge_place_penalty; edge_place_violation | Edge alternatives in original order; fullBoardBounds, inset, optional exact x/y; hard tolerance max(0.05, grid*0.51) |
| relations: endpoints, effect, weights, distance limits, satellite/side | compile_relations; endpoint_point; relation_penalty | Resident compiled endpoints and rules; component pad averages/rounding, absent endpoints, same-owner skip, offset rounding and capped limit penalty |
| signal-path ports and metadata | signal_path::topology_penalty | Ordered path groups, duplicate-order selection by reference, zero-length edges, turns/backtrack/facing and final placement rounding |
| ordinary-net marker relations | ordinary_net::penalty, called by signal_path::topology_penalty | Active-frame fanout 2..8, sorted net iteration, shortest pad distance, pair affinity cap and CPU pair accumulation order |
| envelope overlap | envelope_overlap_penalty; layer_envelope | Top/bottom envelopes; conflict exclusion; max depth, depth²*700 + depth*140 |
| compactness area/perimeter | board_score | Ordered collision-box union; normal/high weights; softSpacing.compactnessScale |
| softSpacing: gap, exemptPairs | soft_spacing_penalty | Pair exemptions, locked/conflict exclusions; same-layer minimum hypot distance and deficit² |
| softAlignment: pairs, anchors, tolerance, weights, orientation | soft_alignment_score | Pair and placement IDs; bbox fallback versus component origins; piecewise error and cosine orientation term |
| IDs, labels, kind, ownership/source metadata, placements and connection-point references | lexical_ids; primitive_key; seed_rank; endpoint compilation; output application | IDs stay integer, metadata remains CPU; kind affects edge bias; placement geometry contributes to endpoints/orientation/output |

## Order, rounding and CPU work retained

- `position_candidates` deduplicates centers before move/clamp. Its legal filter
  is per orientation and per ordinary/alignment call: if any candidate has zero
  incremental hard violations, only legal candidates survive in that group.
- `finish_cheap_candidates` assigns ordinals before deduplication, then sorts
  `(hard_count, hard_severity, score, ordinal)` and applies separate 32 ordinary
  / 16 alignment budgets. `rerank_candidates` applies the baseline-hard filter
  before existing `lazy_rank` and route corrections.
- `rank_improves` accepts severity or score improvement using its existing
  `0.001` rules. Sorting uses exact `partial_cmp`; it is not the same comparator.
- Joint pairs check the first primitive against the parent, then the second
  against parent plus first. Pair shifts are atomic; equal-depth Beam handling,
  dedupe and 16 geometric / 4 route-corrected budgets remain unchanged.
- Translation rounds each box coordinate, placement, connection point and path
  port independently. Repeated repair translations must reproduce each step;
  adding translation deltas and rounding once is not equivalent. Rotation stays
  CPU and uses the current pivot and normalization rules.
- CPU retains generation, fit-to-bounds, seed/next selection, control flow,
  dedupe, existing lazy route policy, Micro-A*, final serialization and TS
  ordinary/aligned portfolio. Small isolated control ranks may remain CPU;
  mass legality/full ranks, including repair, are required GPU work.

`PCB_BOARD_PACKER_DETAIL=1` emits `[board-detail]` CPU term call counts and
accumulated worker milliseconds. Timings are nested and must not be summed or
treated as wall time. Detail profiling is opt-in; ordinary performance runs
leave it and candidate validation off.


## Development evidence (not performance acceptance)

CPU full captures produced `placementOk=true` for esp32c3, ESPower and Telemetry.
Exact native calls, inputs and outputs are preserved beneath their respective
`capture-*-cpu/native/board/` directories. Telemetry includes both ordinary and
aligned calls; their portfolio must be measured together.

| Input | CPU native observation | Development GPU validation |
|---|---|---|
| esp32c3 | ~55–67 ms; 5655 legality proposals / 449 full CPU ranks | Every proposal rank agrees; final native output exactly matches capture |
| ESPower | ~11.5–14.6 s; ~349k legality proposals / 13597 CPU ranks; route profiling ~8.6 s | `ESPower-gpu-verify/results.json`: 349479 candidates, 747 batches, exact final output, one runtime init, no fallback |
| Telemetry | Ordinary ~490 s, aligned of comparable scale; full capture ~1126 s | Both ordinary/aligned outputs match exactly; final repeated acceptance is recorded below |

These numbers describe exploratory runs. GPU validation adds CPU work and extra
readbacks; some early observations overlapped compilation or another capture.
They must not be used as accepted speedups. The ESPower validation uses the
frozen `build-profile-recovery/solver.node` and its source manifest. The original
Telemetry/esp32 capture metadata predates managed source manifests; their addon
hash identifies the preserved baseline binary, while recorded working-tree
hashes do not certify build correspondence.

`PCB_BOARD_GPU_VERIFY=1` compares each full candidate rank against CPU outside
the protected GPU session. The newer pipeline also reads materialized pose
geometry and compares every coordinate bit, including repeated rounding.
`PCB_BOARD_GPU_VERIFY_SHORTLIST=1` compares shortlisted IDs and their rank bits
against the full unpruned GPU batch. Both flags are disabled in performance runs.
`PCB_BOARD_GPU_CHUNK_SIZE` restricts a diagnostic batch to 1..512 candidates;
smaller chunks must preserve the final result.

The current production pipeline first evaluates legality for every proposal,
preserves the original per-generator legal filter, deduplicates on CPU and
scores only survivors. Single-move GPU selection preserves separate 32/16
ordinary/alignment budgets; pair selection preserves 16; repair selection
preserves 1. Geometry templates are resident, while ordered translations and
frame indices form compact batches. Final serialization uses one isolated CPU
rank, and Micro-A* retains its original shortlist policy.

Legality computes fixed-parent contributions once per chunk and adds only the
moving unary and cross-pair counts. Full rank still reduces contributions in
the original order. Primitive-pair maximum clearance and conflict eligibility
are immutable across poses and are computed once from ordered component rules.
The diagnostic path independently checks optimized legality against full hard
counts; this optimization cannot prune a soft score on an assumed sign.

The initial implementation used chunks of at most 512 candidates (4096 after
the optimization follow-up below). Scratch and resident data each have a
64 MiB budget, with allocation/index/dispatch guards. Orientation interning
retains its source Arcs and resets between chunks when the template/pointer or
resident budget is reached. Scratch layout `board-ranks-f64` is isolated from
block layouts, and handles stay inside the shared session mutex.

The initial default was CPU (changed to `auto` in the optimization follow-up).
`auto` requires at least 34 primitives, 154 components,
24 movable primitives, search width 32 and 535 relations: the conservative floor
of the measured Telemetry workload. Smaller and route-dominated boards stay CPU
even with a ready runtime. Cold and ready runtimes use the same floor. The
initial effective native worker budget had to be at most four: greater budgets
remained CPU. The follow-up instead caps auto's GPU host workers at four.
Explicit `cubecl` retains the
existing native thread cap. This policy is empirical for the recorded machine,
not a universal crossover guarantee.


## Correctness and recovery after GPU selection

`build-validated/manifest.json` binds the frozen Rust sources to its release
addon. `ESPower-gpu-terms-shortlist-idle/results.json` validates materialized
geometry bit for bit, every term, integer hard counts, aggregate ranks, optimized
shortlist IDs/rank bits and the complete native output. It actually uses CubeCL,
with no fallback. Its 46.36 s duration includes verification and is not a speed
measurement. `ESPower-gpu-terms-shortlist/` is excluded: it encountered a busy
lease while tests were running and replayed CPU. The replay tool now rejects
such runs when GPU execution is required (`requireGpu=0` permits intentional
fallback checks).

`Telemetry-gpu-shortlist-first/results.json` preserves both ordinary and aligned
CPU captures exactly. The exploratory pair took 577.23 s (282.35 / 294.88 s);
10.51 million legality/rank evaluations were dispatched across the two calls.
Aligned Beam produced 388 joint candidates. Both calls use the same initialized
runtime; resident data is below 0.9 MB and scratch peaks at 5.32 MB. This is one
run, not acceptance of stable performance or the later optimized legality code.

The first full focused test run exposed an existing board-assembly assertion
comparing 6.95 to 6.949999999999999. The original frozen addon reproduces it
(`npm-test-board-assemble-baseline.log`); the adapter already rounds messages to
three decimals. That origin-conversion test now assigns an exact binary fixture
y-coordinate, retaining its strict assertion and leaving production behavior
and CPU references unchanged. After correcting this setup and the runtime test
inputs, `npm-test-pcb-board-fixed.log` passes 56 tests, including 13 GPU tests.
`npm-test-pcb-block.log` passes 28. Rust GPU tests pass 64 (2 ignored), CPU-only
passes 60 (1 ignored). A CPU-only release addon imports and returns the exact
esp32c3 CPU capture when CubeCL is requested; it reports the CPU-only build.

GPU tests cover non-rectangular outlines, edge alternatives, halfway coordinates,
compound/opposite-layer/through-hole geometry, overflow, obstacles, regions,
relations and topology, ordinary nets and alignment in normal/high modes,
source-group legality and stable shortlist ties, batch sizes 1/17/512,
snapshots after all three stages, disabled/unsafe/no-device recovery, runtime
failures within dispatch and after every stage, 1/2/4 workers, block → board →
block layout isolation, 2/4 competing processes and lease release after failure
while the failed process remains alive. Hardware absence is simulated through
an invalid Vulkan driver manifest, not tested on a physically different card.

## Historical release checks

`build-release/manifest.json` freezes source hashes and addon SHA-256
`2ed40cfb44497c66bc6c3b75bcb4294b71299e2509df4969a084b03450ac6a0e`.
The Windows x64 GPU addon is 19,202,560 bytes; PE imports contain only Windows
system DLLs, with no helper/compiler DLL. The current CPU-only build is
3,474,432 bytes, SHA-256
`e26fafdc027d40276c333242a1587a54a3cfa447b72e7af7c25a38ac8ae344ee`.
Its isolated CubeCL-requested replay reports CPU-only and exactly matches the
esp32c3 capture (`final-cpu-only-smoke/`). See `imports-release.json` and the
two build manifests for source and import evidence.

Final checks: `cargo test` passes 64 (2 ignored); `cargo test
--no-default-features` passes 60 (1 ignored); opt-in hardware `npm test --
pcb-board` passes 61; `npm test -- pcb-block` passes 28; `npm test --
pcb-route-cost-comparison` passes 9; `npm run typecheck` passes. Logs end in
`-release.log` under the evidence directory.

Additional `npm test -- pcb-signal-path` passes 7/8. The cross-block RF-style
fixture fails `report.ok === true` at line 118 with both the final release and
the preserved original addon (`npm-test-path-route-release.log` and
`npm-test-path-route-baseline.log`). This existing quality defect is not fixed
by this GPU change, and its fixture/reference is unchanged. Native path,
polygon, route and grid tests pass; GPU path terms also have independent
candidate and complete-result coverage.

## Repeated native performance

Machine: Windows 10.0.26200, AMD Ryzen 5 5600H (12 logical processors), RTX 3060
Laptop GPU with 6 GiB, driver 610.88, Node 26.5.0 (`hardware-release.json`).
`final-Telemetry-cpu4/` and `final-Telemetry-auto4/` each run the exact ordinary
and aligned captures together, first plus three warm repeats, with four native
workers and cache disabled. No builds, tests or other GPU solver jobs ran during
these series. Candidate, shortlist and stage validation and detailed profiling
were off. All 16 native outputs exactly match the original captures; the GPU
series additionally matches the new CPU reference. All eight GPU calls report
CubeCL, no fallback and one shared runtime initialization.

| Telemetry, complete native pair | CPU | Auto / CubeCL |
|---|---:|---:|
| First run | 346.067 s | 314.966 s |
| Warm median | 339.925 s | 310.752 s |
| Warm range | 333.838–356.068 s | 306.443–318.709 s |

Warm full-pair speedup is **1.094x**; first-run speedup is 1.099x. This is a
modest full-call improvement at four workers, not a kernel-only multiplier.
CPU generation, routing and the serialized GPU operations still limit scaling.

| Warm stage median | CPU | CubeCL |
|---|---:|---:|
| Ordinary Beam | 138.185 s | 124.839 s |
| Ordinary local improve | 24.588 s | 17.147 s |
| Ordinary repair | 0.109 s | 0.100 s |
| Aligned Beam (includes pairs) | 150.074 s | 146.804 s |
| Aligned local improve | 25.980 s | 17.952 s |
| Aligned repair | 0.054 s | 0.062 s |

Each ordinary/aligned pair dispatches 10,510,562 legality/rank evaluations in
22,506 batches. Aligned pair work has 49,087 raw GPU evaluations in 194 batches;
the controller's preserved count is 388 joint candidates. Resident data peaks
below 0.90 MB and shared scratch capacities at 5.325 MB. GPU operation/read
timings include synchronization; submit is host time. Engine mutex wait is an
accumulated worker total, not additional wall time.

These series use measured addon `2ed40cfb…6a0e`. The final accepted build is
`8557ab42a3d5481e1ac7468fd5235ff01df49b0f2113e048b58325a32dd22220`,
also 19,202,560 bytes. The only native source changes after measurement cap
`auto` at four workers and correct CPU-only backend reasons. Evaluator,
kernels, generation, ranks and search are unchanged; the measured four-worker
branch remains admitted. `measurement-to-accepted.json` and `.patch` record
the exact two-file delta, and `accepted-Telemetry-*-check/` repeats both exact
calls on the final addon. Do not associate the earlier timings with a different
addon hash without this qualification.

The final addon passes 63 hardware-enabled board tests, including second-call
aligned failure recovery and high-worker auto admission, plus typecheck and
64 GPU / 60 CPU-only Rust tests. Its CPU-only binary is 3,474,432 bytes, SHA-256
`99ea066c64091167409810617d9b43c1e603b9b71775a1ddf94e01dc0725556e`;
`accepted-cpu-only-smoke/` exactly matches esp32c3 and reports CPU-only.
`imports-accepted.json` records Windows-system-only imports for both binaries.

`accepted-ESPower-gpu-stages/` independently verifies all 361,770 candidates,
materialized geometry bits, all terms, hard counts, optimized legality and
shortlist. Its Beam/local/repair checkpoints and complete output exactly match
`accepted-ESPower-cpu-stages/`. The 41.26 s verification run is excluded from
performance measurements.

`final-small-interleaved.json` alternates CPU/auto order within one process for
four cycles with the final addon and four workers, removing ordering drift from
the earlier separate small-board series. All outputs exactly match captures;
all calls remain CPU, with no GPU initialization.

| Small native task, warm median (range) | CPU | Auto |
|---|---:|---:|
| esp32c3 | 59.18 ms (57.75–63.37) | 57.24 ms (53.51–66.72) |
| ESPower | 4.806 s (4.795–4.849) | 4.829 s (4.814–4.850) |

The ranges overlap and the ESPower median difference is 0.46%; the same CPU
path avoids GPU dispatch costs. This does not establish universal auto
profitability on other hardware or input shapes.

## Historical handoff before optimization

No further implementation or measurements were run after the user requested
the final report. The implementation and the completed checks above are saved
in the working tree; the roadmap remains partially open.

The final addon also completed a single cold Telemetry native-pair check:
CPU 341.976 s, auto / CubeCL 330.183 s (1.036x). Both calls exactly match
the original capture and the new CPU reference; CubeCL reports no fallback.
This check is separate from the repeated series on the earlier measured addon.
Neither series measures the complete TypeScript board-placement pipeline.

Full GPU captures of representative boards, visual placement review and
end-to-end board-stage performance/quality acceptance remain unfinished.
The PortableScope stress replay was interrupted: only its Beam log
(530.788 s, hard count 9) was produced, with no complete output. It is not a
successful validation or an accepted timing result. No benchmark process
remained running at handoff.

The existing cross-block RF-style signal-path fixture still fails on both
the preserved baseline and the changed addon (7/8 tests pass). Other completed
checks are recorded above. No commit, push or pull request was created.

## Optimization follow-up, 2026-10-01

The user resumed implementation and explicitly replaced repeated median suites
with one CPU/GPU pass per test/version, reusing saved CPU evidence. The preceding
handoff describes the old implementation. Its complete tree/addon is retained
in `optimization-baseline/`; all paths below are under
`debugging/board-gpu-2026-10-01/`.

The audit found avoidable work beyond the inherent CPU route shortlist:
CPU materialized every proposed pose before GPU materialization; fixed parent
geometry/contributions were recalculated between chunks; rank terms and
shortlist selection were serial within each candidate/batch; 512-candidate
chunks caused excessive dispatch/readback. Auto also rejected workloads when
the host budget exceeded four workers, and the default backend was CPU.

Implemented corrections:

- Keep translation proposals compact on CPU; reconstruct component/pad geometry
  only for selected proposals. Replay each original rounding operation.
- Cache exact fixed parent geometry, unary terms and pair terms in resident
  buffers, bounded to 16 frames / 64 MiB. Keys contain template identities and
  translation bits; template resets invalidate the cache.
- Increase the batch ceiling to 4096, retaining the 64 MiB capacity guard.
- Use 128 GPU lanes per rank candidate for independent relation/alignment/path/
  ordinary-net terms, with accumulation in original CPU order. Parallelize
  shortlist selection and read its IDs/scores together.
- Store primitive-to-slot lookup and materialized packing union bounds;
  conservatively reject disjoint bounds before exact collision-box loops.
- Default to `auto`; keep small inputs on CPU. Cap auto's GPU host workers at
  four instead of rejecting heavy inputs on a six-worker host. CPU recovery
  retains the original requested budget and original input.

F64, candidate sets, search widths, route policy and complete CPU results remain
the reference. There are no new runtime/dependency/helper DLLs.

Intermediate build `build-optimized/` passed five focused GPU tests, 64 GPU and
60 CPU-only Rust tests and typecheck. `optimized-ESPower-verify/` independently
checked all 361,770 candidates, geometry bits, terms, shortlist and stage
checkpoints against `accepted-ESPower-cpu-stages/`; the 45.591 s diagnostic run
is excluded from speed measurements. A single complete Telemetry native pair
in `optimized-Telemetry-once/` took 198.125 s, with exact CPU/capture outputs.
The final build additionally contains the packing-bound optimization and auto
policy fix; its measurements are reported separately below.

### Final optimization build and one-pass performance

`build-optimized-final/manifest.json` binds unchanged final sources to addon
SHA-256 `2dbcddb15a43b5f19db26cb0a2c0bd9a712c063406e300d3a6d8b203383909d7`.
The addon is 19,256,320 bytes (19.26 MB); `imports-optimized-final.json` confirms
only the same Windows system DLL imports. The complete 20-test hardware GPU
suite passes, including exact geometry/terms/chunk-size checks, six-requested-
worker auto selection, shared-runtime/lease behavior, stage failures and second
aligned-call CPU recovery. Final Rust tests: 64 GPU / 60 CPU-only passed;
TypeScript typecheck passes. See `optimized-final-*-tests.log` and
`optimized-final-typecheck.log`.

Telemetry uses the same two native captures, four host workers, cache disabled,
and validation disabled during timing. `optimized-final-Telemetry-once/` is
**one complete GPU pass**, compared with saved CPU outputs; CPU was not rerun.
Both complete outputs exactly match CPU and the original captures; both report
CubeCL and no fallback.

| Telemetry native scope | Saved CPU | Previous accepted GPU | Optimized GPU |
|---|---:|---:|---:|
| Ordinary solve | 164.524 s | 155.347 s | 75.881 s |
| Aligned solve | 177.452 s | 174.837 s | 87.006 s |
| Both complete calls | **341.976 s** | **330.183 s** | **162.887 s** |

This is **2.099x CPU speed**, 52.4% less time, and 2.027x the previous accepted
GPU implementation. Saved comparisons are `accepted-Telemetry-cpu4-check/`
and `accepted-Telemetry-auto4-check/`; the replay's exact-reference argument
uses `final-Telemetry-cpu4/`, whose outputs also match. These are single-pass
measurements, not medians or a statistical stability claim.

| Stage | Saved CPU | Optimized GPU |
|---|---:|---:|
| Ordinary Beam | 140.805 s | 66.895 s |
| Ordinary local improve | 23.551 s | 8.891 s |
| Ordinary repair | 0.107 s | 0.054 s |
| Aligned Beam, including pairs | 151.772 s | 76.902 s |
| Aligned local improve | 25.560 s | 10.033 s |
| Aligned repair | 0.052 s | 0.038 s |

The unchanged 10,510,562 GPU evaluations now use 5,040 batches instead of
22,506. Ordinary/aligned controller joint-candidate counts remain 0/388.
CPU route shortlist and sequential search decisions remain CPU work; this
measurement excludes TypeScript portfolio, block assembly and post-place.
Do not describe the 2.099x native gain as an entire PCB pipeline gain.

### Full-board integration after optimization

A single forced-CubeCL ESPower capture in `optimized-final-ESPower-board/`
completed the entire pipeline in 45.408 s, `placementOk=true`, all 53 components
placed and no fallback. Its native input and complete native solution match
`capture-ESPower-cpu/`; final placements also exactly match that saved full CPU
board capture, including coordinates, orientations and locked components.
The historical full CPU capture took 52.795 s; it was not rerun. This comparison
is a single integration check, not a universal full-board speedup claim.
`optimized-final-ESPower-comparison.json` records the checks, including a byte-identical final SVG. The additional 24 native/assembly integration tests pass (`optimized-final-board-integration-tests.log`). Auto continues to
leave this smaller route-dominated input on CPU.

The expensive incomplete PortableScope stress run was not restarted, and no
new median series was run. The earlier RF-style path regression is unrelated
and remains open as recorded above. The demonstrated heavy-input gain covers
both complete Telemetry native solves; full Telemetry TypeScript portfolio /
post-place timing and broad full-board acceptance remain outside these new
measurements. Preserve this distinction when closing remaining roadmap items.
