# Block scoring and board-context alternatives

Research branch: `experiments/block-placement-quality`. These changes are enabled in the repository's ordinary placement path, not only in the replay harness. The separately installed EasyEDA Copilot runtime is not updated by building this repository.

## Defaults and controls

Eligible ordinary blocks contain 2–12 child primitives belonging to one electrical block. Their default beam width is 4. The full profile enables pad-anchored net candidates, stable net weights, reduced hull-area weight, smooth aspect penalty, long-pair penalty, additional local passes, pair swaps, pair reinsertion and dense-pad access scoring. Families and module packing retain their existing policies.

| Environment variable | Default | Alternatives |
|---|---|---|
| `PCB_BLOCK_PROFILE` | `full` | `legacy` disables the previous experimental flags and the portfolio |
| `PCB_BLOCK_ROUTING` | `micro` | `geometric`, `off` |
| `PCB_BLOCK_PORTFOLIO` | `1` | `0` keeps one internal block layout; `2` also tries two board-wide repacking hypotheses |

Routing switches affect the ordinary block score. Board packing and the independent postrefine route check continue to use their existing micro router. `off` removes only the block routing correction; it does not remove ordinary wirelength or long-connection penalties.

## Architectural experiment

An unlocked eligible block retains up to three distinct internal arrangements: the full profile, a compact greedy variation, and a greedy net/long-pair variation. They remain attached to the primitive through hierarchy packing, translation and rotation. Nested children now rotate about the same origin as the parent; previously child metadata rotated about each child's own center while the parent's flattened placements used the parent origin.

After board packing, the selector visits retained blocks in deterministic order. It compares their alternatives at the current location and at small translations of one or two grid steps. Evaluation includes connections to the rest of the board. A change must improve the global score, leave the full board geometrically valid, preserve fixed components and component inventory, and introduce no new hard-constraint violations. The selector also prevents deterioration of reported hint violations. The hard validator is shared with postrefine; postrefine's move generation and purpose are unchanged.

This is a bounded first implementation of adaptation in board context. It does not move a neighbouring block aside or rerun internal placement with the neighbour as an obstacle. It can therefore reject a good alternative whose footprint cannot fit the current neighbourhood. Diagnostics distinguish an internal variant change from a translation of the original variant.

Mode `2` additionally packs the board with two alternative sets of internal block shapes, then chooses the admissible board with the best global score before the local selector runs. This permits neighbours to make room during packing. It remains three global hypotheses rather than the Cartesian product of every block's alternatives. Mechanical placements are preserved across hypotheses. Use `full-micro-repack` and `full-geometric-repack` in the harness to isolate this step.

## Fast geometric score

The geometric correction tries a direct segment, both L-shaped paths and four doglegs. It charges excess length, intersections with foreign pad boxes, intrusion into their clearance bands and projected crossings with other nets' spanning-tree segments. Same-net pads are excluded; opposite-side SMD pads are excluded when the endpoints establish a common layer. Through-hole pads obstruct both layers. Mixed-side connections and crossing hints remain conservative planar approximations.

External nets receive an additional bounded escape-to-block-boundary estimate. The clearance bands are a proxy for cramped access, not a measured routing-channel capacity. No A* is run and line intersections are not counted as actual vias. Existing wirelength and long-pair terms stay in the objective; the proxy adds routing difficulty rather than charging full length twice.

## Reproduction

Run from `eda-copilot-backend`:

```powershell
npm run native:build
npm run build
node --import tsx scripts/experiment-placement-architecture.mjs Telemetry full-micro
node --import tsx scripts/experiment-placement-architecture.mjs Telemetry full-geometric
node scripts/experiment-placement-matrix.mjs
node scripts/experiment-placement-matrix.mjs --repack
```

Other fixture names are `ESPower` and `esp32c3`. Modes are `legacy`, `full-micro-single`, `full-micro`, `full-off`, `full-geometric-single`, `full-geometric`. `single` disables the portfolio, not beam search.

