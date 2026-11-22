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

The original automatic finite-plane bend then passed on this addon, retaining
the geometry that previously failed the foil-only thin-cell screen. Two ports
at 1 and 3 kHz passed automatic extraction and exact 1 kHz P6 replay. Maximum
full KCL was 1.05709e-10, KVL 4.77827e-11 and wave/Joule discrepancy
5.65867e-15 W. The report plans 75,170,887 B and retains 12 foil prisms.

The supervised cold run took 1764.485 s, with sampled process-tree private peak
206,114,816 B (196.57 MiB), RSS 169,906,176 B, one solver thread and no limit
relaxation. An addon compilation overlapped the final portion of the run;
wall time is an observation, not an isolated performance benchmark. The native
budget was 1800 s / 3072 MiB; process-tree cap 4096 MiB plus 1 GiB host reserve.
This confirms low memory on this coupon but a costly first calculation.

The same process then restored the exact response from memory with no new
native call. A fresh-process disk replay passed in 1.906 s total (0.245 s in
the extraction/cache wrapper), with **zero native solves**, identical producer
evidence, exact samples and original strict gates. Its private peak was
204,652,544 B. Producer core identity is
`4174ec5f9e3d11a6941eba1f47f07e702be5a0dab3c2176172c0fda2f07cef72`;
cache key `b7b4ced1086e2abe8cd9b9e9e44e9222653a4fc84c5decc31d7360bd3f8b04b0`.
Artifacts: `debugging/p7-mixed-20261104/bend-cold`, `bend-restart`, `exact-cache`.

The actual mesh contains 168 copper tetrahedra (132 explicit-volume cells),
62 full potentials and 504 full current coordinates. Two enrichment rounds
grow the model from 4/2 to 8/10 potential/current modes. Thus three streamed
magnetic sweeps each evaluate 14,196 pairs; the elapsed time is not exhaustion
of the 16-round allowance. The current log cannot isolate costly near-pair
quadrature from repeated sweeps. Native deadline checks occur between pairs;
inner adaptive/self quadrature is not individually interruptible. The process
supervisor enforces the hard outer limit. No timing optimization or repeat
calculation was performed after this pass.

Independent physical accuracy, actual barrel AC response and real-board/case-
matrix acceptance remain pending. The response stays partial/unverified.
