# Changelog

## Unreleased - 2026-09-29

### PCB layout

- Let independent native block-batch jobs lend their bounded CPU execution slots while waiting for GPU admission, ownership and readback. Keep at most eight extra waiting stacks, preserve input-order result assembly, and include full-call CPU recovery in the same CPU budget. Report active/peak CPU slots and suspended worker time. This is bounded blocking compensation for block batches, not a completed continuation scheduler for board/refiner or a CUDA backend.

- Separate GPU hardware allocation limits from driver free-memory estimates. Low estimated headroom now reduces block, board and refiner batches instead of disabling the shared runtime during block setup; release idle scratch and request allocator cleanup under pressure. Board/refiner batch sizing no longer uses a fixed 64 MiB quota. Preserve one-candidate forward progress and whole-call CPU recovery on actual runtime failure. This remains estimated accounting, not guaranteed VRAM reservation or a completed asynchronous scheduler.

- Scope GPU device ownership to complete solver calls instead of idle runtime lifetime. Explicit `cubecl` waits for a busy device; `auto` can decline occupied device/local admission before GPU work starts. Keep the same process runtime and kernel cache, release ownership before original-input CPU recovery, and report process lease waits separately. Refiner admission wait does not consume its refinement time budget. OS lock waiting is still blocking and does not yet provide fair cross-process GPU scheduling.

- Persist strict F32 SPIR-V between processes using source/dependency/toolchain/build and device/options cache keys, checksummed entries and atomic writes. Invalid or unavailable cache data falls back to compilation; shader-audit runs bypass the cache. No extra runtime dependency or helper binary.

- Measure first-kernel host preparation and strict SPIR-V source compilation separately from accumulated queue/service time. Exclude GPU admission-controller windows overlapping cold kernel preparation, including concurrent sibling jobs, while retaining full startup time in telemetry.

- Add backend-neutral GPU memory-budget observations, with current Vulkan driver estimates cached for 250 ms and unavailable telemetry explicitly reported as unknown. Admission uses conservative per-workload scratch estimates to reduce overlap under memory pressure; block initialization uses the observed budget and backend allocation limit instead of a fixed 64 MiB cap. A dynamic 5% budget margin covers observation races. This is not complete allocation accounting or CUDA support.

- Start the adaptive GPU manager roadmap. Remove CPU-thread-count gates and the board auto four-worker cap. Run one full board packing with pair-normalized weak positional alignment instead of two full searches; production positional alignment does not trigger atomic pair expansion. Add process-wide FIFO GPU admission starting at four operations, isolated reusable per-flight scratch and throughput/latency depth trials; release the device-state mutex before execution/readback. Backend portability, dynamic device memory budgeting, cooperative CPU scheduling and load-based auto routing remain in progress.

- Relax Board Packager, block solver and post-place/refiner GPU workload admission floors by approximately 8x (integer floor, minimum one). Preserve concurrency/readiness requirements, device/input/memory guards and full CPU recovery. This broadens GPU eligibility; it is not a measured speed guarantee.

- Record capture backend requests and launch-time addon/source provenance before execution so interrupted captures remain identifiable; retain actual native backend/fallback decisions in summaries and enable Board Packager detail profiling. Capture continues to inherit backend settings without forcing CPU/GPU. Review incomplete PortableScope `2026-10-02T07-57-08-534Z`: Board Packager deliberately chooses CPU under `auto` (28 primitives/23 unlocked below measured floors), with 369.442 s beam; grouped FPGA/MCU also miss the primitive-count GPU gate. No full-board rerun.

- Extend the F32 CubeCL block solver to composite blocks and up to 256 components/primitives with resident geometry ranges, bounded candidate chunks, device memory guards and complete CPU fallback. Add through-hole, topology/facing, long-net, power-yield, anchor and world-constraint scoring; fix frontier dispatch above 32 primitives. Full two-hypothesis DDR replay improves from 740.782 s CPU to 168.627 s GPU (4.39×), with valid final geometry and a +0.694% released-hypothesis score difference. Record single-pass evidence, recovery tests and remaining limits in `BLOCK_GPU_256_RESULTS_2026-10-02.md`.

