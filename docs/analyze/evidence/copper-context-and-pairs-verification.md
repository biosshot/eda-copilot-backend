# Shared copper context and cached individual near pairs

Task06669f3 is implemented in two logical parts. The valid physical operator
coupon gains **1.915× on mean complete kernel/action wall** at unchanged gates.
Production copper preparation is corrected; the internal classifier uses that
same G1 geometry for G2. Full P7/real-board magnetic extraction, vias/ports and
P8/P11 remain unadmitted. CPU F64 analysis and placement F32/GPU are unchanged.

Packet: `debugging/p7-design-checkpoint/copper-context/`. Tracked tools and
review summaries: `tests/fixtures/analysis-local-extraction/copper-context/`.
Separate `geometry.diff` and `classification.diff` preserve G0→G1 and G1→G2.
No whole-main merge, live export, full ROI solve, new agent or commit occurred.

## Geometry: implementation and correctness

`geometry/context.rs` uses the installed i_overlay4.0.7 explicit-adapter API.
The only Cargo/lock change makes that existing transitive crate a direct
dependency. One deterministic dyadic origin/scale covers the full Board's
primitives, outline, pad shapes, drills and fills, with extent headroom below
the integer range. It is derived from extent, integer precision and existing
curve options; there is no new user snap tolerance or exact-rational engine.
Nonfinite/unrepresentable extent produces a diagnostic.

The context is thread-local and RAII-restored, including early returns and
nested calls. Local pad/drill assembly transforms into this world adapter.
Primitive union, board clipping, drill union/subtraction, topology overlap and
final layer union all reuse it. Dyadic integer outputs travel through the
existing F64-mm geo boundaries; a later operation never chooses another
adapter. Original pre-overlay single-primitive boundaries remain available to
the existing contact predicates, so a rounded positive gap cannot become a
reliable electrical path. No ABI/report schema or transport input changed.

`min_output_area=0`, `clean_result=false`; no area/aspect copper filter is used.
Removing exact integer collinear vertices does not change covered area. The
working conversion allowance16sqrt(2)×gridStep is combined conservatively with
the original Boolean allowance; curve approximation remains separate. STM's
grid step is0.238419nm, TPS/Band0.119209nm, with conversion allowances5.394797nm
and2.697398nm. These are working representation bounds, not a newly certified
topology theorem. Only Thunder's reported Boolean bound increases,
5.364406→5.394797nm (+0.57%), to account for the common context. User curve
options and electrical gates are unchanged. Unresolvable contacts retain
existing uncertain/unsupported diagnostics.

The experimental extractor now consumes the **native layer union and native
ROI clip with that same Board adapter** instead of independently re-unioning
source polygons in GEOS. Existing angular/25um plating hypotheses remain.
Experimental GEOS precision arrangement/CDT uses precisely the native dyadic
grid. It is a count-only mesh, not a production DC/Spade replacement or a
completed source/net/port-constrained ROI current space.

All8 frozen Board hashes, source/provenance objects and outlines are retained;
excluded sources remain0. Reliable island source-membership signatures and
short counts are identical on every board. Region/contour counts are unchanged.
Full snapshots and per-source IDs, symmetric differences and observed discrete
vertex-to-boundary distances are in `audit.json`. The indexed discrete metric
is not represented as a certified continuous Hausdorff enclosure.

Nine contact labels change from uncertain to point, all still unreliable:
NanoScope tracks756/758 TOP,933/934 BOTTOM; STM97/99,46/49,280/284,151/153,
31/34,53/55 TOP; Thunder480/483 TOP. `contact-differences.json` preserves IDs
and old/new records. No reliable connectivity is lost or invented; sources,
layer/net identities, pads/pours, voids, drilled spans and metadata are retained.

### STM and the real bridge

Previously established provenance is reused. Both STM613 sources now share
exactly one cap vertex. The old613 centroid lies0.024723nm outside corrected
Cu, within the representation bound; its nearest corrected support534 has
aspect19.997. Old668/556 positions map into healthy support620, aspect1.791.
Finite35um thickness and positive kernel contributions are retained. The real
0.2um bridge has overlap near2e-8mm². The rejected circle-only hull is not used.

