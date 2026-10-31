# P7-A baseline evidence, 2026-10-31

P7-A is incomplete. P7 remains open. No production/native code, previous fixture, dependency or common roadmap was changed. The implementation and the missing evidence are separated below.

## Reproducible inputs

`tests/fixtures/analysis-p7-closure/manifest.json` binds exact board and direct JavaScript DSL bytes with SHA256, physical contacts, source-net track inventory, assumptions, load convention, frequency axis and predeclared floors. Generate using `python scripts/verify-p7-closure-fixtures.py`; this only writes the new closure fixture directory.

Selected complete copper channels:

| Board | Signal source → sink | Physical return terminals | Scope |
|---|---|---|---|
| STM32G0_CAN_RS485, 2 layer | U5.7 → J5.2 | U5.5 → J5.3 | Remote RS485 channel; D5/JP2 branches retained in original board |
| NanoScope, 4 layer | U3.52 → U4.6 | U3.57 → U4.4 | Full QSPI_SCLK route, including U2.6 branch |
| ThunderF722, 6 layer | U2.34 → U4.6 | U2.31 → U4.4 | Full SPI2_SCK controller-to-flash route |
| TPS63060DSCR | J1.1 → U1.3 | J1.2 → U1.4 | Full VIN distribution copper; converter behavior excluded |
| BandAmpV5 | RF1.2 → C1.1 | RF1.1 → U1.2 | Full RF_IN_PORT copper up to series component; C1/U1 models unavailable |

These are concrete source-to-sink selections rather than isolated track fragments. Captured pad nets/coordinates/layers and every selected-net track are stored. Signal routes were visually inspected in `debugging/p7-closure/real-selected-geometry.png`. This does not certify connectivity of the finite return polygons or numerical admission. All copper remains in the original board; names do not weld separate islands. Real-channel EM runs were not repeated against known unsupported geometry. Complete independent response references are still missing.

The uniform 1.6mm stack (35µm copper, equal dielectric gaps), er4.2, lossless substrate,58MS/m copper,25µm finished-hole plating are explicit benchmark assumptions, not recovered board manufacturing data. Actual laminate, roughness and loss tangent remain unavailable. No component parameter was inferred from pad shape. The BandAmp selection stops at C1 because extending through amplifier/coupling components needs explicit models; this is not a complete amplifier RF certificate.

## Reference and numerical runner

