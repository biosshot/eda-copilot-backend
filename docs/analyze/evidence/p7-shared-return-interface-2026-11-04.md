# P7 shared-return spatial interface, 2026-11-04

The internal mixed solver can assign the original copper tetrahedra to exclusive
owners. Each owner has local potential coordinates, including distinct shared
trace coordinates. The solver assembles incidence by mapping those coordinates
back to the common original mesh. Actual finite pad quotients are retained. The
electric and magnetic actions still use one common field, including interactions
between owners. Copper loss is recovered separately for each owner and checked
against the original solved loss.

The recovered interface reaction is the weak nodal conduction dual. A trace
row adds all owner reactions and the common displacement current, then compares
with the original net external injection. Original full-space KCL, KVL and power
checks still apply. This reaction is distinct from a pointwise normal current
on a single face.

## Supervised compact control

The continuous-return coupon used the same 78 copper tetrahedra and original
two port pairs in both solves. Two common mesh constraints resolved the
longitudinal and lateral legs of the partition. The test checked the original
face keys and both in-plane normal directions before solving. It also refused
an incomplete ownership map before field preparation.

| Check | Result |
| --- | ---: |
| Frequency | 1 kHz |
| Shared potential coordinates / oriented faces | 7 / 6 |
| Split vs. unsplit S and copper loss | Exact equality in saved verifier |
| Maximum full-space relative KCL / KVL | 4.69061414e-10 / 3.22518605e-11 |
| Maximum trace relative weld / KCL residual | 0 / 1.30634380e-10 |
| Maximum absolute trace balance error | 1.30620265e-10 A |
| Maximum weak owner conduction / displacement magnitude | 0.251817264 A / 1.39082463e-8 A |
| Maximum wave-to-copper power discrepancy | 3.40569586e-15 W |
| Supervised wall time | 29.688 s |
| Peak tree private / RSS, 20 ms sampling | 9,617,408 B / 16,171,008 B |

The test used a 1,800 s deadline, 3,072 MiB native allocation cap and one
thread. The supervisor used a 4,096 MiB private commit cap and 1 GiB host
reserve. The native test and saved-result verifier passed. The initial fixture
attempt had only one in-plane cut direction and failed after the port comparison.
One post-change run passed after adding explicit common mesh constraints.

The native addon build passed in 6m 06s. SHA-256
`0edd8fdf566f56340ceb19fb58f4dfecc7fd103d022c2247d7f724302825f79d`.

Artifacts are under `debugging/p7-shared-return-interface-20261104/`.
The original attempt is at the top level. The passing run is in
`reviewed-witness/`, with `supervision.json`, `foil-reduced.txt` and
`verification.json`.

This is a common-mesh interface and recovery control. Independent physical
accuracy, scalable owner-interior elimination, section-to-local assembly and
whole-channel P7 acceptance remain open.
