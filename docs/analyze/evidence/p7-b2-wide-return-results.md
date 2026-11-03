# P7-B2 wide-return geometry — bounded implementation

2026-10-31. Partial result; B2 real-channel acceptance remains open. No full-channel EM or memory admission.

The planner now distinguishes geometric section candidates from electrically admitted corridors. `RunCorridor.geometricIntervalsMm` and `wideReturnDomains` expose bounded numerical rectangles, inspected transverse bounds, unchanged original copper polygons, source/region/layer/island mapping, explicit numerical lateral cuts and required expansion/interface checks. `admissibleIntervalsMm`, corridorCells and the executor remain guarded. The successful synthetic geometry is never promoted to an EM result.

Candidate intervals use the existing exact complete-boundary event sweep over the entire physical layer union and board transverse extent, including holes, cutouts and bores. Source seams do not become events. This implementation is deliberately conservative: remote unrelated edges can blanket an interval. Lateral continuations have uniform-continuing-support/joint-event-required classification and physical island mapping. `geometricEvents` explains the joint classification. Multiple parallel runs keep the existing joint coupled envelope.

Source polygons stay complete; sources[].disposition is explicitly a **potential** bounding-box intersection prefilter, not a clipped-copper ownership certificate. Native board-section remains authoritative. Refusal audit retains complete original sourceCopper/physicalIslands/physicalContacts/bores and terminal contacts for C1. The full original copper enables later enlargement and topology checks. No name-based connections or artificial physical edges are introduced.

## Evidence

14 corridor/domain tests PASS, including finite physical edge, narrow slots on and outside the original width, slots between sample locations, remote branch and bore, physical source subdivision, separate same-name islands, 45-degree rotation and coupled neighbour. Existing board planner tests 12/12 PASS (including original STM32 endpoints, D5/JP2 branches, BOTTOM copper, finite via ownership and original guards). C1 coordinates typecheck/build.

`node --import tsx scripts/verify-p7-wide-return-domains.ts` ran the existing complete selected channels and captured original copper plus plots under ignored `debugging/p7-wide-return/`. Manifest DSL lacks explicit contact layers required by localRequest; the geometry harness adds TOP contact layer selection for the existing four selected terminals, without editing the source fixtures or endpoints. First harness attempt stopped immediately with Contact layer required; corrected harness ran once, no FEM.

| Full channel | Before admitted intervals / length | After geometric candidates / length | Source regions | Geometry + render wall s | cumulative peak RSS B |
| --- | --- | --- | --- | --- | --- |
| STM32 U5→J5, 2 layers | 0 / 0 mm | 0 / 0 mm | 844 | 1.827 | 198230016 |
| NanoScope, 4 layers | 0 / 0 mm | 0 / 0 mm | 3019 | 29.238 | 369369088 |
| ThunderF722, 6 layers | 0 / 0 mm | 0 / 0 mm | 1895 | 11.978 | 421642240 |

Wall values cover planner capture before rendering/serialization; peak RSS is Node process cumulative max, not per-case independent maximum. Private commit was not measured (null in evidence), so no private-memory acceptance. Plot PNGs for all three boards were opened and inspected: complete source copper, physical bores and four magenta contacts remain; gray envelopes show proposals, and absence of green domains correctly matches zero real candidates.

## Remaining blocker and handoff

A broad homogeneous control now yields one nonempty section candidate while production admission remains refused. However, sweeping the complete board width also includes unrelated remote events, yielding **zero** candidates on all three real full channels. Therefore this is a working bounded candidate interface and conservative topology-preserving implementation, **not completed B2 acceptance**. A physically justified finite transverse selection must replace this conservative whole-strip certificate before useful real-channel candidates can be claimed; C1 expansion and distributed interface/D work are still required. No heavy magnetic run, native change, DSL fixture edit, commit, roadmap or CHANGELOG edit was made.

C1 agreed to consume WideReturnDomain; original source geometry and refusal evidence paths were sent directly. The task stops here with these limitations explicit.
