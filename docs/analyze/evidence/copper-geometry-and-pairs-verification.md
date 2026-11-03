# Copper geometry and pair classification checkpoint

**Phase A incomplete; phase B not started.** This is a provenance and geometry
prototype packet, not an accepted production fix. Task664056e is copied alone.
No production source, tolerance, source identity, material/interface, physical
space, independent reference, runtime cap or public admission changed. No full
ROI, live export, kernel integral or addon rebuild was run in this task.

The exact cause of STM613 is now established. A local direct-boundary prototype
removes its artifact, but general source assembly, clipping/drills and subsequent
overlays remain unresolved. The common-coordinate proposal below changes the
overlay representation/contact contract, so it is presented for review rather
than silently implemented. This is not a proof that every possible remedy needs
a new tolerance; a tolerance-free robust overlay is an alternative, with larger
implementation scope. Do not treat the local prototype as completed extraction.

## A — implementation and evidence

Tracked tools and minimal regression inputs:
`tests/fixtures/analysis-local-extraction/copper-geometry/`.
Ignored raw packet: `debugging/p7-design-checkpoint/copper-geometry/`.

### Complete STM613 construction

The frozen source IDs are `88d9932c804b2bb2`, path
`copper.fixed.tracks[293]`, and `ddd0394190d6ae65`, path
`copper.fixed.tracks[294]`. Both are TOP/+3V3, width0.243mm, and share the exact
authored normalized endpoint(-3.116,9.13)mm. No local drill intersects this
witness. The previous audit's immutable source-pcb/Board/preparation hashes and
STM668/556 regressions are retained.

Actual unchanged `Builder::circle` produces the **same exact F64 cap vertex**
(-3.040245989074164,9.224992525119866)mm in both sources. Builder then unions the
endpoint circle/rectangle pieces independently. `geo0.31.0` unary_union builds
an i_overlay4.0.7 FloatOverlay per call. Its i_float1.15 adapter uses each call's
own bounds, midpoint and power-of-two scale; its integer conversion rounds,
including the log2 scale exponent. It does not truncate that exponent.

|Source|Adapter midpoint/mm|Scale|Restored cap vertex/mm|
|---|---|---:|---|
|88d9932c804b2bb2|(-3.6159999999999997,9.13)|2^30|(-3.040245988950133,9.224992524944247)|
|ddd0394190d6ae65|(-3.06295,9.07695)|2^32|(-3.0402459891706703,9.224992525164783)|

The reproduced adapter conversions match exact vertices in each native output.
All five frozen complete source polygons, including668/556's three tracks,
match their accepted snapshots coordinate-for-coordinate, symmetric difference0.
Thus this is an actual unchanged native replay, not a Python polygon substitute.

Source1 facet20 intersects source2 facets26 and27. Independent100-digit Decimal
arithmetic on their exact F64 endpoints gives, rounded to F64:

1. (-3.040245988950135,9.224992524944248)mm;
2. (-3.0429492531005464,9.226691098458947)mm.

Both match saved GEOS union vertices with **0ULP residual**. Source1's original
cap vertex is the third triangle vertex. A GEOS union of **only these two exact
prepared source polygons**, followed by constrained CDT, reproduces STM613 with
the exact same unordered vertex set. No neighboring third source, barrel
constraint, port, field mesh or mesher cleanup is needed to create it.

The first intersection lies only2.8435583831733384e-15mm from the original
vertex; the longer second intersection results from nearly coincident cap
facets. Native independent per-source rounding originates the divergent
boundaries; GEOS nodes their crossings; CDT preserves them. The finite35um prism
then inherits the bad conditioning. The earlier actual/instrumented source-rule
failures at12/8 and24/16 through positive order63 remain saved. They were neither
retried nor replaced by a zero contribution. There is no new accepted near-self
integral because there is no accepted post-fix source/union/mesh chain.

### Local prototypes and a real bridge counterexample

The standalone Rust probe emits original raw circle points and a two-point
convex hull of endpoint circles. It avoids per-source Boolean rounding and
removes the microscopic STM613 edge locally: the two-source union minimum edge
becomes0.027207367693103317mm. **This shortcut is rejected.** For the same
45-degree track it moves its straight broadside inward0.0007639664979707767mm.

A same-net authored0.0002mm-wide rectangular pad placed across that exact
straight broadside has native overlap2.0000023554169484e-8mm². Circle-only hull
opens a0.0006225451417334874mm gap. This is a real connectivity change, not a
microscopic artifact to discard. Its width is larger than the STM saved Boolean
bound7.152557373046875e-6mm. Curve approximation alone cannot justify losing it.

A second, experimental GEOS prototype takes the hull of the **raw endpoint
circles plus the exact four straight-rectangle corners**. It preserves the
bridge (overlap about2e-8mm²), retains shared cap facets, and has two-source union
minimum edge0.013625118678636451mm. For a single straight capsule this is an
inscribed construction: all vertices lie in the ideal capsule; it contains the
convex hull of the inscribed endpoint circles. Consequently its Hausdorff
distance from that ideal capsule is bounded by the existing circle sagitta,
without assigning a new snap epsilon or deleting any triangle.

