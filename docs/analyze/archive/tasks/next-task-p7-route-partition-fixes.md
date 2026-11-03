# P7: fix R1–R3 and complete actual STM32 DSL acceptance

Status: implementation authorized. Continue the existing uncommitted candidate; do not restart or discard native oriented-boundary work. This task overrides the incomplete handoff in p7-route-partition-results.md. Geometry-only success is not completion. Read p7-route-partition-plan.md, both agent reports and p7-memory-architecture-audit.md.

## R1 — exact multi-source cut contacts (native owner)

Real planning fails at face-0 because a connected physical face interval is formed by several source polygons while the electrode can certify only one sourcePath. Implement explicit internal multi-source electrode support with exact union restricted to layer, oriented finite face and physical domain. Preserve single-source requests as a valid special case; agree schema with TS owner before dependent edits.

Never choose sourcePaths[0], join by net name, fill gaps, count overlaps twice or combine physically disconnected intervals into an invented equipotential electrode. Split disconnected contacts and preserve their distinct identities. Validate source existence/layer/contact membership, reject empty/ambiguous unsupported topology, deduplicate canonical source identity in cache signatures. Map the actual union area into native contact nodes and current-space boundary conditions, not merely DTO fields.

Tests: overlapping same-conductor sources form one exact union with no double count; reversed source ordering gives identical identity/results; disjoint same-net pieces stay distinct; different-layer sources reject; finite span/rotated cut/holes preserve area; legacy rectangle/oriented single-source controls unchanged. Include a numerical continuity/conservation control for the new union, not only import tests. Normal native build required; no new physics solver.

## R2 — actual uniform run corridors, not eight all-local BSP cells (TS owner)

The candidate partitions a rectangle with route-oriented infinite cuts, creating unrelated plane faces and all-local cells. Replace this classification/partition path with explicit coupled uniform run corridors and compact event domains. Route graph supplies longitudinal direction and endpoints; actual source copper plus return/environment supply cross-sections. Preserve D5/JP2 branches, bottom-layer route and physical return on STM32. No shortest-path trimming, removing neighboring copper to force a section, or automatically converting arbitrary polygons to uniform lines.

Do not accept arbitrary width-based end collars without boundary sensitivity validation. Cuts must lie in verified uniform regions. Nearby events/coupled vias may merge; a corridor is admitted only when its signal, finite return and environment are consistent. Treat return-only shared faces and plane-mediated coupling physically; multi-source support must not merely allow an incorrect all-local partition to proceed. Existing joint multiport P6 assembly and off-diagonal section coefficients must survive.

Remove the rejected BSP automatic fallback after replacement works. Geometry helpers with real callers may be retained; no legacy selector or duplicate default planner. Do not conflate section count with accuracy: real plan must contain meaningful uniform runs AND pass numerical checks. If a specific region genuinely cannot be separated, quantify why and keep the local region rather than omit interactions.

## R3 — material/outline invariance (TS owner with native support as needed)

Uniformity must include board exterior, cutouts, dielectric material boundaries, copper and bores through the whole run, not just copper or three convenient sample slices. Use exact boundary-event checks or equivalent demonstrable invariance and native physical slices. A diagonal/curved board edge changing dielectric support makes an event or local block. Existing material input limitations remain explicit.

Add adversarial regressions: unchanged straight copper over a varying board edge; internal dielectric cutout between sampled slices; continuous uniform board rotated0/45/90; source polygon split without changing physical copper. The first cases must not silently become a single uniform section. Validate physical source-fragment coverage and no overlap; do not use self-consistent fake area intervals as the only proof.

## Mandatory integration and evidence (TS owner after native handoff)

1. Reproduce current failure, then pass actual STM32 planning and new R1/R3 regressions. Visualize source copper, chosen corridors, locals, cuts, finite returns and coupling groups with reasons. No full costly runs before geometry/contact prerequisites pass.
2. Run completed small full DSL cases: rotated uniform channel, bend, coupled aggressor/victim, via/return and broken-reference case where supported. Compare to saved independent full/local physical references. Check boundary displacement/domain extension, current conservation, passivity/reciprocity and mutual responses; preserve strict solver guards. Physical final target <=8% with appropriate absolute floors near zero and reference uncertainty. Do not accept the prior compact8.11% baseline as passing or rewrite reference values.
3. Run actual STM32 U5.7→J5.2 DSL (same physical board/materials/endpoints/branches) through final reduced model and P6 response, not a small substitute or a geometry report. Capture cold/warm/fresh-process disk cache evidence, final matrices and plotted response/partition. Record per-block preparation/factor/reduction times, unknowns/fill, tree RSS/private commit and memory release across blocks. Sequential heavy blocks; reuse one factor for all RHS within each block.
4. Use existing supervisor: goal<=4GiB and<=120s cold,<=5s warm,<=10s restart. One accepted candidate real attempt at diagnostic<=8GiB/600s per cold call, host commit reserve checked, paging opt-in only. If an accepted comparison reference requires a larger bounded budget, document the need and obtain parent direction; no unlimited runs or unchanged heavy retries. A changed failed candidate may be rerun only with the defect/fix and prior evidence recorded.
5. Real accuracy needs a legitimate reference for THIS topology. Reuse an existing independent real reference only if geometry/ports/band match; none is assumed to exist. If none exists, obtain a bounded independent reference where feasible and distinguish reference uncertainty, numerical refinement and composition error. Inability to complete the real reference must remain a specific acceptance gap; coupon error alone cannot certify STM32. Non-completion/resource failure is not a low-memory success.
6. Compare resource results honestly: previous real F64 was a preallocation refusal, not a successful8.715GB run. Report new measured peak versus prior estimates as different metrics; do not invent a measured speedup. Reduced memory must accompany completed physical output. Keep HF/all-DSL scope limitations explicit; no P7 closure based solely on geometry or low-band success.

## Ownership and handoff

- /root/oriented_boundaries: R1 Rust + native tests/transport, own normal native build slot first. Coordinate new schema with /root/route_partition. Do not modify TS planner/CHANGELOG. Publish p7-oriented-boundaries-results.md updates, hand off files/schema/binary and build ownership explicitly.
- /root/route_partition: R2/R3 TS, ownership/coupling/cache integration, all final DSL acceptance after native handoff, CHANGELOG and p7-route-partition-results.md. Coordinate native prerequisites rather than stopping at missing schema. No heavy build/run until native owner yields.
- No further agents, review-only agents, commits/push, git reset/clean, fixture deletion or overwriting user native/analyze/debugging. Parent task docs and audit are preserved. Keep progress concise, avoid repeated unchanged status checks.

## Completion contract

Deliver implemented and tested corrections, final files frozen and clear pass/fail ledger. Do not finish merely after nine geometry tests or an updated diagram. Complete all feasible independent work, small physical checks and bounded actual STM32 attempts. If physically/numerically blocked, document exact reproducible obstruction and what was completed; never weaken constraints or claim full completion. Parent reviews before commit. No newly invented solver/method sweep in this task.
