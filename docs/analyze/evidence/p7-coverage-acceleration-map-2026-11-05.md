# P7 case coverage and acceleration protection map — 2026-11-05

Read-only mapping of the current case contract to concrete evidence and to the physical families that an acceleration must preserve. This is a navigation aid, not a coverage pass or acceptance update.

## Authoritative coverage source

The required case matrix is [p7-case-matrix.md](../reference/p7-extraction/p7-case-matrix.md). Its 2026-11-05 update explicitly says full code coverage and physical acceptance across the matrix are unconfirmed. The 42-row acceptance manifest referenced by the integration ledger is a preparation/coverage inventory (`ready=0 pending=42`), not 42 executed cases and not a success rate. See [integration acceptance](p6-p7-integration-acceptance.md) for the original manifest and final obligation matrix. The manifest rows should be interpreted alongside—not as replacement for—the case matrix.

## Coverage and acceleration protections

| Case family (matrix) | Existing evidence / implementation | Acceleration protection that remains mandatory |
|---|---|---|
| Uniform straight sections, material/stack variants | P7a section evidence; captured-section and reuse evidence | Preserve cross-section mutual matrices, material/skin band, terminal mapping, and full requested band. |
| Bend, width/stack transition, pad/connector | segmented coupon controls; actual STM32 geometry planning | Keep local 3D regions at geometric discontinuities and finite physical contacts; certify section/local interfaces. |
| Through, blind/buried via, remote return | local volume/board-mesh evidence; STM32 exact request | Retain actual barrel span, remote return path and via-near/remote field/current effects. Do not clip to a convenient corridor. |
| Split/distributed planes and multiple return islands | corridor and shared-return evidence; finite-return refusal paths | Preserve distinct islands and distributed lateral return coupling; a local ground or resistor graph is not an EM equivalent. |
| Differential pairs, coupled groups, branches/meanders | coupled-section and multiport tests | Preserve off-diagonal coupling, branches, common and opposite drives, and all physical ports. |
| Component bridges and PDN multiports | typed component bridge and local PDN evidence | Preserve pad attachment, model band, losses and disconnected copper topology. |
| Layer count/asymmetry and invariants | case matrix includes 2/4/6/8 layers, multiple references, transforms and CAD partitioning | No single-reference collapse without equivalence evidence; retain gauge, rotation/translation, ordering/ID and partition invariance. |

## Reading status

Small coupon, analytic graph, manufactured and geometry-only controls cover mechanisms, not all real-board physical cases. In particular, the DC `shared_return_prototype` is a topology falsifier only; it reports no distributed EM metrics. The real STM32 automatic attempt selected joint planning and then refused at the RS485_B via arrangement guard before field solves ([run evidence](p7-stm32-automatic-2026-11-05.md)). The full case matrix therefore remains open. No solver, tests, or numerical runs were performed for this mapping.