Artifacts appear in `.test-output/architecture/<fixture>/<mode>/`: full placement, full-board and block SVGs, stage data, report, native block captures, timings, input/native hashes and diagnostics. Inputs are versioned resolved board snapshots; these commands do not alter a live EasyEDA document.

For the independent bounded routing probe and rendered comparison:

```powershell
node --import tsx scripts/experiment-route-probe.mjs Telemetry architecture legacy
node --import tsx scripts/experiment-route-probe.mjs ESPower architecture legacy
node --import tsx scripts/experiment-route-probe.mjs esp32c3 architecture legacy
node --import tsx scripts/experiment-architecture-report.mjs
```

The probe keeps a common baseline job plan across candidates, but samples only the micro router's bounded job set. It is not a complete-board routing test.

## Evidence

See [full-board measurements](results.md), [raw summaries](measurements.json) and [USB visual comparison](usb-comparison.html). Pair sum, worst-pair length and HPWL are separate metrics; a lower value in one does not imply improvement in the others. Timings are sequential single-run observations on this host.

Regression checks cover the ordinary branch policy, parent/child/alternative transforms, neighbour-aware selection, fixed components, keepouts, hard distance and bounding constraints, candidate inventory, routing modes and geometric primitive intersections. The existing raw-native baseline remains opt-in and preserves the captured USB solution.

## Findings from the ordinary-path comparison

The full micro-router profile substantially improves the primary USB block before board packing: aspect ratio 3.15 → 1.40, internal two-terminal length 44.64 → 25.67 mm, worst internal pair 11.68 → 4.99 mm, area 122.37 → 127.28 mm². See [local USB measurements](usb-local.md); these measurements must not be confused with the full-board totals.

At board level, `full-micro` changes the two-terminal sum from 781.84 to 821.22 mm on Telemetry, 95.23 to 93.52 mm on ESPower, and 302.77 to 306.04 mm on ESP32C3. Enabling every local improvement is therefore an experimental default, not a demonstrated universal quality improvement.

The geometric replacement does not beat the full micro-router profile on whole-board two-terminal length in these three fixtures. It reduces observed block-solving time, but this does not establish a reliable end-to-end performance gain. The micro router remains the default block routing metric; geometric scoring and removal of the route term stay available for controlled comparisons.

Selection after packing is useful but limited. In the full micro-router Telemetry run, all six accepted changes are translations of the existing internal variant. ESPower does accept a different crystal arrangement. Other scoring modes accept different USB/power arrangements, illustrating that candidate generation, local objective and board context interact. A per-block score winner cannot be assumed to be the best member of the complete board.

Repacking selects a different board hypothesis on ESPower with the micro router, reducing pair sum to 92.32 mm. Its measured runtime is 24.16 s versus 17.89 s for the default full profile. On Telemetry both repacking modes select the original hypothesis and cost 139–146 s versus 65–71 s without repacking. ESP32C3's geometric repack improves the weighted objective but not pair sum, and worsens HPWL. Repacking therefore remains explicit mode `2`, not the default.

The [independent routing probe](routing-probe.md) does not establish an overall routing-quality improvement. All experimental profiles have higher weighted penalties than legacy on these sampled job plans. Some jobs reach the search budget, so neither this penalty nor a shorter ratsnest is proof of routing success or failure. The ESPower length improvement must be interpreted with that limitation.

All 24 full-board runs report valid geometry and zero changed fixed poses, using the same native binary hash. Final checks passed: TypeScript typecheck/build, 147 PCB tests, and 44 Rust tests (one pre-existing ignored test). Native defaults for raw legacy captures remain unchanged. The repository's ordinary path intentionally enables the previous improvements and local portfolio for further research; the separately installed backend is untouched.

The next useful experiment is a diverse beam of partial board assignments: choose a block shape when its neighbours become known, rather than synchronously replacing all blocks with one alternative index. Finalists should be compared using a common routing probe as well as length, while treating budget cutoffs as inconclusive. This follows from the measured mismatch between local USB improvement, board wirelength and route penalties; it is not implemented in this change.