12/8 and24/16 XY/z self controls are checked independently with the frozen
positive4D Sobol/exact-double-z method, powers16/18/20 and4 fixed seeds.
The working uncertainty is2×last change+6SE, empirical rather than certified:
support534 screen0.025982%; support620 screen0.056389%, both below0.2%.
Old STM613 source-rule failures at both orders and668/556 cancellation evidence
remain saved. Healthy new values are not treated as unchanged old tiny-cell
contributions, and no failed integral is replaced by zero.

## Classification: implementation and correctness

G1/G2 use identical fixed supports. `Plan` retains centers/radii/moments and a
packed bit per unordered pair. Inside coarse near blocks, self stays near;
other pairs must pass the existing inflated individual-ball theta0.4 test and
finite moment/remainder admission. Failed checks stay exact-near. Coarse far
blocks are unchanged. The mask is preflighted before allocation and reused
without repeated separation predicates/moment preparation during actions.
Far evaluation remains pair-streamed O(N²); no FMM, GPU or new basis is added.

Coverage tests establish uniqueness/completeness, all self pairs near,
singleton-leaf agreement and stable repeated visits. Full physical-space
aggregate bounds, raw reciprocity, PSD, zero/weak currents and explicit refusals
are retained. Arbitrary physical coefficients, zero and1e-12-scale vectors
pass absolute energy/potential checks against the saved all-near kernel.
No weak-current contribution or denominator is clipped.

The unchanged24-prism/two-island,4-mode closed-current coupon reuses the
accepted independent full matrix. Its empirical reference uncertainty0.400963%
remains separate; no full-matrix reference solve is repeated. The timer starts
before exact-cache/source preparation and includes classification, exact-near
integrals, aggregate-bound work, full matrix and5 vector actions. Frozen input
parsing/geometry is outside this common timer. G1 is genuinely all-near:
300 unordered pairs/576 ordered evaluations; G2 has159 near/141 far/294 evaluations.

Mean gain1.914761×; minimum G1 wall divided by maximum G2 wall still1.679×.
The full working error screen is0.447469% for both against5%; G2 far budget
0.222275% against1%; raw reciprocity5.55248e-11 against1e-9; spectra are positive.
B±sym(delta) PSD and absolute energy/potential bounds pass every stored action.
Zero produces exact zero energy and bound. No averaging, PSD repair, electrical
threshold relaxation or reference uncertainty removal is used.

Exactly2 new numerical batches: G1 21.960s/220.91MiB; G2 7.689s/22.57MiB,
each below120s/512MiB. G1 also contains the independent post-fix near reference,
so batch-wall ratios are not used to claim speed or memory gain. Prior accepted
weak/near-null controls remain unchanged alongside new small-vector checks.

## Validation and remaining limits

22 focused geometry checks (20 existing +2 new),20 conduction,12 DC and6 near/far
checks PASS across the saved commands. Mixed-width shared nodes/polyline and
different-net true-gap checks include rotation/translation of the whole Board.
Existing holes/cutouts, pad/pour, neck, resource and invalid-input thresholds
are unchanged. The initial combined debug corpus target was interrupted and
is not claimed as a whole-suite PASS; all8 optimized native G0/G1 corpus reports
separately pass the recorded source/shape/connectivity/exclusion comparisons.

An initial legacy debug ESPower timing hit120s; it is retained as a resource
refusal, never a cheap successful baseline. Optimized actual-source wrappers
completed all16 board/configuration processes. Failed initial raw-rounding and
curve/Boolean-budget checks, and postprocessing failures, are retained. No
green kernel campaign was repeated. All raw samples and variability are saved.

ONE normal default-GPU shared-addon jobs1 build PASS7m16s. DLL/.node hashes match;
only20 existing placement warnings and existing linker information remain.
Package ABI/load and a real five-source geometry call PASS. No C/P6/Thermal API
changed. Heavy was coordinated and released.

Four PNGs were viewed: ROI overlays, local STM/bridge, cost/memory, matrix/error.
Raw matrices/actions/bounds, references, preflights, resources, input/source
hashes and compiler/package evidence are retained. Minimal five-source/bridge
JSON tests depend on the preserved sibling `copper-geometry` fixtures.

The1.915× result is for this valid declared operator coupon. NanoScope geometry
preparation regresses13.3% on the cold/warm mean (warm4.5%); NanoSpec also regresses.
This measured cost buys consistent representation and removes known numerical
blockers; it is an explicit tradeoff, not a geometry speedup. Peaks stay comparable.
The vertex/input payload memory model is not a certified full-library heap bound;
each process also has the hard512MiB monitor. Input/import and full process walls
are saved separately from preparation timing.

