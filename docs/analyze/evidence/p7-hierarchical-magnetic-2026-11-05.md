# P7 hierarchical magnetic action, 2026-11-05

Checked mixed extraction now uses one fixed, reciprocal Newton tree on the
original constant-vector tetrahedron current coordinates. The median tree uses
order 10, leaf size 32 and maximum separation ratio 0.2. Every basis,
enrichment and recovered-state action uses that same geometry and order.
Canonical original near and self pairs use the existing numerical kernel.

Accepted far interactions use a Cartesian Taylor polynomial and exact analytic
tetrahedron moments evaluated with outward F64 intervals. For cluster radii
a and b and center distance R, q = (a + b) / R must be below one.
The uniform Newton remainder is bounded by q^(p+1) / (R (1 - q)).
Signed currents use an absolute source-weight sum for the error bound.
Both sides of each interaction use the same transposed block. Symmetry alone
does not certify positive energy, so existing projected-matrix and power
checks continue to apply.

Each checked response recomputes magnetic actions on the actual recovered
original real and imaginary currents. Original resistive products and the
anchor-difference voltage gradient are also enclosed in intervals. The KVL
gate uses an upper residual bound divided by a lower normalization bound,
with the existing 1e-8 threshold. Reports and cached responses carry the
operator identity, fixed tree signature and per-column uncertainty.
The certificate covers far truncation and arithmetic. Existing near/self
quadrature, electric-field, mesh/domain and material accuracy remain
unverified.

## Focused verification

The independent tiny control used three original tetrahedra, four exact
pairs and two accepted far-cluster interactions. It exercised all nine
Cartesian unit current columns and two opposite signed drives. The separate
fixed-order reference includes its own interval moment-defect and remainder
bounds. The saved JavaScript verifier checks action enclosures and two manufactured
original-current KVL controls. The native control separately checks reciprocity.

| Check | Result |
| --- | ---: |
| Tiny proof supervised wall time | 0.359 s |
| Maximum measured action difference | 5.16988e-26 |
| Maximum action error enclosure | 2.32645e-25 |
| Manufactured KVL upper relative residual | 7.20797e-16 |
| Existing compact endpoint supervised wall time | 48.062 s |
| Compact peak tree private / RSS, 20 ms sampling | 4,096,000 B / 11,722,752 B |
| Compact split/monolithic maximum complex S difference | 1.11022e-16 |
| Compact maximum relative KCL / certified KVL | 6.93344e-10 / 8.86662e-11 |
| Compact maximum KVL uncertainty contribution | 1.08468e-18 |
| Compact maximum column power discrepancy | 4.78046e-15 W |

Both native controls ran once under one thread, an explicit 1,800 s
deadline, 3,072 MiB native ceiling and 4,096 MiB tree private commit cap
with 1 GiB host reserve. The compact case had three tree nodes and
903 near/self pairs, with no accepted far blocks. It verifies the production
certificate path and conductive partition against the monolithic correction.
The separate tiny proof verifies accepted far interactions.

Eleven TypeScript transport/cache/audit/evidence tests, typecheck and package
build passed. The first compile-only attempt found helper visibility errors.
Only the two internal helper visibilities were corrected before compilation.
The existing forced-mixed report guard was also corrected to allow an
automatic partition whose original cells all admit the foil mapping.

The compact control previously took 5.656 s with the bounded direct evaluator.
Fresh recovered-state actions and interval arithmetic increased its time to
48.062 s. Larger spatial geometry can reduce pair work through far blocks,
but full-board runtime and physical accuracy have not been admitted.

Evidence is under `debugging/p7-hierarchical-magnetic-20261105/`, with
separate proof and compact supervisor logs and comparisons. No STM32 field
solve was performed for this increment.

The native addon build passed in 5m 59s. SHA-256
`a2668d1ba38b2f111c436cedbfbcc4c3bad90a1a925c1904328c2b62a29484bf`.
