"""Write review report entirely from saved evidence."""
from trace import ROOT,FIX,OUT,read
a=read(OUT/'audit.json');c=read(OUT/'census.json');inv=read(OUT/'inventory.json');cat=read(OUT/'category-summary.json')
short=lambda n:n.replace('STM32G0_CAN_RS485','STM').replace('TPS63060DSCR','TPS').replace('BandAmpV5','Band')
it='\n'.join(f"| {short(r['name'])} | {r['sourceObjectCount']} / {r['preparedRegionCount']} | {r['preparedContourCount']} / {r['preparedVertexCount']} | {r['unionContourCount']} / {r['unionVertexCount']} |" for r in inv)
dt='\n'.join(f"| {short(r['fixture'])} | {r['rawConductorCdtTriangles']} -> {r['foilTriangles']} | {r['supports']} / {r['axialSectorDofs']} | {r['candidateCurrentDofs']} / {r['candidatePotentialNodes']} |" for r in a['roiRows'])
st='\n'.join(f"| {short(r['name'])} | {f['family']} | {f['statistics']['areaMm2']['0']:.5g} | {f['statistics']['volumeM3']['0']:.5g} | {f['statistics']['minimumAltitudeMm']['0']:.5g} | {f['statistics']['xyAspect']['1']:.5g} | {f['statistics']['belowBooleanWidthContract']} | {f['statistics']['sharedExactEdges']} |" for r in c['rows'] for f in r['families'])
def positive(r):
 v=r['samplePositiveQueryFractionRange']
 return 'unknown' if v[0] is None else '{:.1f}..{:.1f}'.format(100*v[0],100*v[1])
