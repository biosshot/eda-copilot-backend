# Copper geometry checkpoint

This packet is **A incomplete, B not started**. Production geometry/kernel,
public API and authored fixtures are unchanged. No accepted after-A geometry
or runtime speedup exists.

`regressions.json` freezes five original tracks and exact native source polygons
with board/preparation hashes, including STM668/556 and STM613 provenance.
`probe.rs` imports the actual unchanged native Builder/areas/model; placeholders
are resolved to absolute paths when compiled. Cached geo/serde_json dependencies
only; no Cargo changes. It emits five exact source replays, original raw endpoint
circles and an explicitly experimental two-point circle-only convex hull.

Saved results are in `debugging/p7-design-checkpoint/copper-geometry/`.
`audit.py` consumes those results, reconstructs i_float1.15 adapter rounding and
both GEOS intersections using Decimal100, reproduces the failed CDT triangle,
and rejects the circle-only hull using a genuine bridge counterexample. It also
tests an experimental rectangle-preserving hull. Neither is a general overlay
implementation. `corpus-audit.py` inventories all eight frozen preparation
snapshots and the unchanged source hashes, not a candidate native DC solve.
`plot.py` renders the evidence. Saved magnetic matrices and failed kernel batch
are reused unchanged; no kernel batch was consumed in this task.

Stop boundary: a common overlay representation needs an explicit contract for
coordinates, intersections, repeated clipping/drills and unresolved contacts.
Do not infer a new snap epsilon, area filter, precision grid or near/far shortcut
from this packet. The report lists the exact incomplete acceptance criteria.