- Preserve asymmetric component body offsets when native block search rotates or translates placements; transform original bounds around the component origin instead of rebuilding a centered rectangle. Match GPU candidate body translation to the shared native placement-grid rounding. Add regression coverage for all quarter turns and the ESPower U1–X1 clearance violation.

- Allow up to 0.01 mm per-axis native translation residuals when TypeScript applies a locked primitive, preserving its exact authored pose. Continue rejecting rotation, larger shifts and non-finite translations. This accepts PortableScope U6's spurious -0.001 mm metadata without changing native geometry or search.

- Complete and close the all-CPU/GPU F32 migration on `feat/pcb-f32-migration`: native binary32 storage and 4-byte GPU buffers, shared placement rounding/rotation coefficients, coordinate frames before narrowing, and TypeScript arithmetic helpers. Agree signed FTZ for CPU/GPU; enforce it through a scoped CPU environment and explicit shader bit operations because this RTX 3060 Vulkan driver exposes neither denorm preserve nor selectable FTZ. Remove the subsequent physical-interval geometry expansion: restore the existing body/pad transforms, intersection predicates and CPU/GPU materialization on F32. Keep coordinate transport, rounding/rotation semantics and conservative score pruning. Historical full-pipeline measurements are not measurements of this cleanup. Close the roadmap at the user’s instruction with documented coordinate-frame and ESPower route-quality limitations retained for later work; see `F32_MIGRATION_RESULTS.md` for the final artifact and checks.

- Implement one CubeCL F64 evaluator for local and final native Post-place/Refiner calls: resident topology, compact candidate batches, full score terms and ordered reduction, with streamed CPU route work under the existing CPU budget. Preserve stable results, legality and route-only improvements; GPU failure repeats the whole original call on CPU with a fresh full timeout. One-pass complete Telemetry refinement improves 17.634 -> 12.887 s with one worker (cold GPU, 1.37x) and 7.053 -> 5.649 s with four workers (ready shared GPU, 1.25x). Conservative auto retains CPU for small inputs and more than four workers; the tested six-worker CPU is faster. Full ESPower/esp32c3 placements and SVGs remain exact, without claiming a full-board speedup from the native result. Single addon: 20.088 MB / 19.158 MiB. See `docs/experimental/pcb/cubecl-backend/POST_PLACE_GPU_RESULTS.md` for quality, recovery, provenance and evidence limits.

- Implement the Board Packager CubeCL F64 backend with whole-call CPU recovery. Defer CPU proposal geometry, cache fixed parent contributions, batch up to 4096 candidates and distribute rank terms/shortlist selection across GPU lanes while preserving F64 accumulation order. The complete saved Telemetry ordinary/aligned native pair improves from 341.976 s CPU / 330.183 s previous GPU to 162.887 s GPU (2.10x CPU), with identical outputs in one pass using existing CPU references. Default to conservative `auto`, cap its GPU host workers at four and retain CPU for small inputs. The single Windows addon is 19.26 MB. This native speedup excludes block assembly, TypeScript portfolio and post-place; see `docs/experimental/pcb/cubecl-backend/BOARD_PACKAGER_GPU_RESULTS.md` for checks and full-board limits.

- Add a Rust/CubeCL F64 backend for supported native block Beam, singles and pair-reinsert batches, including GPU scoring, pruning, ranking and frontier checks. Reuse one process-wide Vulkan runtime; keep small/unsupported blocks on CPU and restart the original native block call on CPU after a GPU failure. On the saved four-block Windows/RTX 3060 package, warm full-cycle time improves from 130.600 s to 16.159 s; full FPGA time improves from 127.146 s to 13.748 s with identical reference outputs. Ship a single 17.687 MB Windows addon with no helper/compiler DLL or CPU JIT. See `docs/experimental/pcb/cubecl-backend/RESULTS.md` for scope, validation and limits.
- Use unified block checkpoint selection across beam, single moves, pair refinement, and role hypotheses, including blocks larger than twelve primitives. Retain only comparable legal variants and report selection diagnostics.
- Add fixed-connector-family placement in board context, adaptive block spacing, and conservative soft alignment/orientation proposals with a fallback to ordinary packing.
- Switch block scoring to geometric evaluation, retain route-aware checks at later board stages, and add incremental scoring and caches for stable block interactions, pad geometry, and access penalties.
- Batch independent block hypotheses, combine suspicious-role trials, and prune duplicate or provably dominated candidates. Add profiling for candidate generation, search stages, and concurrency. These changes do not establish that large-board runtime is acceptable; continue measuring full-board wall time.
- Respect board default layers, per-side silk and pad geometry, and fixed component origins throughout placement, scoring, rendering, and refinement. Preserve pad sides in block and board native contracts.
- Add PortableScope as a PCB placement fixture, restore signal-path guides and constraint-region diagnostics in regression artifacts, and save Telemetry and ICM20948 V2 comparison snapshots.

