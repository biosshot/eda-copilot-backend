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

## Longest-edge experiment: not selected

A test-only Duffy reduction selected each tetrahedron's longest physical edge,
kept positive physical volume weights, and compared order 6/8 with original
volume cubature 8/10. Common coordinate localization preserved relative
placement. The analytic segment primitive refused both a regular separated
control and the captured STM32 outer-convergence pair. A second attempt added
positive segment quadrature 4/6 with an unchanged 1e-12 segment-refinement gate;
both cases still refused. The required positive control failed. Neither attempt
is a successful tetrahedron model or speedup measurement.

Logs: `debugging/p7-node-budget-general-edge-controls-fixed/` and
`debugging/p7-general-edge-numeric-fallback-controls/`. The final experimental
source is archived as `debugging/general-edge-refused-experiment.rs`; the
unaccepted test module and its registration were removed from the source tree.
Production primitive and dispatch are unchanged. Do not interpret the quick
early refusal as a kernel acceleration. More expensive numeric fallback was not
selected without evidence that it would beat original direct cubature.

The simpler next candidate is outer-order escalation in the existing vertical
integrator. A NumPy feasibility replay (`debugging/axial_outer_replay.py`) on the
one real sample gives relative 6/8 difference 1.39558e-10, 8/10 8.55e-14 and
10/12 8.30e-16. Replayed order 8 agrees with saved native fine within 2.28e-15
relative. These are diagnostic arithmetic results, not native admission,
certified full errors, performance evidence or coverage of other pairs.
