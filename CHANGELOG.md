# Changelog

## Unreleased

### PCB layout

- Add an optional native Metal runtime on macOS 15+, disabled unless `PCB_GPU_METAL_DISABLED=0` (also with `PCB_GPU_RUNTIME=metal`). CubeCL's MSL is compiled with safe math, no contraction, precise `sqrt` and explicit FTZ `min`/`max`, and a device is admitted only if live F32 probes match the CPU bit for bit; any failure replays the call on the CPU. In `auto`, Metal takes only the initial stage of blocks with at least 80 netted connection points, one hypothesis per batch; board packing and post-place refinement stay on the CPU, and CUDA and Vulkan policies are unchanged. macOS builds compile only MSL and no longer need the Vulkan SDK. Over six fixtures, placements are identical to the CPU and the total is 333 s instead of 350 s (ICM20948_v2 −24 %, H743 +3 %); see [Metal runtime](docs/experimental/pcb/metal-runtime-2026-10-20/README.md).
- Check schematic block declarations against the authored DSL: a block preview or `preserve(...)` no longer gives false declaration warnings, a satellite counts as part of its parent's module, and a malformed declared block is skipped with `PCB_SCHEMATIC_DECLARATION_ENTRY` info while the others are still checked. `pcbLayoutDslSkeleton()` breaks cycles of declared attachments and warns about oversized blocks.
- Honor explicit `PCB_LAYOUT_SUBTREE_WORKERS`, `PCB_BOARD_PACKER_THREADS` and `PCB_POST_PLACE_THREADS` values up to the number of logical cores, in TypeScript and in the native board packer and post-placement refiner. The default stays at half the logical cores, at most 8. On H743 (18-core host, CPU build) poses are identical and 8 / 16 / 16 reduced wall time from 112 s to 93 s; see [thread overrides](docs/experimental/pcb/thread-overrides-2026-10-17/README.md).
- Add an opt-in persistent layer to the native solve cache (`PCB_NATIVE_SOLVE_CACHE_DIR`), bound to the exact native binary and backend selection. Rerunning H743 unchanged took 8.6 s instead of 91.0 s, and 38.9 s instead of 92.3 s after changing one block, with identical poses; see [persistent solve cache](docs/experimental/pcb/persistent-solve-cache-2026-10-18/README.md).
- Solve the block pair neighbourhood speculatively on the block's thread budget: consecutive pairs run on worker contexts against one state, the first accepted pair in order wins and later pairs are solved again, so results equal the serial loop. Single hypotheses use the batch API when more than one thread is available. On H743 the MCU pair call took 8.8 s instead of 28.8 s with 8 threads, with byte-identical solutions. Each pair job works on an unshared copy of the block, because shared `Arc` counters made pairs about 2.5× slower inside a worker (and `rp2040_base` slower than serial); over five boards at 2 / 16 / 16 the total is 230.4 s instead of 280.7 s on `main`, with identical poses and no board slower. See [parallel pair neighbourhood](docs/experimental/pcb/parallel-pair-improve-2026-10-17/README.md).
- Use the CPU's nominal primitive size in the GPU long-net scale. The GPU took the extent of the rounded bbox, which differs for off-grid boxes (STEPPER_CONTROLLER `pd_controller`) and could select another pose. The GPU primitive record grows to 16 ints and the frame header to 45 slots. CPU placement is unchanged.
- Apply the native bounds clamp in the compact `fit_to_bounds` instead of asserting an unbounded problem. GPU frontier scarcity reached it for bounded blocks (STEPPER_CONTROLLER, ESpower), and the whole block was replayed on CPU. CPU placement is unchanged.
- Recover bounded compact GPU poses from the native candidate box edges instead of rounding the centre difference. Half-tick centres (H743 `usb_power`) rebuilt boxes 0.001 mm away from the native candidates, so GPU and CPU searched different candidates. CPU placement is unchanged.
- Give the pair-search GPU kernel loops (`insert`, `convex`, `full`, `rank`) a single structured exit. CubeCL lowered their inner `break`s to unstructured SPIR-V that `spirv-val` and strict drivers reject. Kernel values and the CPU path are unchanged.
- Reuse existing `u32`, `bool` and vector types when `enforce_modes` adds the explicit FTZ sequence to strict F32 SPIR-V. Duplicate non-aggregate type declarations are invalid SPIR-V, and a strict driver (Mesa KosmicKrisp on Apple Silicon) rejected four kernels. The emitted arithmetic is unchanged.
- Accept optional schematic block declarations in `PcbSchematicGroups` (`PCB Block`, `PCB Module`, `PCB Attach`, `PCB Role` properties) and warn when the DSL contradicts them; without declarations, results are unchanged and every failure of the new checks is reported as info. Export `pcbLayoutDslSkeleton()` for the declared `block()`/`module()` structure; see [intent validation](docs/pcb-intent-validation.md).
- Queue every subtree worker task before solving the other board children on the calling thread, and allow fixed and edge families in workers. Synchronous children used to delay each later dispatch, so worker children of Portable_AC_DC_Telemetry started 17.9 s into the block phase. Over the 15 layout fixtures, poses are identical and the total is 566.5 s instead of 610.4 s at 4 / 16 / 16 with `PCB_LAYOUT_SUBTREE_MIN_COMPONENTS=4`; see [subtree dispatch](docs/experimental/pcb/subtree-dispatch-2026-10-18/README.md).
- Snap each edge gap of the shared clearance check (`geometry_policy::overlap`, CPU and GPU) to the whole tick when it lies within the F32 representation error of the two edges. Grid boxes one tick under the clearance read as legal at one position and as a 0.001 mm overlap at another, so centring a block could add a hard violation (H743 `buck_5`, C23 and C24 at 0.099 mm with 0.1 mm clearance). The USB boundary test now shifts an edge by 6e-7 mm instead of 2e-7 mm, which is below that error near 2.5 mm. Over the 698 captured block inputs of the 21 layout fixtures, 671 solutions are identical, 8 lose a false 0.001 mm overlap with the same poses (ESP32_C3_MAN `digital_input_1..4` from 1 hard violation to 0), and 17 reach other poses, 15 of them with a lower score. Board and refinement calls are identical except the ThunderF722 board score (+0.125 of 1456448).
- Round the block solver's working pad access rows to the placement grid on load, as a move to the authored pose and the GPU already did. A component that never moves (an authored fixed connector or MCU) kept its off-grid input rows on CPU, so its dense IC access term differed between CPU and GPU (H743 USB1: 298.1422 against 298.2039). Moves still rebuild from the authored rows and fixed poses are untouched. Over 21 layout fixtures, every final pose and every board and refinement call is identical to `main`; three block calls with an unmoved fixed MCU change only their score (ESP32_C3_MAN −1 of 13.3 M, Portable_AC_DC_Telemetry +0.02 and +0.08). See [access rows on load](docs/experimental/pcb/access-rows-on-load-2026-10-19/README.md).
- Run the board alignment finishing pass (`refineBoardAlignment`) in one native call, like post-placement refinement. Rust generates every candidate, evaluates the objective, the electrical guard, the placement-report geometry, mandatory hints and the native change validation, and accepts moves in the original order; the TypeScript arithmetic and per-call native frames are reproduced, so placements, accepted moves and rejection counters are identical to the previous pass (compared in full on every rejection stage, STM32G0 and Telemetry). Candidates update only the components they move, the route term re-evaluates only the terms of moved pads and changed links, and a pair's candidates run on the post-placement thread budget with results independent of the thread count. Over the 15 layout fixtures at 2 / 16 / 16, poses are identical and the total is 429.9 s instead of 525.3 s (Portable_AC_DC_Telemetry 185.9 → 112.1 s, its alignment pass 80.7 → 0.33 s); see [native board alignment](docs/experimental/pcb/native-board-alignment-2026-10-19/README.md).