### Developer workflow

- Analyze the latest ESPower (50.620 s, invalid geometry) and PortableScope (727.516 s, failed before board packing) runs. Reproduce false locked-U6 transform metadata with a 1 ms native replay; identify delayed promise rejection, asymmetric body-box recentering and invalid empty-portfolio fallback, and DDR/composite GPU coverage plus pad-crossing/frontier scoring as optimization priorities. Add one ESPower placement CPU profile with identical output; no production solver behavior changed. Evidence: `docs/experimental/pcb/cubecl-backend/ESPOWER_PORTABLE_SCOPE_REVIEW_2026-10-01.md`.

- Review the in-progress F32 migration and rerun complete Telemetry once on the current release addon: 197.840 s with an explicit four-thread shared board/block budget and valid independent F64 geometry checks. Correct the previous 510.271 s report's worker count from four to one. Record CPU micro-routing and unsupported GPU block forms as the main bottlenecks, plus unresolved early TypeScript narrowing, physical-error propagation and quality acceptance in `F32_REVIEW_TELEMETRY_2026-10-01.md`; no solver behavior changed in this review.

- Plan an all-CPU/GPU PCB F32 migration in `docs/experimental/pcb/cubecl-backend/F32_MIGRATION_ROADMAP.md`: a new implementation branch with no legacy F64 solver, the frozen old revision as an external reference, localization before narrowing, explicit float controls/rotation/sentinel and propagated-error contracts, geometry-based quality acceptance, and single-pass full-cycle/time/memory checks. Implementation is underway; acceptance remains open.

- Extend exact native PCB capture/replay to local and final refiner calls. Add an immutable one-pass refiner experiment harness with input/addon hashes, saved CPU-reference comparison and independent placement validation; reuse CPU runs instead of mandatory median suites.

- Add separate GPU roadmaps for Board Packager and Post-place/Refiner, to implement sequentially on the shared compute runtime. Specify full-stage batching, scoring/constraint semantics, CPU replay, and independent correctness, performance and timeout-quality checks; Board Packager implementation and its measured native optimization are complete, with broader full-pipeline evidence limits documented; Post-place/Refiner is also implemented and accepted within its documented measured scope.
- Extract the existing CubeCL runtime and exact rounding helpers into the internal `compute` module in the same addon. Add consumer-local scratch layouts, typed runtime failures and device diagnostics; preserve block F64 scoring, process-wide session/lease, flags and original-call CPU replay. Validate exact results, recovery and the single 17.702 MB Windows addon on RTX 3060; timing remains within repeated baseline variation. See `docs/experimental/pcb/cubecl-backend/GPU_INFRASTRUCTURE_API.md` and `GPU_INFRASTRUCTURE_RESULTS.md`; board packing and post-place/refiner now use this same runtime.
- Implement the roadmap's user-approved first-stage GPU F64 and whole-block CPU restart, with separate exhaustive candidate/pruning validation and full-cycle benchmark tools. Defer all-CPU/GPU F32 migration to a separate future task without a date; retain the approved CPU F64 / GPU F32 intermediate option. The proposed 1.6x F32/F64 performance difference remains an unmeasured hypothesis.
- Add an isolated F64/F32 FPGA block-search precision check with exact F64 replay, offline cross-scoring, placement previews, and evidence of clearance-boundary score discontinuities. Production precision remains F64.
- Refresh the USB block CPU reference with user approval; record the pre-existing placement regression as open technical debt rather than treating the fixture update as a quality fix.
- Scope the CubeCL implementation roadmap to native block solving and saved FPGA replay; preserve prior GPU experiments, require reuse of the fast full-score path, and record validation/performance evidence before accepting each milestone.
- Keep PCB capture console output short; save full child-process output in `run.log` and document the `runs/` and `replays/` artifacts.
- Keep root agent rules short, add scoped PCB/native/circuit instructions and an architecture map, and move preserved PCB experiments to `docs/experimental/pcb/` with updated references.
- Add opt-in PCB layout capture and replay for exact native block and board inputs, outputs, stage timings, and Rust profiles; save source fixtures, visual previews, and assembly JSON in timestamped `debugging/pcb-layout/` runs.
- Consolidate generated results under `debugging/`, including circuit layout galleries and pattern tests. Add a repository-wide `AGENTS.md` with experiment-record and performance-check requirements; remove retired placement experiment runners.