Counts before/after geometry are separate from G1/G2 classification. G0 frozen
experimental source-union meshes are not mislabeled old native layer-union
meshes. G1 mesh areas equal their fixed-grid Cu domains in the saved calculation.
They remain pre-constraint candidate spaces; count plans do not admit a ROI
energy bound, current-space rank, source/port assignment or solve. Unmeasured
fallback/shape strata, real via interfaces and full ROI costs remain blockers.
Earlier1.9–2.7 work projections are not guaranteed P7 runtime gains. STOP for
parent review; no full P7/P8/P11 or new numerical stage is declared complete.

## Recorded measurements

|Board|G0 cold/warm s|G1 cold/warm s|Peak G0/G1 MiB|Regions/islands|Max source displacement nm|
|---|---:|---:|---:|---:|---:|
|BandAmpV5|0.803/0.889|0.796/0.729|18.5/16.8|319/6|0.136|
|ESPower|3.628/3.965|3.543/3.541|29.0/29.1|668/56|1.217|
|NanoScope|37.174/39.874|45.583/41.684|136.8/135.8|3019/175|1.764|
|NanoSpec|21.510/25.673|24.358/27.569|84.2/84.2|2509/160|2.130|
|STM32G0_CAN_RS485|1.430/1.366|1.533/1.331|27.4/27.4|844/65|1.607|
|ThunderF722|18.008/16.862|15.655/18.032|90.0/89.1|1895/116|0.585|
|TPS63060DSCR|0.198/0.203|0.222/0.210|12.0/12.2|172/8|0.162|
|USB_PD_Trigger_5A|1.096/1.115|1.163/1.028|19.4/19.7|388/20|0.189|

|ROI|Frozen G0 experimental foil -> G1 foil|Axial|Current coefficients|Potential nodes|Worst aspect|Minimum edge um|
|---|---:|---:|---:|---:|---:|---:|
|STM32G0_CAN_RS485|1002 -> 903|192|1902|726|1.27e+03|0.76387|
|TPS63060DSCR|2178 -> 1911|576|4110|1319|1.62e+03|1.15001|
|BandAmpV5|2962 -> 2600|768|5584|1804|1.37e+05|0.000238419|

|ROI, same G1 geometry|Near G1 -> G2|Far G1 -> G2|Plan G1/G2 s|Planned G1/G2 MiB|
|---|---:|---:|---:|---:|
|STM32G0_CAN_RS485|236969 -> 97754|189715 -> 328930|0.00283/0.04579|14.25/14.30|
|TPS63060DSCR|1023013 -> 511123|970079 -> 1481969|0.00750/0.17442|29.73/29.97|
|BandAmpV5|1583110 -> 843071|2093486 -> 2833525|0.01455/0.26398|37.57/38.00|

|Coupon|All operator samples s|Mean s|Exact ordered evaluations|Full screen|Far budget|
|---|---:|---:|---:|---:|---:|
|G1|4.512671 / 5.011970 / 4.597173|4.707271|576|0.447469%|0.000000%|
|G2|2.190169 / 2.497574 / 2.687493|2.458412|294|0.447469%|0.222275%|

Sample wall standard deviations (n=3, not a confidence interval): G1 0.267238s; G2 0.250964s.


## Parent review and integration (2026-10-24)

Reviewed the shared adapter/frame lifecycle, preserved original contact predicates, pair-mask coverage and admission/resource checks. Independently verified all 208 saved artifact hashes (zero mismatches) and inspected the STM support/bridge visualization. Integrated only the current delta into feat/analyze, retaining the accepted P6/spectral kernels and both dependency locks.

Main-branch focused checks: conduction 20, DC 12, geometry 22 (corpus test excluded), magnetic near/far 6, validation 13 passed. The initial integrated debug corpus run was stopped for runtime; it is not a full-suite PASS. The eight-board optimized corpus evidence belongs to the agent run with matching source hashes. Coupon acceleration does not establish full-board extraction performance; full P7 remains open.

Integrated normal default-GPU Node addon release build passed (18m09s); package ABI/load and five-source geometry smoke passed on the rebuilt main-branch binary. Logs: debugging/p7-integrated-build.log, p7-integrated-tests.log, p7-integrated-focused.log, p7-design-checkpoint/copper-context/package-smoke.json. No worktree addon was substituted.