## 0.4.0 - 2026-10-14

### Release validation

- Build the RF signal-path test footprint through the actual EasyEDA importer, retaining its original body/pads and success/clearance assertions. The old manual footprint bounds excluded protruding pads.
- Compare bounded USB experiment moves with the current native baseline instead of a historical F64 rank, retaining an independent zero-hard-violation check for that baseline. Update the saved Telemetry alignment check to verify rejection of unsafe moves under current assembly clearances; retain USB profile hard-legality and RF clearance checks.
- Update stale tests for the F32 physical tolerance, required component roles and native refiner contract v3. Check experiment opt-in behavior against the current solver rather than exact legacy F64 poses; retain independent quality and hard-constraint regressions.
- Add mandatory CPU recovery checks with GPU disabled or CUDA/Vulkan unavailable, including empty and locked boards and installed-package workers. Bound native test subprocesses and expose focused recovery checks through `test:recovery`.
- Reject known legacy native release binaries and verify host numeric/score/refiner contracts. Prepare macOS CI's Vulkan build SDK and check Linux/macOS runtime dependencies; target-platform CI remains required.

### Component libraries

- Accept optional `edaEdition: 'easyeda' | 'jlceda'` API options for component search, symbol previews, schematic assembly and PCB footprint resolution. Default to international EasyEDA; use JLCEDA's API domain when explicitly selected, including PCB workers and edition-separated caches. Keep UUID and DSL formats unchanged.