pt='\n'.join(f"| {short(r['fixture'])} | {r['category']} | {r['pairs']:,} | {r['pricedPairCount']:,} / {r['unpricedPairCount']:,} | {r['workingSuccessfulCostRangeSeconds'][0]:.1f}..{r['workingSuccessfulCostRangeSeconds'][1]:.1f} | {positive(r)} | {'REFUSAL' if r['refusal'] else 'sparse'} |" for r in cat)
rt='\n'.join(f"| {short(r['fixture'])} | {r['nearPairs']:,} | {r['leafOnlyPairs']:,} ({100*r['leafOnlyFraction']:.2f}%) | {r['extraBallOverestimatePairs']:,} | {r['remainingNearPairsAfterLeafFilter']:,} | {r['countOnlyMaxSpeedup']:.3f} |" for r in a['roiRows'])
wt='\n'.join(f"| {short(r['fixture'])} | {r['normalZeroFallbackWorkingSecondsBefore'][0]:.0f}..{r['normalZeroFallbackWorkingSecondsBefore'][1]:.0f} | {r['normalZeroFallbackWorkingSecondsAfter'][0]:.0f}..{r['normalZeroFallbackWorkingSecondsAfter'][1]:.0f} | {r['conditionalSecondsSaved'][0]:.0f}..{r['conditionalSecondsSaved'][1]:.0f} |" for r in a['roiRows'])
text="""# P7 copper partition and near-cost audit

The largest measured work-count problem is coarse leaf classification, not a
uniformly expensive pathological pair. Native theta0.4/leaf8 sends48.2..62.4%
of all near pairs to the near kernel even though their individual existing
moment criterion already passes. Source fragmentation and numerical validity
are separate problems: STM668/556 originate before CDT, and the one frozen
batch exposes a new hard source-quadrature refusal on STM613 self.
No full ROI operator, production mesher/kernel change or public admission.

**One next change recommended:** stream individual existing moment
separation/remainder checks inside coarse near blocks, retaining self/failed
pairs in exact-near and enforcing the full physical aggregate error budget.
This is a proposal for parent review; it is not implemented by this audit.
Geometry/current approximation changes are not recommended as the next patch.

## Immutable chain and source witnesses

Task copied alone from MAIN cac5f8b. Inputs are the original Board DTO,
source-pcb.txt/source.epro2/capture metadata, accepted native geometry
snapshots, saved layer contours/supports and native near/far plans.
No fresh export or board preparation was performed. Input/source/raw hashes
are retained in the packet; no original file was overwritten.

The actual chain for these supports is:

1. CAD LINE IDs -> normalized Board track/pad/via/zone objects.
2. Native CPU/F64 prepared polygons per source. Simple strokes union endpoint
   circles and segment rectangles using geo/i_overlay's local integer adapter.
3. Analysis preflight clips prepared source polygons to the frozen ROI and
   unions them per layer using GEOS. This is a geometry/counts candidate,
   not the production Spade field/current mesh.
4. Per-layer arrangement adds field/board/drill/barrel boundaries, polygonizes
   the arrangement and uses constrained Delaunay triangulation.
5. Each retained foil triangle becomes one finite35um prism. Candidate foil
   currents have two piecewise-constant XY coefficients per triangle; each
   axial angular sector has one Z coefficient carried by two prisms.
   Potential-node counts are geometric counts. No full constrained physical
   ROI current space, rank, ports or current solution has been assembled.

The snapshots have no per-support source/net field. This audit preserves row
indices and recovers source witnesses from original polygons; representative-
point owners do not prove that an entire triangle belongs to only one source.
No semantic same-net/material/interface merge can be authorized from this
census alone.

### STM668/556: exact stage, not a guess from aspect ratio

CAD IDs54abddda771675b4, bef1aae594813e2c and5ff3f2845c577b5b are TOP +3V3
tracks. Raw CAD and normalized Board agree on a shared endpoint
(-3.7041,12.425)mm and width0.5mm. The ideal generated cardinal tangent is
(-3.4541,12.425)mm for all three; the raw endpoint equality is exact.

The prepared horizontal stroke's rightmost X is
-3.454099999639392mm, displaced0.360608pm. A vertical stroke's corresponding
Y is12.425000000278652mm, displaced0.278652pm. These differences already
exist in the native **per-source stroke Boolean result**. None of these
three sources has a drill/board clipping operation at the witness.

A tiny actual-source replay, using unchanged model/areas/primitives helpers,
reproduces all three complete prepared polygons with exactly equal F64
coordinates and zero symmetric-difference area. Helpers from the accepted
preparation commit f648956c are canonically identical to the compiled helpers;
their source/binary/library hashes are recorded.

GEOS layer union intersects the horizontal cap facets with X=-3.4541mm.
Independent80-digit arithmetic on the exact saved F64 segment endpoints
recovers both saved union vertices at Y12.424999995878233 and
12.425000004121769mm with **0 ULP residual**. Their separation is8.243536pm.
CDT retains these existing boundary vertices, yielding the long thin668 and
tiny556 triangles. CDT/prism extrusion does not originate their tiny extents.

This is an artifact of independently rounded representations of the same
authored junction, subsequently noded by layer union. It is not evidence of
an authored physical pm neck. It is also not permission to delete it: the
accepted saved-domain kernel keeps it, and source-coherent geometry repair
would require its own domain/error/connectivity contract.

### Precision and the new STM613 blocker

F64 coordinate ULP, native Boolean conversion bound, curve approximation and
CAD/export resolution are different quantities. At Y12.425mm the F64 ULP is
about1.78e-15mm; the668/556 separation is millions of those ULPs. The saved
board-level Boolean displacement bound is7.15e-6mm, while curve approximation
is about1e-3mm. These are error bounds, not snap thresholds.

The extra worst-aspect witness613 has a shortest edge2.84356e-15mm, comparable
to coordinate ULPs. All its vertices already occur on the saved TOP layer
union boundary. Owners88d9932c804b2bb2 / ddd0394190d6ae65 share an exact
normalized endpoint(-3.116,9.13)mm, width0.243mm. The complete per-operation
construction history of this extra witness was not reconstructed; no general
small-area/ULP filter follows from its dimensions.

The sole new batch refuses613 self at both XY12/z8 andXY24/z16:
positive source quadrature fails its unchanged1e-5 order-change criterion
through63. Both uninstrumented and telemetry copies return the same refusal.
No retry, higher cap, softened geometry, unchecked fallback or kernel repair
was attempted. The short failed call is never priced as a cheap valid integral.

Native-reader/SI and Python-reader/SI coordinates differ by at most3.47e-18m
after vertex relabeling. Cell613 changes orientation in original labeling and
is relabeled by the existing positive-prism constructor; microscopic areas
can change by factors near5. The transport audit records this sensitivity.
It neither changes the parser nor establishes a new geometric error contract.

## Inventory, fragmentation and shape distributions

Original objects count distinct source paths intersecting the ROI; complete
intersecting prepared contours are counted before clipping. Clipped union
contours/vertices are separate. Full-board object inventories and all source
IDs are in inventory.json; no parametric circle is misreported as an authored
polygon with its later tessellated vertex count.

| ROI | source objects / prepared regions | prepared contours / vertices | clipped union contours / vertices |
|---|---:|---:|---:|
""" +it+"""

| ROI | raw layer CDT -> constrained foil triangles | total prism supports / axial sector DOFs | candidate current coefficients / potential nodes |
|---|---:|---:|---:|
""" +dt+"""

Raw layer CDT is a counterfactual without the common barrel/interface
constraints; it preserves layer-domain polygons but changes triangulation,
candidate current approximation and interface conformity. Its smaller count
is **not** a proven safe coarsening. Exact domain equality does not prove
current-space equality or flux/port approximation accuracy.

Quantiles0/1/10/50/90/99/100% for area, volume, minimum altitude, shortest
edge and XY aspect are saved per family. Worst coordinates/source witnesses
are retained in census.json, measurement-preflight.json and the two STM traces.

| ROI | family | min area mm2 | min volume m3 | min altitude mm | max XY aspect | altitudes below Boolean bound | exactly shared XY edges |
|---|---|---:|---:|---:|---:|---:|---:|
""" +st+"""

Every exact mesh edge has multiplicity1 or2; no multiplicity>2, overlapping
same-family volumes or indeterminate contact construction was found in this
census. This is geometric incidence evidence, not full net/connectivity/port
validation. Counts below a precision bound are diagnostics only; all supports
remain present. Axial long-Z aspect is separate from the displayed XY aspect.

## Near categories and measured cost strata

All counts below use the **actual unchanged native plan** at theta0.4/leaf8.
One Python hierarchy mirror differed on TPS by40pairs; that failed mirror
result is preserved. No tolerance was relaxed. A geometry-only native export
provides exact order/nodes/blocks/moments/SI supports and matches every frozen
native count. The export evaluates zero integrals.

3D categories: shared XY edge with positive Z overlap is a shared prism face;
shared XY vertex is a vertical prism edge. Isolated3D vertex contacts are zero
for these same-family Z intervals. Foil/axial cross terms are exactly vector-
orthogonal under the existing model, so the same two families are retained.
Close disjoint means exact prism distance<=max pair thickness; this definition
is fixed and independent of timing. Distance-bin details are also saved.

The deterministic sample was frozen **before** any new timing: min index-hash
witness per nonempty contact/false-near cohort, explicit axial controls,
worst-aspect self per family and known STM witnesses.28 unique pairs,
two fixed rules12/8 and24/16, one process<=120s/512MiB. Prior accepted STM
timings were reused; their new evaluations collect fallback telemetry only.
Other pairs compare actual uninstrumented integrals against a callback-only
telemetry copy: all successful outputs are bit-identical; both failed rows
return identical errors. Production source files are unchanged.

The following costs are **partial work estimates for sampled shape strata**,
not total ROI runtimes or confidence intervals. Pools retain family, XY shape
bin and self-vs-mutual; borrowed categories are explicitly labeled in
category-costs.csv/audit.json. Unsampled bins are left unpriced, rather than
spreading one pathological timing across all pairs. In particular the middle
100<aspect<=1e4 bin has no new representative timing. Positive percentages
are ranges among successful point queries in sampled pools, not estimates
of population fallback frequency. Failed query counters are retained raw.

All table costs use fixed XY24/z16. Both rules/raw signed values and detailed
per-family/category/shape counts are saved. Priced/unpriced counts include
both foil and axial primitives.

| ROI | category | near pairs | priced / unpriced pairs | partial successful work seconds | sample positive query % | validity |
|---|---|---:|---:|---:|---:|---|
""" +pt+"""

Normal sampled directed integrals cost3.95..4.58ms with0% positive queries.
The TPS worst-aspect self costs85.36ms with31.60% positive queries; known
STM668/556 has47.22%, while668 self has7.83%. These tails are not assigned
as a universal ROI mean. New613 refusal means no finite accepted-runtime
upper can be asserted for the current ROI kernel. Raw reciprocity/order
changes are diagnostics for new pairs; independent0.2% integral acceptance
is not inferred from two native rules.

## Leaf overclassification and one next change

An accepted far node ball covers every complete individual support ball.
If parent radius sum S<distance D and support-center offsets sum x, then

    q_individual <= (S-x)/(D-x) <= S/D.

Thus every parent-far pair already passes the individual criterion. A leaf1
plan has exactly the set q_individual<=theta. The difference counted below
is leaf8 near work that can be streamed through the **existing individual
moment admission/remainder**, without changing source domain or current
space. Self always stays near. The optional support-difference radius column
is only a geometric opportunity; changing that bound is not recommended now.

| ROI | current near pairs | individual-ball pass inside near | additional ball overestimate | near remaining after individual filter | unit-work ratio upper scenario |
|---|---:|---:|---:|---:|---:|
""" +rt+"""

The existing saved24-prism/four-mode closed-current control already uses
leaf1 at the same theta0.4. Reusing its native matrix, full independent
reference, bound and spectra gives full screen0.447469%<=5%, aggregate far
bound0.222275%<=1%, raw reciprocity5.55e-11<=1e-9 and positive physical
eigenvalues. Frozen geometry, holes, material boundaries, source currents
and closed-current conservation are unchanged. This is a saved-value control,
not a new continuum certificate or a transfer of acceptance to any ROI.
Reference uncertainty remains explicitly empirical/statistical.

For a conditional zero-positive-query work model, use the measured normal
per-directed cost3.95..4.58ms and the prior measured moment cost0.257us.
Near off-diagonals retain both directions; self is evaluated once.

| ROI | current normal-work seconds | filtered conditional seconds | conditional work saved seconds |
|---|---:|---:|---:|
""" +wt+"""

These are neither actual ROI timings nor total accepted-cost bounds. The
guaranteed saving is zero until the full ROI physical current-space aggregate
bound is admitted; that operator was not built here. The empirical unit-work
scenario gives1.93..2.67x, and even its optimistic residual exceeds500s.
Unpriced shapes/fallback tails,613 and full-matrix validity remain blockers.

## Comparison of the four options

| Option | Physical domain | Current approximation | Near/error/resource effect |
|---|---|---|---|
| Better constrained triangulation, same boundary | Must preserve every saved hole/feature/material/interface | P0 support space changes; nested refinement can retain old space but adds DOFs | Can improve aspect; cannot remove mandatory pm boundary vertices. Steiner quality improvement may increase cells and near work. No proven speedup. |
| Merge only proven redundant partitions / change basis | Exact polygon union can preserve geometry, but source/net/port/interface proof is missing | Removes independent current variations; needs independent full magnetic reference and convergence | Raw617/1030/1432 layer counts are only optimistic geometric counters. Not a safe reduction or approved current model. |
| Specialized close-pair formulas / adaptive quadrature | Can retain exact finite prisms | Can retain current space | Normal cost scales with outer query count; fallback tails and613 need conditioning/error controls. Two native orders are not a reference. No new formula/order policy implemented. |
| Individual existing moment checks inside coarse near leaves | Unchanged | Unchanged | Removes48.2..62.4% of near candidates before aggregate-error admission; conservative existing remainder reused. Far streaming remains quadratic. This is the ONE recommended next change. |

The recommendation is limited to a separately reviewed internal coupon
implementation with cumulative resource/aggregate energy guards. It must
preserve self and all numerical refusals, exact source identities, geometry,
nets/connectivity/holes/material interfaces and current space. It may not
delete613 or treat its error as zero. No full ROI solve, source precision fix,
basis coarsening or new public API is authorized by this report.

## Resources and review packet

After Thermal release: actual-source optimized compile19.71s; native
geometry-only plan telemetry0.583s; category census12.17s; ONE kernel/replay
batch0.839s, peak5.16MiB. No deadline/memory increase or numerical retry.
No production mesher/kernel, dependencies, GPU/placement/C/P6/Thermal API
or addon was changed/rebuilt. The task explicitly excludes a large addon
rebuild; only the isolated actual-source fixture was compiled.

Saved-evidence tests5/5 pass: partition/contact conservation, frozen one-batch
caps/telemetry parity, exact source replay/intersections, retained failures/
unpriced strata and independently recalculated full four-mode energy gate.
Python syntax and git diff checks are recorded. Geometry overlays,
pm witness view and category/cost plot were viewed.

Tracked tools/fixtures: tests/fixtures/analysis-local-extraction/copper-partition/.
Ignored detailed packet: debugging/p7-design-checkpoint/copper-partition/,
including inventory/input hashes, original source regions, native plan
membership, exact STM traces, coordinate transport, fixed preflight, full
raw JSONL/errors/counters/timings, stratified CSV, reused full matrices,
build/resources/tests, viewed PNGs and manifest.json.

STOP for parent review. Full P7 and real-board admission remain open.
"""
(ROOT/'docs/experimental/analyze/copper-partition-audit.md').write_text(text,encoding='utf-8')
print('report saved')