`node --import tsx scripts/verify-p7-closure-baseline.ts` executes genuine coupled cross-section FEM and compares extracted C/L, modal Z/delay and every complex four-port S entry at1/10/100MHz for20mm propagation. Independent engineering values come from the existing hash-bound `analysis-cross-section/references.json`, Kirschning/Jansen equations documented by [Qucs](https://qucs.github.io/tech/node77.html). They are independent equation evaluations, not measured data or an external solver. u=g=1 and er4 satisfy its published static range. Thin-reference versus finite-copper mismatch is explicitly unbounded systematic uncertainty, so any passing comparison must retain this qualification.

The comparator independently constructs modal lossless ABCD response from reference Z and epsilon, converts to50Ohm S, and transforms even/odd waves to physical port order. It never calls the producer's propagation function. Floors were assigned before execution:1fF/m C,1pH/m L,0.01Ohm Z,0.1ps/m delay,1e-4 complex S (-80dB). Relative acceptance8%; no15% fallback. Losses are unsupported in this PEC/lossless control. Numerical residual and convergence remain native diagnostics, separate from reference error.

`python scripts/verify-p7-closure-supervised.py` reuses the existing supervisor's process-tree and Windows headroom policy, isolated cold cache,145s outer deadline,1.5GiB tree-private cap and1GiB host commit reserve. Native budget120s/1GiB.20ms sampled peaks are lower bounds. The addon was held unchanged by agreement with the geometry agent throughout each run window.

| Packet under debugging/p7-closure | Changed input | Outcome | Wall | Tree RSS/private peak |
|---|---|---|---:|---:|
|1793430496396414600|1e-5mm thin copper, original mesh|Supervisor killed on private cap; no solve result|8.859s|1475948544 /1617084416B|
|1793430522458769600|0.035mm finite copper, original mesh|Supervisor killed on same cap; no solve result|15.704s|1509765120 /1674022912B|

These failures are not successful cold solves. The cap was not raised. A subsequent reduced mesh/domain candidate is separate and must preserve native convergence/domain checks. No identical failure is rerun.

Saved straight and bend complete low-band composed/whole-volume comparison packets were copied byte-for-byte into the fixture directory with hashes and origin paths in `reused-references.json`. They are explicitly reused numerical cross-references, not new runs, regression goldens or independent measured truth. Earlier recorded differences remain straight delay4.994104%, complex S0.000574507%, loss0.000711407%; bend delay0.514169%, complex S0.769961%, loss3.995737%. Their original axes, matrices, drives and diagnostics are retained. They do not transfer accuracy to the real channels.

## Verification and remaining acceptance

`node --import tsx --test tests/analyze-p7-closure-reference.test.ts`:3 tests passed. Negatives reject stale hashes, wrong ports/axis, malformed matrices, unavailable solver status, nonfinite values, unit-scale errors and threshold excess. `python scripts/verify-p7-closure-plots.py` generated the inspected selected-contact/track figure. Plane polygons are not drawn in this first figure, and its caption explicitly says so.

Still missing: accepted genuine coupled AC result under supervision and independent error metrics; complete physical split-reference/multiple-reference/AGND-DGND/component-bridge/regular-and-broken-fence coupons; real return connectivity audits, branch geometry and plane plot overlays; executable per-case dispatch/status coverage for unsupported and missing-reference; response/error/resource plots; rigorous independent reference uncertainty. Those are unfinished work, not blockers supposedly proved by two resource failures. There is no basis to close A, P7, P8 or P11.

## Positive genuine coupled AC control with limited independent comparison

Existing coupled-stripline template completed a NEW native run in0.9256076s: width/gap/both dielectric heights0.3mm, copper0.035mm, length20mm, homogeneous er4 PEC/lossless, frequencies1/10/100MHz. Full numerical output: `debugging/p7-closure/stripline-native.json`. Finite mutual C01=-1.333069231829321e-11F/m and L01=2.771171176171538e-8H/m retained; actual FEM residual6.283674483526126e-16, mesh change0.455432%, domain change4.36e-15. Full four-port propagation matrices present. Addon SHA256 c348e48a125249939327f5f1c26dd28f7d30e27edb5d24ec4f9cd210dc6effa2.

`node --import tsx scripts/verify-p7-closure-stripline.ts --reuse` independently compares even/odd delay against sqrt(er)/c and reconstructs ABCD A from numerical S versus cos(omega*length*sqrt(er)/c) at all3 frequencies; max relative difference2.2071e-14. This is genuine coupled EM and an independent analytic physical identity, but velocity is structurally imposed by homogeneous C/C0 extraction: it does NOT independently validate absolute C/L/Z or crosstalk magnitude. Original native run was unsupervised: no accepted process-tree peak. Comparison-only replay is reused. Removing `--reuse` runs the saved explicit physical control anew. `P7_CLOSURE_CONTROL=stripline` selects it in the supervisor for a future justified resource measurement. A remains incomplete.

Additional changed microstrip attempts: domainScale4/maxEdge0.3mm/80k-node candidate at3 refinements (`1793430707809481500`) rejected SECTION_CONVERGENCE in1.110s,207736832/131489792B RSS/private. Changed5-refinement candidate (`1793430728870953100`) rejected SECTION_MESH_LIMIT111397 nodes in2.188s,234168320/162574336B. No identical failure rerun; cap unchanged.

## B addon rebuild: exact supervised AC replay

After geometry agent's normal native build, the exact previously positive coupled-stripline request was replayed once for binary regression and previously missing OS resource capture. Stable new addon SHA256 `f0cd8d66a2a190ee32918cc8b73c7669b35a5e9b568640713d9afc166258cc85`; previous `c348e48a125249939327f5f1c26dd28f7d30e27edb5d24ec4f9cd210dc6effa2`. B granted the run window and no binary replacement overlapped.

Commands (PowerShell): `$env:P7_CLOSURE_CONTROL='stripline'; python scripts/verify-p7-closure-supervised.py`, then `python scripts/verify-p7-closure-regression.py debugging/p7-closure/1793431659930249800`. Packet retains full numerical replay, analytic comparison, regression JSON, logs and supervision. All complete C/L/R/G/C0 matrices and modal quantities (30 numerical values), all four-port complex S matrices and network metadata (98 numerical values), and3 frequencies are JSON-exact equal to original saved output; maximum absolute and relative differences zero. Analytic TEM/ABCD comparisons still pass. This is binary regression, not an additional independent accuracy reference.

Successful cold process end-to-end1.578s, tree RSS250490880B/private290746368B sampled every20ms (lower-bound peaks). Initial Windows commit headroom21658968064B; cap1610612736B, reserve1073741824B. Native request unchanged30s/512MiB, cold isolated directory. Thus a genuine coupled AC numerical control now has successful supervised resource evidence. The remaining absolute C/L/Z/crosstalk independent accuracy and broader A obligations remain open. Three focused tests pass after this change.