### PCB layout

- Account for collision envelopes that alias primitive BBOX bounds when sizing GPU candidate hull buffers. Prevent integer overflow and undersized buffers on large hulls; retain collision rules, clearances and scoring. Cover alias, stored and mixed bounds in Rust and a greater-than-64-point hull on real GPU against CPU.
- Keep USB clearance boundary classification stable through centering: compare F32 gaps with the existing 0.001 mm tolerance and the rounding allowance of the clearance subtraction, preserving full violation severity. Align placement reports with the shared native CPU/GPU predicate. Restore GPU block coordinates from resident placement origins, including moving candidates and origins following `@hint:` points. Match CPU rounding of zero-rotation component templates; transport candidate BBOX bounds independently and reuse them for identical collision envelopes. Strict USB CPU/GPU verification passes without relaxing tolerances. Placement score contract v13 rejects older native binaries; see the [pre-release review](docs/experimental/pcb/release-0.4.0-minimal-fix/README.md).
- Keep empty and fully locked Board calls on CPU even when CubeCL is explicitly requested. Skip GPU control/phase cleanup when there are no movable primitives, retaining the complete CPU result and locked poses.

- Add independent `solver({ previewBlock })` and `block(..., { fixed: { reference, x, y, rotate } })`, both with or without `localLayout`. Preview resolves and fully solves only the selected block family, skips board/global stages, returns coordinates/SVG/diagnostics without a board assembly, and marks unaccepted recovery layouts as failures. Fixed root families use ordinary local search, then retain all transformed component poses through board packing and refinement, including explicit refine groups. Scope preview local-layout metadata and retain internal signal-path segments. See [block preview and fixation](docs/pcb-block-preview-fixed.md).

- Add offline `validatePcbLayoutIntent` with the same input as `makePcbLayout`, English JSON diagnostics and reusable `detectCircuitPatterns`. Use one registry and one file for each of 57 conservative checks; reuse the production schema/validators and restore the placement helpers unchanged. Remove manual knowledge profiles and profile-only checks. Recognition uses schematic topology and labels; it does not place components or change native scores.

- Scope ambiguous rail-passive candidates by explicit assignments, unique direct-wire islands and root/satellite families across intent patterns. Treat rail-bank candidates as complete alternatives instead of Cartesian warnings; accept small dedicated families for ordinary supply locality while keeping critical loops and individual supply pins strict. Accept structured groups/wire islands through the editor-independent `PcbSchematicGroups` contract; report conflicting grouping/ownership as info and abstain on ambiguous or partial wire evidence. Keep the layout input and LLM parameters unchanged.

- Add `debug:pcb-validation` for fixtures or standalone DSL/schematic files, optional footprints/existing placement and saved diagnostic/input snapshots. Warnings fail only with --strict; info stays advisory.

- Warn for unbound connectors and fixed positions farther than a 5 mm perimeter guideline, using shared geometry helpers. Report board-level info when neither mounting-hole components nor DSL holes are present, including during previews. See [intent validation](docs/pcb-intent-validation.md) for coverage limits.

