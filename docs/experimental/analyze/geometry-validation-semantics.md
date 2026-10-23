# Geometry validation semantics

CPU/F64, millimetres, absolute board Y-down. Validation consumes the existing
physical copper preparation, its holes, contacts and conservative islands. It
does not merge regions by net name, infer a refill, repair input or extract a
channel. Structural `.prepare()` remains distinct from physical coverage.

* `clearance`: minimum Euclidean distance of closed copper regions on the same
  layer, including hole boundaries. Overlap/contact is zero. Equal nonempty known
  net labels are exempt from this rule only, without creating connectivity.
  Unknown labels never establish equal nets: close unknown-net pairs are reported
  as uncertain observations, with conditional coverage, not electrical shorts.
* `edge-clearance`: minimum distance to the actual outer outline and cutout
  boundaries. Drill holes are excluded. Existing preparation clips copper; an
  internal optional observer retains only discarded outside fragments for this
  rule's source witnesses. Copper outside material is a distinct violation even
  if its nearest edge distance is positive. Missing/invalid outline is unavailable. Outside findings include an interior point
  farther than the error allowance from both source and board contours; thin
  unresolved fragments remain uncertain.
* `connectivity`: confirmed shorts and uncertain contacts reuse the preparation
  graph. Separate islands are observations, never declared opens. RoutingBoardV2
  net labels do not specify an intentional connected terminal group/open rule;
  open verification is explicitly unavailable. A short marker is a representative
  reliable contact in the proven physical island; the complete participating
  regions and existing contact graph, rather than that one segment alone, prove
  the multiple-net connectivity. It is not automatic fault localization.
  Connectivity has no spacing threshold: its `actual_mm` is the representative
  witness distance and `required_mm=0` is not applied as a distance criterion.
  The requirement is at most one known net per reliable island; uncertain
  contacts are unverified observations, regardless of same/different labels. Consequently an empty connectivity
  finding set is not proof that the board has no connectivity defects.

Scope selectors form a union: selected complete physical regions are compared
against all relevant neighbours, including neighbours outside the selection.
Region selectors select intersecting copper, without clipping its shape. Exact
pad IDs, pad designator/number, components and exact net labels are supported;
missing selectors reject instead of silently selecting nothing. Empty explicit
scope rejects. Duplicate selectors do not duplicate findings.

Measurements retain nominal distance, required threshold, an absolute error
allowance, witness endpoints, region/source/net/layer identity and source polygon
references. Each source polygon is stored once per report. Confirmed clearance
failure requires `distance + error < minMm`; confirmed pass requires
`max(0, distance - error) >= minMm`. The remaining band is uncertain, without increasing
the allowed clearance. With zero error, equality passes. Edge rules at minMm=0 still require
  resolved board containment; a nominal distance within the contour allowance
  remains uncertain. Error combines existing
curve/drill approximation and boolean conversion allowance; it is not a topology
proof. Snapshot findings remain conditional on unknown/stale fills, omitted
geometry, uncertain drills/contacts and accepted preparation limitations.

Stable validation region IDs derive from source kind/ID, layer and canonical
polygon coordinates; original graph IDs are remapped. Sources without IDs retain
path provenance and an identity limitation. Findings are sorted/deduplicated by
rule, threshold, source region identities and status. Same geometry and source
IDs in another input order has the same finding identities.

The count is of confirmed violations only. If coverage is incomplete, the aggregate count is unavailable, regardless of the number of confirmed
findings (`equals: 0` is unknown). Confirmed items and an explicitly labeled
confirmed-report count remain available separately. Measurements/uncertain observations remain
available and do not become confirmed defects. Unsupported channel rules reject
explicitly. Computation/resource errors reject the job, never return an empty
complete result. The existing process runtime handles cancellation/deadlines and
allows a following independent job. Native candidate/segment/report and memory
admission caps additionally bound allocation; no cache or GPU is introduced.


Native admission accepts 1–32 authored rules (duplicates are deduplicated),
`minMm` in 0–1000, at most 1000 scope selectors, native time in (0,3600] seconds
and memory planning in 1–4096 MiB. Default caps are 1,000,000 object candidates,
5,000,000 exact point/segment checks, 100,000 measurements, 2,000,000 report
vertices and 96 MiB encoded report. Explicit internal caps may decrease these or
increase them only within the declared hard maxima (2M candidates, 20M exact
checks, 200K measurements, 4M vertices and 128 MiB report). Public DSL uses the
fixed native caps and its existing time/memory budget. A cap is a refusal, not a
request to omit findings, simplify polygons or change clearance.
