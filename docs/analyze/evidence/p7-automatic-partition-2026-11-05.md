# P7 automatic conductive partition, 2026-11-05

Automatic DSL joint extraction requests the internal
`automatic-conductive-schur-v1` policy. The native selector divides the original
copper tetrahedra at the midpoint of their longest physical extent, with stable
axis tie ordering. It preserves the original finite electrodes and floating
component coordinates. A partition is selected when it has original shared
faces, free shared potential coordinates and free interiors on both owners.
An inapplicable candidate reports its reason and uses the existing conductive
correction before field construction or factorization.

The selected path factors owner interiors and the shared conductive Schur
matrix. Harmonic and AC residual corrections recover the original coordinates.
The full electric and magnetic actions, physical ports, source ownership and
strict residual and power checks continue to use the common original mesh.
Production output contains aggregate topology and correction evidence. Cache
identity binds the requested policy, and supplemental frequency comparisons
require the same mesh and ownership signature, including finite electrode
quotients and retained coordinates.

## Focused verification

Seven live TypeScript automatic-path and cache controls passed in 2.127 s.
Typecheck and package build passed. The initial cache attempt could not retain
disk entries in the sandbox temporary directory. The rerun used a temporary
directory inside the ignored workspace and passed without source changes.

The native geometry test passed in 0.079 s. It checks deterministic selection,
original coordinate mapping, inapplicability when all coordinates are retained,
and resource/deadline refusal. The automatic checked-endpoint comparison ran
once at 1 kHz on the existing compact physical foil fixture and passed in
5.703 s.

| Compact control | Result |
| --- | ---: |
| Original copper tetrahedra | 42 |
| Owner free interior coordinates | 7 / 12 |
| Original shared faces / free shared trace coordinates | 4 / 4 |
| Upper Schur entries | 10 |
| Maximum original conductive residual | 5.99017e-15 |
| Maximum complex S difference from monolithic correction | 1.11022e-16 |
| Maximum relative copper loss difference | 4.79127e-16 |
| Maximum full-space relative KCL / KVL | 6.49131e-10 / 9.11010e-11 |
| Maximum column power discrepancy | 4.78046e-15 W |
| Peak tree private / RSS, 20 ms sampling | 6,172,672 B / 13,578,240 B |

Both native controls used one thread, an explicit 1,800 s deadline,
3,072 MiB native cap, 4,096 MiB tree private commit cap and 1 GiB host
reserve. The native report confirms the admitted 1,800 s and 3,072 MiB.
Artifacts are under `debugging/p7-automatic-partition-20261105/`, with separate
`geometry/` and `compact/` supervision logs and `compact/comparison.json`.

The automatic policy routes eligible planar requests through the mixed operator,
which retains original volume coordinates for its full residual. This can
increase preparation cost relative to the previous planar producer. The common
field allocation and quadratic magnetic pair work remain resource constraints.
The compact controls establish implementation equivalence. Independent physical
accuracy, large-board resource improvement and complete P7 acceptance remain
unverified.

The native addon build passed in 6m 03s. SHA-256
`b607f5815f541dd4f251b293afac368949d7a39055f3a2f44d748cbfc3916dff`.
