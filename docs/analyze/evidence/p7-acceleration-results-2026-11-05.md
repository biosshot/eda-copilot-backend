# P7 acceleration: first implementation batch, 2026-11-05

Subsequent implementation, commits, bounded full-path failures and final A–F disposition are recorded in the [attempt ledger](p7-acceleration-attempts-2026-11-05.md). The <=600-second target remains unmet.

Scope: stage A diagnostics and coverage inventory from the [acceleration plan](../plans/p7-acceleration-plan.md). No numerical operator, routing decision, physical geometry or frequency applicability was changed. Stage A remains incomplete; stages B–F are not implemented by this batch.

## Implemented

`autoFoilPlan` accepts an optional closure observer, threaded through planner options. It emits seed, barrel-envelope, intersecting-islands and completion events with iteration, ROI, retained counts, newly added physical IDs and direct intersecting-region IDs. Callers without the observer retain the existing selection behavior. The trace explains existing conservative selection; it does not classify omitted copper as physically irrelevant or authorize clipping.

`scripts/verify-p7-route-diagnostics.ts` reproduces the authored STM32 channel using tracked board/DSL inputs. It captures the segmented refusal and joint candidate without calling a field solver. The JSON report includes input hashes, planner source hashes, physical terminal requests, corridor intervals/events/lateral continuations, closure steps and timing. Alternate board/source paths are available through `P7_BOARD_PATH` and `P7_SOURCE_PATH`; the runner currently expects impedance tasks. The diagnostic intentionally attempts segmented geometry even for a higher-frequency input; it is not a reproduction of the production frequency dispatch and does not validate a frequency band.

Reproduce from backend root with `P7_OUTPUT_DIR` set to a dedicated directory, then run `node --import tsx scripts/verify-p7-route-diagnostics.ts`. Local result: `debugging/p7-route-diagnostics-20261105/route-diagnostics.json`.

## Measured planning result

RS485_B, U5.7/U5.5 → J5.2/J5.3, 10 kHz. Route reason: `finite-return-closure`, distributed interfaces across multiple corridors. Numerical solves: **0**. Measured planning/closure wall time: **0.490 s** (not whole process time).

| Iteration | Reason | Retained regions | Retained barrels |
|---|---|---:|---:|
| 0 | Initial signal/return/contact seed | 214 | 0 |
| 1 | Intersecting hole/plating envelopes | 214 | 89 |
| 2 | Bounding-box neighbours and their complete islands | 844 | 89 |
| 3 | Closure complete | 844 | 89 |

This identifies the expansion stage, not which of the 630 added regions can safely be omitted. A physically necessary remote return may be outside a small local window. It must remain explicit or be represented by a validated external model.

## Checks

- `npm run typecheck`: passed.
- `node --import tsx --test tests/analyze-board-channel-plan.test.ts tests/analyze-auto-foil.test.ts`: **18 passed**. Includes new traced/untraced equality and exact ID/count accounting on STM32; existing remote bypass, physical via, split return, CAD subdivision, coupled corridor and package controls passed.
- `node scripts/p7/p7-coverage.mjs`: **ready=0 pending=42**, preparation only. [Current manifest inventory and protection mapping](p7-coverage-acceleration-map-2026-11-05.md).

No native build, numerical field benchmark or full STM32 solve was required for this TypeScript diagnostic change. No speedup of electrical extraction or complete memory admission is claimed.

## Next work

Finish A's row-level crosswalk from the inventory to all current case-matrix obligations and resolve stale manifest prerequisites without raising acceptance statuses. Add bounded intermediate timing/counts to the current adaptive magnetic action (C), then compare repeated classification/geometric coefficients/scratch costs. Design and validate B's distributed interfaces before changing selection or clipping. CPU parallelism and removal of the frequency dispatch remain subsequent work under the plan.
