# P7: route-aligned coupled partition and bounded block extraction

Status: implementation authorized; not accepted. Original P0–P15 numbering, analysis scope and public DSL remain unchanged. This replaces the automatic partition implementation inside P7; it does not certify all DSL analyses or arbitrary RF bandwidth.

## Size and boundaries

Current densely formatted TS files: board-channel-plan107 lines, board-channel-executor37, segmented-channel102, analysis-dsl/channel-plan60, local-request34 (~340 lines total, line counts underestimate complexity). Native board_mesh441 lines has axis-aligned-only cut contacts. Expect replacement of ~100–250 existing TS lines plus targeted native contact/domain changes, not a rewrite of all these files or P6. Planning estimate, not a quota: roughly 1–3k readable implementation lines across topology/grouping/interfaces/integration and comparable tests/fixtures/visualization; estimate must be revised if native geometry constraints require more. No number is an acceptance goal.

Preserve RoutingBoardV2 import, confirmed geometry/connectivity, cross-section solvers, local F64 solver, P6 network assembly, workers, model cache, DSL syntax. Replace global X/Y slab discovery and axis-based copper ownership with route topology, coupled runs and local junctions. Retain graph branches and actual source polygons. No skeletonization of arbitrary pours required: unrecognized geometry stays a bounded explicit local region.

## Shared physical contract

Exactly two block kinds remain: section and local. A section uses a longitudinal unit direction, transverse cross-section, real conductor polygons and physical terminal mapping. A local owns physical clipped source fragments. Boundaries carry stable physical conductor/face identity and orientation, not merely net labels. Common ground is not welded by name. Distinct signal/return contacts and floating neighbors must survive.

Native/TS agents must agree one internal oriented domain/contact representation by message and record it here or in their result documents BEFORE integration; no public DSL/transport change. Preferred representation: polygonal XY domain (existing rectangular ROI may remain as its bounds) and plane contact defined by originMm, normalXY and finite tangential spanMm plus original source/layer. Confirm exact clipping/contact feasibility first. Do not fake arbitrary corner cuts by rotating a whole multi-angle local region into one axis. Existing rectangular inputs remain valid special cases used by local fixtures; remove obsolete automatic planner, not valid rectangular geometry.

Coupling groups use spatial proximity plus actual environment/return assumptions. Preserve off-diagonal RLGC and joint via/junction models. Meanders and near crossings count even when distant in route order. No silently independent extraction of a bus and no blanket distance-only guarantee. Nonlocal shared-plane coupling must remain represented by joint local geometry or explicitly outside the supported partition; cannot promise universal128-line extraction. A full plane solver and full-wave are separate existing roadmap obligations.

## Acceptance sequence

1. Visualized geometry-only plans: straight0/45/90 degrees, bend, branch, via, two coupled lines, meander, real STM32. Ordinary diagonal uniform run must become a section; arbitrary polygons not confidently recognized cannot be forced into a section. No lost/doubled copper.
2. Native oriented contacts and P6 interface mapping: rotation/reversal invariance, conductor identity, finite returns, matching shared cut geometry, branch conservation. Independent reference and boundary-displacement checks; no changed goldens to accept a mismatch.
3. Full small-channel numerical controls: coupled aggressor/victim, via/return, split reference. Existing strict residual/passivity/reciprocity guards stay. <=8% final physical error with meaningful absolute floors near zero and reference uncertainty; no summing individual8% allowances. Observe existing band limits, do not extrapolate GHz admission.
4. Bounded sequential block lifecycle: share immutable geometry; retain factor for all RHS within one block, produce/cache reduced model, release full block allocations before next. Instrument active blocks and memory; prove actual release rather than adding redundant clears. Existing frequency-independent section cache remains; exact physical/band/core/port identity for local cache. No approximate via lookup library.
5. One supervised real STM32 cold/warm/restart after small prerequisites pass: goal4GiB/120s cold,5s warm,10s restart. Developer diagnostic cap8GiB/600s with host commit headroom and explicit OS-paging supervisor, no pagefile changes. Do not retry unchanged expensive failures. Report per-block unknowns/fill/peak/time and final model metrics; invalid geometry may not be silently dropped for speed.
6. Delete old automatic global-axis planner/helpers/flags and obsolete tests after replacement passes. Git is history; no legacy fallback switch. Preserve genuinely used numerical kernels, rectangular fixtures and references. No unrelated file cleanup, no deleting user native/analyze/debugging.

## Parallel ownership

- Agent route-partition: TS planner, internal ownership/coupling graph, executor/composition integration, TS tests and visualizations; final end-to-end acceptance owner after native agent handoff. Own CHANGELOG and roadmap synchronization. Does not edit Rust while native agent works.
- Agent oriented-boundaries: Rust board domain/clipping/contact support, additive N-API types required for internal transport, focused native geometry/numerical tests. Does not edit TS planner/executor or shared CHANGELOG. Coordinate exact interface with route agent, preserve baseline physical behavior.
- Only native agent runs heavy native builds initially. Handoff binary/build ownership explicitly to route agent; never two simultaneous large runs. No extra review agents, no subagents, no commits/push before parent review.

## DSL coverage (preserved requirements, not new readiness claims)

Impedance/S/TDR/SI/crosstalk/timing/eye need joint multiport models, physical contacts and validated frequency behavior; these must not be removed. PI requires finite return and appropriate plane/source/component data; this partition alone does not solve arbitrary power-plane behavior. Thermal/DC inputs and implementations remain untouched. EMI/antennas need field/source/full-wave capabilities beyond terminal RLGC/S data; do not replace original board geometry with a reduced model as sole input. Stackup/materials, component models and activity still come from DSL/resources. New partition preserves the inputs, but is not sufficient implementation for the whole DSL.

Final reports must distinguish working code, partial limitations, measured resources and remaining P7/P8/P11 dependencies. Normal native build for Rust changes; focused tests, TS typecheck/build, cache tests and actual plotted physical results when successful. No tests that merely assert the implementation's own classification without physical invariants.