- Share a 0.001 mm physical geometry tolerance across native CPU/CubeCL Block, Board, FastRoute and placement validation. Ignore only boundary noise; preserve full severity beyond the tolerance. Remove false fixed collision penalties reproduced by ESP32 digital_input_1, align PostPlace/report edge allowances, and keep score ties and exact ordering separate. Placement score contract v12 requires a native rebuild; evidence: `docs/experimental/pcb/geometry-tolerance/README.md`.

- Evaluate the complete Board repair batch through the existing GPU rank API instead of requesting an oversized shortlist. Preserve all repair candidates and scores; avoid an unnecessary whole-call CPU fallback above 96 variants, reproduced by the ESP32 orientation-weight experiment.

- Add optional native joint proposals for 2–16 repeated channels at a shared locked connector, using shared CPU/GPU geometry and full FastRoute selection. Generate the nearest pair and complete eligible group (capped at 16); keep independent placement available. Keep beam width unchanged, preserve fine-position score elites and reserve half the slots for geometric diversity. Enable a weak normalized orientation preference independently of generation (at most four total points). Score contract v11; evidence and limits: `docs/experimental/pcb/channel-search/README.md`.

- Preserve placement hint semantics across native Block/Board and PostPlace: away_from/clearance use bbox gaps and authored minima, layer hints support groups without distance attraction, selected board edges retain their meaning, and clearance(group, all) excludes source members. Share CPU/CubeCL arithmetic and relation encoding; pass ignored nets into Board FastRoute. Keep existing PostPlace weights and physical constraints. Placement score contract v10 requires a native rebuild; evidence and legacy edge-orientation limitation: `docs/experimental/pcb/condition-semantics/README.md`.

- Preserve every ordinary net's distance in Board connectivity using a weighted sum with capped total pair strength. Share pair planning and distance arithmetic across native CPU, Board GPU and Block GPU; replace the 30 mm saturation with linear growth up to 40 mm and a 0.25 slope beyond it. Placement score contract v9 requires a native rebuild. Experiment evidence: `docs/experimental/pcb/ordinary-net-calibration/README.md`.

- Calibrate shared FastRoute crossing weights while keeping ordinary crossings cheap: directed critical/critical, critical/path and path/path costs are 30/60/90. Reduce the existing soft non-core pair/path cubic excess coefficient from 1 to 0.25, preserving linear distance, core weights and physical constraints. Nine debug captures select the moderate profile: STM32 retains D4 at 0° and reduces explicit crossings from four to two; remaining RS485 crossings are unresolved. See `docs/experimental/pcb/route-weight-calibration/README.md`.

- Prepare derived block group points before validating single/batch pair-search continuations, so `pairSeed` and source primitive inventories agree. Preserve strict inventory and locked-pose checks; add CPU/GPU continuation regression coverage.

- Preserve nested block geometry in Block Solver and module geometry in Board Packer using shared native group endpoint preparation and existing CPU/GPU point transforms. Encode module membership from the placement tree, reuse the same preparation in quality evaluation, and reject references spanning independent rigid owners. No weight changes; placement score contract v8 requires a native rebuild. See `docs/experimental/pcb/block-relation-context/README.md`.

- Preserve authored block relation geometry in Board Packer after satellite/island folding: resolve `block:` endpoints from original component ownership and move their group center with the rigid parent through existing CPU/GPU point transforms. Restore references whose tree identity disappeared, without changing relation weights. Placement score contract v7 requires a native rebuild. See `docs/experimental/pcb/block-relation-context/README.md`.

- Replace the remaining Micro-A* implementation and comparison APIs with a canonical full-layout FastRoute objective shared by block/board search, quality/checkpoints and post-place. Stop accumulating insertion-history penalties; remove duplicate straight-line pad/crossing costs, and batch post-place route jobs through the existing shared CPU/CubeCL arithmetic and whole-call recovery. Placement score contract v6 requires a native rebuild. See `docs/experimental/pcb/unified-fast-route/README.md`.

- Evaluate substantial board and block geometric route corrections in CubeCL batches when their solver uses GPU; keep tiny batches on shared CPU arithmetic to avoid dispatch overhead. Reuse one CPU/GPU path-arithmetic implementation, shared connection planning and ordered F32 reductions; cache layer filtering and intern repeated obstacle arrays. Preserve lazy candidate selection and full-call CPU recovery on GPU failure. See `docs/experimental/pcb/geometric-route-gpu/README.md` for validation and measurements.