## 0.3.5 - 2026-09-26

- Hide console windows on Windows when starting PCB layout and subtree placement process workers.
- Pin workerpool to 10.0.3 and extend its fork-option validation for Node's `windowsHide` option. Preserve process isolation and fall back to default fork options if the internal allowlist is unavailable.

## 0.3.4 - 2026-09-26

- Add a regression assertion for CPU-capped direct native refinement search, including hosts where the requested thread count exceeds the available concurrency budget.

## 0.3.3 - 2026-09-26

- Make refinement and route-cost regression tests respect host CPU caps instead of assuming a fixed worker/thread count.
- Keep serial/native comparison coverage valid on small CI machines.

## 0.3.2 - 2026-09-26

- Run complete route-aware post-placement refinement in Rust with parallel candidate evaluation and one native call per search.
- Adapt refinement to component/pad complexity (3–16 passes) and enforce a cooperative 30-second native budget.
- Cap placement concurrency at half available CPUs, with a maximum of eight workers.
- Remove the retired TypeScript refiner from tracked sources and retain native correctness, timeout and package regression checks.

## 0.3.1 - 2026-09-25

- Enforce schematic wire clearance for perpendicular approaches and endpoints as well as parallel segments, including rigid route moves during refinement.
- Preserve valid schematic crossings while rejecting foreign-net wire approaches that violate the configured gap; add endpoint-clearance regression coverage.

## 0.3.0 - 2026-09-25

### Component and schematic contracts

- Resolve devices from accessible EasyEDA public libraries and preserve library-qualified part references alongside legacy LCSC UUIDs.
- Support per-connection directional net-port styles and use bidirectional net ports as the default schematic marker.
- Preserve multipart section indices even when intermediate sections are unused, and reject connections to pins absent from the library symbol.
- Expose EasyEDA symbol data for host-provided component previews.

### Schematic placement and routing

- Add dense-pin padding and reserve space for client wire labels while keeping dense labeling independent of port style. Limit wire-label padding to symbols that need it, preserving ordinary two/three-pin component bounds.
- Straighten short power-rail steps and arrange resistor pull banks on short shared buses.
- Force required cross-page ports, preserve them across dense blocks and keep dense local/cross-page signal groups as named wires.
- Improve independent block placement and routing, softly align IC rows and page blocks, and refine layouts with deterministic order seeds.
- Prefer readable orientations for short connectors and improve port orientation on passive supply branches.
- Reroute excessive wire detours and local obstacles; place long-link ports near their pins to reduce unnecessary runs.
- Align passive ladder patterns by pin geometry and compact passive supply branches.
- Add a PortableScope schematic regression bank and live schematic examples for the ADC/clock layout cases.

## 0.2.0 - 2026-09-19

- Allow fixed placement for every component role and add legalized local block
  layouts while preserving seed topology during refinement.
- Add bounded micro-A* routability scoring and route-aware post-placement
  refinement for swaps, rotations and local placement alternatives.
- Compare route candidates on identical routing obligations, preserve endpoint
  sibling-pad obstacles and report route-aware placement evidence.
- Improve ordinary-net affinity, passive-block handling, edge-group validation
  and local ground proximity for compact board placement.
- Cache board-outline distances and exact native solves, prune redundant route
  probes and accelerate grid conflict and A* searches.
- Expand placement reports and the published placement DSL for the new routing
  and local-layout behavior.

## 0.1.0 - 2026-09-12

- Initial standalone backend release with component resolution, schematic
  extraction, PCB placement, workers and native binaries for supported hosts.
