# Changelog

## Unreleased - 2026-09-29

### PCB layout

- Add a Rust/CubeCL F64 backend for supported native block Beam, singles and pair-reinsert batches, including GPU scoring, pruning, ranking and frontier checks. Reuse one process-wide Vulkan runtime; keep small/unsupported blocks on CPU and restart the original native block call on CPU after a GPU failure. On the saved four-block Windows/RTX 3060 package, warm full-cycle time improves from 130.600 s to 16.159 s; full FPGA time improves from 127.146 s to 13.748 s with identical reference outputs. Ship a single 17.687 MB Windows addon with no helper/compiler DLL or CPU JIT. See `docs/experimental/pcb/cubecl-backend/RESULTS.md` for scope, validation and limits.
- Use unified block checkpoint selection across beam, single moves, pair refinement, and role hypotheses, including blocks larger than twelve primitives. Retain only comparable legal variants and report selection diagnostics.
- Add fixed-connector-family placement in board context, adaptive block spacing, and conservative soft alignment/orientation proposals with a fallback to ordinary packing.
- Switch block scoring to geometric evaluation, retain route-aware checks at later board stages, and add incremental scoring and caches for stable block interactions, pad geometry, and access penalties.
- Batch independent block hypotheses, combine suspicious-role trials, and prune duplicate or provably dominated candidates. Add profiling for candidate generation, search stages, and concurrency. These changes do not establish that large-board runtime is acceptable; continue measuring full-board wall time.
- Respect board default layers, per-side silk and pad geometry, and fixed component origins throughout placement, scoring, rendering, and refinement. Preserve pad sides in block and board native contracts.
- Add PortableScope as a PCB placement fixture, restore signal-path guides and constraint-region diagnostics in regression artifacts, and save Telemetry and ICM20948 V2 comparison snapshots.

### Developer workflow

- Add separate GPU roadmaps for Board Packager and Post-place/Refiner, to implement sequentially on the shared compute runtime. Specify full-stage batching, scoring/constraint semantics, CPU replay, and independent correctness, performance and timeout-quality checks; implementation has not started.
- Extract the existing CubeCL runtime and exact rounding helpers into the internal `compute` module in the same addon. Add consumer-local scratch layouts, typed runtime failures and device diagnostics; preserve block F64 scoring, process-wide session/lease, flags and original-call CPU replay. Validate exact results, recovery and the single 17.702 MB Windows addon on RTX 3060; timing remains within repeated baseline variation. See `docs/experimental/pcb/cubecl-backend/GPU_INFRASTRUCTURE_API.md` and `GPU_INFRASTRUCTURE_RESULTS.md`; board packing and post-place/refiner remain future tasks.
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