- Revise STEPPER_CONTROLLER for a clear top-side NEMA 17 motor face, two-sided screw access and an axis-centered bottom AS5600. Retain 45 mm width, grow height to 66 mm, inset ESP32-C3 over an antenna notch without protrusion, move through-hole interfaces outside the motor footprint and add functional satellites, ordered paths, hard power islands and fixture geometry assertions.

- Redistribute STEPPER_CONTROLLER data/UART USB ports along the bottom-side motor-area edges, preserving user-approved flush USB tabs, actual electrical pad occupancy, supported pads and screw access. Keep the protruding driver module on top; make status pairs independently movable with electrical relations, and save the new full-run evidence and checked reference with explicit local repair provenance.

- Add live EasyEDA ESP32_C3_MAN (107 components) and Portable AC/DC Telemetry (154 components) placement fixtures, original netlists and PCB snapshots, functional DSLs and offline electrical/ownership validation. Preserve the telemetry outline notch. ESP32_C3_MAN uses an authored 120 x 85 mm outline, locked edge connectors and controls, antenna overhang, mounting holes, shunt-access exclusions, a hard buck switch-node constraint and PowerPAD thermal geometry; verify mechanics and placement with the current local backend.

- Revise Portable AC/DC Telemetry with locked source-facing interfaces, notch-relative ESP antenna placement, explicit KF301 mating direction, Kelvin/divider paths, attached support groups, four hard switching islands and real exposed-pad thermal geometry. Require clean placement geometry and preserved mechanics in its fixture runner.

- Separate Portable Telemetry measurement and harvesting-power areas using functional placement exclusions; keep converters out of the upper-right analog area and remove competing J1/MCU attractions that mixed sensitive inputs with power and modem circuits.

- Replace pin-count Dense IC halos with native pad-row access profiles: 0.127 mm track, 0.254 mm copper clearance and 0.6 mm via land. Count connected row pins, account for central copper/holes and jointly occupied inward via sites; include connectors. Share the row obstruction cost across block, Board Package and PostPlace CPU/CubeCL, without the former 20-component cutoff or compactness scaling. Add a 0.1 mm assembly margin above board component clearance for eligible ICs/connectors. Remove the TS pin-count clearance formula; score contract v5 requires native rebuild.

- Family search now releases only ordinary R/C/L support groups together. Keep crystal/mixed assemblies, bypass/core islands, fixed geometry and authored layouts rigid; retain unconstrained capacitor-cluster relaxation and full Beam/singles/pairs search.

- Family admission: critical distance priority remains a soft native objective instead of becoming an implicit PostPlace/checkpoint veto. Preserve explicitly authored hard non-core distances and penalized core drift. Label an empty-portfolio recovery as unaccepted rather than a legal score winner.

- Experimental explicit geometric connections: include authored pad-to-pad signalPath/criticalPair segments even when absent from the ordinary net tree; deduplicate matching segments and use shared Rust/CubeCL crossing importance (1/2/3/4/5) and pad multipliers (1/2/3). Preserve distance scoring and final threshold 10; requires score-contract v4 native rebuild.

- Experimental Board Package geometric routing correction reuses `fast_route` instead of Micro-A*. Transport and transform owned pad obstacles; include pad/clearance/ratline penalties and obstruction of existing links by newly placed pads. Final PostPlace also selects its existing native geometric mode without Micro-A*; preserve block policy, relation weights and search budgets. Record the user-tested reduction of the final PostPlace improvement threshold from 100 to 10.

- Experimental branch: share Rust/CubeCL distance-relation profiles across block, board, core-pair islands and PostPlace; normalize explicit signal-path topology weights and component endpoints. Remove duplicated distance formulas and hidden priority/stage multipliers. Score contract v3 requires a rebuilt addon. Core hard distance permits drift with x100 excess cost in the shared profile and explicit island warnings; physical overlap remains hard. Integration quality is not accepted: see `docs/experimental/pcb/unified-relations/README.md` for failing checks and remaining semantic gaps.

