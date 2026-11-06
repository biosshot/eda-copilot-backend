# General segment-pair primitive

The standalone `line_pair::average_segment_pair` computes the normalized
unit-parameter integral of inverse distance between finite segments. Its units
are inverse meters; it contains no current-direction dot product or magnetic
constant. It is not used by production dispatch or tetrahedron outer cubature.

For nonparallel vectors a/b and start separation r, the parameter domain maps
to the parallelogram `r, r+a, r+a-b, r-b`. Its surface Coulomb potential divided
by `|a cross b|` gives the required average. The implementation uses edge log
terms and two oriented solid angles, with compensated relative coordinates and
summation. Positivity, angular, range and cancellation guards return no
candidate when unresolved. No geometry clipping is performed.

Exactly parallel/antiparallel cases use the existing finite-axial kernel divided
by the length product. A compensated cross product prevents rounded
near-parallel or collinear cases from being treated as a safe separated pair.
Collinear cases, including overlapping diagonal segments, conservatively return
no candidate. The existing axial function now exposes its endpoint-term scale;
its old value arithmetic remains unchanged. The parallel estimate depends on
this cancellation scale instead of a fixed relative number.

All error estimates are heuristic. The function is experimental infrastructure,
not a complete numerical error certificate or an accepted physical model.

The focused release control compares independent positive 32/48-point double
quadrature on skew, perpendicular and parallel examples. Differences were
approximately 1.6e-15 to 2.7e-15 relative; reference refinement was at most
1.7e-15. Reciprocity, orientation reversal, antiparallel direction,
rotation/translation, scaling, near-parallel refusal, far cancellation and
collinear fallback controls pass. It also passes in the combined thirty-control
external supervisor (`debugging/p7-axial-reasons-line-final-controls/`). The
native addon build passes (`debugging/p7-axial-reasons-line-native-build.log`).

Possible next use is longest-edge tetrahedron integration to reduce outer
variation of long thin cells. Before integration, replay actual board fallback
geometries, measure conditioning/admission and time, validate outer refinement
and original-space residuals, and account added storage. This primitive alone
does not preserve long 2D transmission sections in automatic routing or prove
the <=600-second complete-extraction target. No case-matrix status is promoted.
