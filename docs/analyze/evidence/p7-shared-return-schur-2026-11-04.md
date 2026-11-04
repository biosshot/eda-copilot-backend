# P7 shared-return conductive Schur, 2026-11-04

The private shared-return path independently factors the free conductive
interior of each copper owner. It streams owner contributions into one
sparse shared-trace Schur matrix, solves the trace, and back-substitutes in the
original physical coordinate order. This replaces the monolithic conductive
correction for both harmonic initialization and AC residual enrichment in
partition mode. Original finite pad and floating-island coordinates retain
their prescribed values in each correction. The solver checks every recovered
correction against the original free conductive equations.

The electric and magnetic actions still use the common original mesh. This
increment establishes exact conductive condensation for the tested partition
and leaves independently condensed complex fields, production section/local
composition and board-scale acceptance open.

## Focused release controls

The algebra control checks a separate interior for each of two owners and a
two-coordinate shared trace against the existing monolithic solve. It also
rejects a missing trace coordinate. The supervised release test passed in
0.047 s.

The same two-port continuous-return coupon from the [shared-interface
control](p7-shared-return-interface-2026-11-04.md) compared condensed and
monolithic correction on the same mesh at 1 kHz. It retained the six original
oriented faces and seven potential trace coordinates, six of them free in the
conductive correction. Both native and saved-result checks passed.

| Check | Result |
| --- | ---: |
| Owner interior coordinates | 27 / 2 |
| Free shared trace coordinates / upper Schur entries | 6 / 21 |
| Maximum original free conductive residual | 2.69120e-13 |
| Maximum complex S difference | 2.22045e-16 |
| Maximum relative copper loss difference | 3.19386e-16 |
| Maximum full-space relative KCL / KVL | 5.89895e-10 / 2.62156e-11 |
| Maximum relative interface KCL / absolute balance error | 1.26219e-10 / 1.26219e-10 A |
| Maximum wave-to-copper power discrepancy | 3.40439e-15 W |
| Planned retained conductive bytes, including one-RHS scratch | 121,215 B |
| Supervised wall time | 25.469 s |
| Peak tree private / RSS, 20 ms sampling | 9,555,968 B / 16,220,160 B |

The runs each used one thread, a 1,800 s deadline, 3,072 MiB native cap,
4,096 MiB tree private commit cap and 1 GiB host reserve. Artifacts are under
`debugging/p7-shared-return-schur-20261104/`, with separate `algebra/` and
`compact/` logs. Continuum accuracy and band coverage remain unverified.

The native addon build passed in 5m 11s. SHA-256
`4d39b23fff0bd748fa5a46309ce9270134af443eadec4c6cca2bef805fd825aa`.