- Reuse the block passive-pin-to-IC stretch penalty in PostPlace CPU/GPU and native score inspection, scoped to original block/family ownership. Preserve weights, search and the 100-unit threshold; require a rebuilt score-contract v2 addon. Validation deferred to the user.

- Use normal priority for inferred small-block ground links and connect bypass hints from the capacitor pad on the target net to the authored IC pad. Preserve local-ground eligibility, distance limits and PostPlace hint expansion.

- Select the best distinct block hypotheses for pair refinement within the existing two-seed budget without rejecting them by intermediate electrical, wire or link-distance ratios. Preserve native scoring and pair-stage eligibility limits.

- Require more than 100 units of effective improvement in final post-place refinement. Preserve local block thresholds and the existing Micro-A* routing evaluation.

- Allow consecutive signalPath segments to share the same intermediate pad as a same-net waypoint. Preserve net/continuity validation and reuse native zero-length-edge handling. Route the STM32 fixture's CAN and RS485 placement paths through D4/D5, retaining the soft 5 mm protection-to-connector limits without duplicate criticalPair rules.

- Preserve each grouped/released block hypothesis's relation endpoints when evaluating checkpoints, pair seeds, and postrefined candidates. Released satellite anchors must not disappear during portfolio selection; keep canonical component roles and existing search budgets/weights.

- Make Rust authoritative for PCB placement scoring: use the block search evaluator for checkpoint selection and the board packer evaluator for board portfolios; reuse native spacing/alignment and MST measurements. Move remaining island/orientation/quality criteria into Rust, obtain net classification and default weights from Rust, and require the native score API contract. Restore the original CPU/GPU compactness costs. Preserve the preceding power-spread and passive-to-IC stretch changes; document retired TypeScript formulas and stage boundaries in docs/experimental/pcb/rust-score-authority/README.md.

- Add the live-exported STM32G0 CAN RS485 Controller 60x40 placement fixture: all 68 components across three schematic pages, functional placement DSL, electrical pin/net validation and assertions for geometry and the hard buck switch-node limit.

- Supplement board local improvement with free/contact positions from separate top/bottom envelopes, preserving narrow off-grid slots and the existing coarse beam portfolio. Add actual same-layer contact repair alongside whole-block escapes. Align final report and post-place edge validation with board packing's body/opposite-face geometry, avoiding false edge errors from decorative footprint bounds.

- Treat zero-size EasyEDA drill placeholders as SMD pads on their declared side; preserve explicit multilayer and positive-drill through-hole pads. Fix false opposite-side occupancy for PortableScope C95/C122 and U8.

- Fix EasyEDA footprint bounds for ARC/CARC and Bezier contours, including nested circles; retain NGON pads and center asymmetric polygon pad envelopes on their geometry. Cover the real 10mm capacitor footprint, curve extrema, pad forms, rotations and both PCB sides.

- Fix CubeCL block external-relation endpoint selection: use a runtime index instead of a constant-branch expression that compiled to side one and could read the missing endpoint. NVIDIA Compute Sanitizer exposed the out-of-bounds access on the saved USB block; retain full-call CPU recovery and cover both relation directions.

- Bound CUDA allocator streams to eight instead of the upstream default 128; clean idle allocations across streams and discard obsolete board GPU caches at phase barriers. Latch actual backend allocation errors before resolving unbound handles so they reach whole-call CPU recovery instead of repeated worker panics.

- Add a real CubeCL CUDA backend, selected before Vulkan only after dependency and live strict-F32 probes. Keep one addon, dynamic installed CUDA/NVRTC discovery, whole-call CPU recovery, CUDA free-memory observations and compilation timing. Pin NVRTC FTZ/no-FMA/precise arithmetic and release compiler programs. Vulkan remains available when CUDA is absent; other operating systems/backends are not certified by the Windows checks.

- Extend bounded GPU-wait compensation to Board Packager beam lanes with separately owned evaluators; keep local/repair CPU fallbacks within the original thread cap. Pipeline refiner GPU work with CPU route evaluation even with one CPU slot. Tune admission under sustained load, classify allocation failures, and preserve full original-input CPU replay with a fresh refiner budget.

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