This positive local result is **not** a general fix: source polylines may have
turns, overlaps/holes; unequal widths and pads have different primitive
boundaries; board clipping and drill subtraction invoke more independently
bounded overlays. Reintroducing the existing per-call integer adapter there
does not establish a common representation. No production special case was
promoted on the strength of these two sources. These prototypes do not prove
translate/rotate, different-width, gap/bridge and material-interface invariance
through the entire required chain.

### Complete frozen-corpus baseline

All eight available frozen boards and their accepted preparation snapshots were
read, their original Board hashes verified and all region source IDs preserved
in `corpus-baseline.json`. Geometry-only STRtree intersections inventory the
same-layer/same-net contact graph. These component counts deliberately exclude
through-layer barrel connectivity; they are **not** replacement native DC or
connectivity results. No live export or full-board magnetic/DC solve occurred.

|Frozen board|Regions|Vertices|Holes|Same-net contact edges|Same-layer components|
|---|---:|---:|---:|---:|---:|
|BandAmpV5|319|26550|539|325|8|
|ESPower|668|32687|317|709|104|
|NanoScope|3019|211009|2058|2932|622|
|NanoSpec|2509|158231|2210|2599|253|
|STM32G0_CAN_RS485|844|36189|238|926|154|
|ThunderF722|1895|148095|1828|1340|760|
|TPS63060DSCR|172|9834|124|309|12|
|USB_PD_Trigger_5A|388|27603|362|495|38|

There are real multi-point source tracks in ESPower and NanoSpec; they cannot
be silently treated as straight capsules. Saved invalid polygon and cross-net
contact diagnostics are retained separately, without reinterpreting them as
new physical shorts or silently repairing them. Current areas/primitives source
hashes equal their frozen before-task bytes. Thus production differences are
zero, but this is **not** an accepted candidate whole-corpus topology gate.

The three ROI baseline counts are reused unchanged from the accepted census:

|ROI|Prepared regions/vertices|Union vertices|Foil/axial supports|Candidate current coefficients|
|---|---:|---:|---:|---:|
|STM|54/5985|633|1002/192|2100|
|TPS|62/4853|1010|2178/576|4644|
|Band|60/10697|1398|2962/768|6308|

These remain candidate counts before physical constraints, not an assembled ROI
current space. **After-A counts and a post-A baseline do not exist.** No mesh
reduction or runtime speedup is attributed to either local prototype.

### Concrete proposal requiring review

Do not add a free snap radius or a small-area/slenderness deletion rule. Proposed
next production mechanism is a source-preserving overlay context shared by
primitive assembly, board clipping, drills and layer union. Preserve authored
primitive/source IDs, nets/layers, common cap facets, holes and interface chains.
The context must also define a canonical identity for newly intersected nodes;
sharing only original circle vertices is insufficient.

One bounded implementation route is a common dyadic adapter rather than today's
independent origins/scales. Before implementation, explicitly approve its
coordinate/error contract: context extent and overflow bounds, per-operation
and cumulative displacement, treatment of transformed primitives, and a
**reported unsupported contact** when a genuine gap/bridge cannot be resolved.
Relate all bounds to the existing approximation/Boolean budget; do not silently
merge different nets or discard genuine narrow bridges. Shared grid alone is
not a proof of topology preservation, and may be inadequate for the required
smallest features. An exact-predicate/rational-intersection source overlay
without coordinate quantization is the alternative if no topology resolution
limit is acceptable. That alternative has not been implemented or priced here.

This is a proposal, not a newly imposed tolerance or a claim of impossibility
for other methods. Stop at this contract decision per taskA§3. Required next
evidence remains: shared node/turn/different-width/translate/rotate fixtures,
distinct-net near gaps, genuine thin bridges, cutouts/drills/pads/pours and
material/port identities; all8 frozen production geometry/connectivity diffs;
3ROI source→union→mesh counts; independently checked finite-prism self integral;
focused geometry/connectivity/conduction/DC tests; independent magnetic
coupon/energy regressions. A production path change then requires the agreed
isolated jobs1 addon build. None of these unexecuted gates is marked PASS.

## B — implementation and evidence

**Not started, because A acceptance is not met.** `near_far.rs` and all current
geometry/current bases remain unchanged. No individual near-leaf classification
diff, after-A baseline, new pair census, matrix, aggregate error bound or speedup
claim exists. Existing leaf1 coupon matrices, independent reference, PSD,
reciprocity and zero/weak-mode evidence remain preserved; they do not validate
an unimplemented phaseB or the local geometry prototypes.

The old copper audit's shape-stratified costs, unpriced strata, kernel refusals
and conditional1.93–2.67 work ratios remain historical diagnostics, not guaranteed
benefits. Existing matrix/error/cost PNGs are retained in the previous packets;
the new `geometry-evidence.png` shows sources, GEOS noding and the real bridge
counterexample, and was visually inspected.

The short geometry-only Rust build/replay used cached dependencies; build about
12.1s, five tracks, zero kernel calls. The full saved corpus audit took5.31s.
No native coupon batch was consumed, no previous green kernel run repeated, and
no resource cap raised. The short heavy slot was agreed with Thermal and
released immediately. The raw original build did not record peak memory, so no
measured512MiB claim is made for it. No new agents or commits. STOP for review;
P7/P8/P11 and production geometry acceptance remain open.
