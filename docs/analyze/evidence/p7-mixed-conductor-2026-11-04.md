# P7 mixed foil/volume conductor, 2026-11-04

Status: integrated numerical implementation under end-to-end verification. This document
does not establish physical case-matrix acceptance.

The mixed map retains the original native copper tetrahedra and finite electrode
faces. Complete admissible planar prisms provide reduced initial coordinates;
barrels, thin-screen failures, contested projected contacts and an adjacent
physical-face halo retain volume coordinates. It neither substitutes ideal vias
nor joins conductors by their net names. Original galvanic components remain
separate, including floating copper and purely capacitive coupling.

One original electric field operator supplies projected and full-space actions.
The existing tetrahedral magnetic pair rule supplies streamed common actions,
including foil/volume cross terms, without retaining a full quadratic kernel.
Pair evaluation time remains quadratic; this change provides no arbitrary-board
runtime guarantee or magnetic quadrature accuracy certificate.

The reduced basis starts from mixed harmonic coordinates, completed by the full
zero-contact DC correction. Each added potential shape includes its compatible
resistive-gradient current as well as independent current corrections. Final
matched columns are checked in the original full space with the existing
1e-8 KCL/KVL and wave/copper power guards. Zero projected incidence and a C-only
initial basis are valid; their full currents remain available for enrichment.

## Development evidence

- Actual via geometry/map control: 16 foil prisms, 432 explicit barrel tetrahedra,
  392 shared surface rows; 2552 mixed versus 2664 original current coordinates.
  Projected incidence relative discrepancy 1.14e-16, resistance/power 6.28e-15.
  Original span, finite contacts and galvanic islands are retained.
- The first compact mixed response exposed valid zero-incidence columns rejected
  by the initial basis guard. After that correction, the same physical control
  still failed full KVL after 16 iterations. Full DC completion fixed the initial
  contact mismatch, but stable AC enrichment also required pairing each voltage
  correction with its gradient current. Neither geometry nor gates were changed.
- With both corrections, the same one-point 1 kHz, two-port release control
  passed in 2.91 s. It contains 42 native copper tetrahedra, represented initially
  by eight foil prisms and 18 volume tetrahedra. Final ranks are 11 potentials and
  14 currents after three enrichment steps. Maximum full KCL 6.66244e-10,
  KVL 2.18284e-10, column power discrepancy 4.77916e-15 W; planned 48,629,431 B.

The integrated addon build passed in 5m09s, SHA-256
`2ef413381454d868c01649d68360cd24ad371c2a63544c06914409001fc246a7`.
The original planar shared-grid regression also passed after the common-basis
refactor (8.02 s). The no-FEM compatible-gradient/C-only seed regression passed.
Combined focused DSL/cache checks and typecheck passed. Initial disk-cache tests
hit Windows sandbox `EPERM` for atomic rename in system TEMP; rerunning with a
writable workspace TEMP passed, without changing atomic production writes.

Actual-addon automatic mixed extraction, independent physical accuracy and
real-board/case-matrix acceptance remain pending. Reported byte plans are not
OS memory measurements.
