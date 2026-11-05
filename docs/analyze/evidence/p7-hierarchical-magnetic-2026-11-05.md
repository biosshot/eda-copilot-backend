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

## Actual STM32 planning control

An ignored test loads the exact saved STM32 solver board and request, builds
the original mesh, and constructs the production tree once. It creates no
potential/current space, electric field or magnetic action. The memory base
uses the larger mesh retained/preparation bound, an explicit decoded-input
reserve and the separate original tetrahedron geometry buffer. Tree work is
admitted against the remaining part of the same 3,072 MiB budget.

| Planning-only result | Value |
| --- | ---: |
| Supervised wall time | 88.859 s |
| Original copper tetrahedra / sources / barrels | 154,899 / 844 / 89 |
| Tree nodes | 16,383 |
| Far cluster interactions | 2,012,017 |
| Remaining exact tetrahedron pairs per action | 2,283,342,452 |
| Retained mesh bytes | 180,928,643 B |
| Planned hierarchy bytes, including action scratch | 1,094,595,968 B |
| Total geometry/input/tree plan | 1,695,810,920 B |
| Peak tree private / RSS, 20 ms sampling | 529,580,032 B / 472,162,304 B |

The tree reduces the original 11,996,927,550 pair count, but approximately
19% remain exact. With 286 expansion terms, the current far contraction code
also loops over 286² term combinations for each far interaction. These counts
leave action runtime admission open. No field or magnetic action was run,
and no new parameter or geometry sweep was performed.

The test ran once under the same 1,800 s / 3,072 MiB native and
4,096 MiB tree private commit limits, with one thread and 1 GiB host reserve.
Results are under `debugging/p7-hierarchical-magnetic-20261105/actual-plan/`.
Only the library test target was rebuilt for this ignored test. The production
addon from the preceding batch is unchanged.

## Harmonic quotient and fine-tree control

The next representation retains degree10, qmax0.2 and the original full-KVL
gate. It uses121 real Laplace-quotient coefficients,2926 valid M2L couplings,
and2108 separable shift terms. Packed shell error bounds and sequential
Cartesian axes bound memory while the original near/self kernel is unchanged.
Production and planning use one-original-tetrahedron terminal leaves. These
are structural changes; the count reduction alone does not admit board runtime.

The first proof compilation failed with E0618 because a local action-error
vector shadowed the `error` function. The local was renamed `action_error`.
The preserved attempt is `debugging/p7-harmonic-proof-20261105/` (exit101,
12.484s supervised). After that source correction, the existing signed/reference
proof passed in0.16s with9 original Cartesian unit-current cases and2 signed
manufactured original-current KVL controls. The saved JS verifier passed:

| Proof quantity | Value |
| --- | ---: |
| Maximum observed action difference |5.169878828456423e-26|
| Maximum reported action enclosure |1.9387048534047071e-25|
| Maximum manufactured KVL upper ratio |6.623301405901401e-16|

The proof's cargo launcher exited0, but its supervisor recorded `launcher
exited before its descendants`. Its184.969s wall time includes compilation;
peak tree private/RSS were1512869888B/1420623872B. This supervision result
is not a clean supervised pass. The numerical test and saved verifier results
are recorded separately under `debugging/p7-harmonic-proof-fixed-20261105/`.
No numerical proof was repeated to repair the launcher status. A receipt binds
the prebuilt unit-test executable SHA256
`4265aef371976924c81ec3e67355b5fcd2a0659cd4a8bdc11da4e267789ebc4f`
to the successful proof's native source hashes.

One subsequent saved STM32 planning attempt launched that executable directly
with unchanged supervision gates. It refused before any field or magnetic
action, under `debugging/p7-harmonic-fine-tree-plan-20261105/`:

| Fine planning quantity | Value |
| --- | ---: |
| Supervised wall/native phase time |90.093s/89.9569762s|
| Exit/supervisor reason |101/null|
| Peak tree private/RSS |788045824B/619048960B|
| Geometry/input base |586344648B|
| Separate tetrahedron geometry buffer |14870304B|
| Native admitted budget |3220961280B|
| Field solves/magnetic actions |0/0|

The exact refusal was `LOCAL_MEMORY_LIMIT: hierarchical magnetic interaction
budget exhausted`. Source inspection identifies the retained-descriptor
length cap:154899*128=19827072 active entries. It was not an allocator failure
or proof that the admitted3GiB was consumed. The error report does not preserve
a far/near breakdown or the full remaining pair counts; none is inferred.
The planning result does not admit field preparation or electrical extraction.
No parameter sweep, cap increase or STM32 electromagnetic retry was performed.

The separate streaming-interaction followup remains staged and untested; its
purpose is bounded descriptor storage. It must expose full traversal/cost
counts before any electrical run, and cannot itself establish runtime or
physical acceptance.

The harmonic representation batch also passes eight focused TypeScript tests,
typecheck, package build and the native addon build (5m20s).
