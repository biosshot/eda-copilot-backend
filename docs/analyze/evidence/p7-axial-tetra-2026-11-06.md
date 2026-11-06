# Experimental finite-axial tetrahedron cubature

The [instrumented board prefix](p7-persistent-near-2026-11-06.md) identifies
separated double cubature as approximately 70.3% of summed worker kernel time.
This primitive reduces arithmetic within that kernel; it does not group or
remove physical tetrahedra, constrain currents, change via models, or identify
long transmission-line sections.

## Geometry and arithmetic

For an exactly vertical tetrahedron edge, reorder its endpoints to vertices
0/3. At fixed Duffy coordinates u/v, the remaining coordinate is a uniform
vertical segment. Its endpoints follow the original affine tetrahedron map;
the outer weight is `6 V wu wv (1-u)^2 (1-v)`. Source and target each retain
their independent P0 vector current. Unsupported or unresolved geometry returns
no candidate so the existing direct cubature remains available.

For half lengths ha/hb, center distance R and direction z/R, the normalized
double-line potential is a Legendre series in the centered axial difference.
Write A=ha/R, B=hb/R, u=(A+B)^2, v=(A-B)^2. Its even moments can be evaluated
without binomial powers or endpoint subtraction:

`S0=1; Sk=u*S(k-1)+v^k; M(2k)=Sk/((2k+1)*(k+1))`.

For q=(ha+hb)/R<1, the mathematical even-series tail after degree 2k is bounded
by `q^(2k+2)/(R*(1-q^2))`. The implementation limits q to 0.25, uses localized
coordinates and compensated origin subtraction, and compares outer orders n
and n+2 with a 1e-10 relative threshold. These safeguards are not a complete
numerical error certificate: the returned series/roundoff estimate is heuristic
and excludes outer, coordinate and summation errors. It must not be presented as
the full-action uncertainty or used alone to promote physical acceptance.

The existing default constructor and direct integration remain unchanged.
The experimental streamed dispatcher is described below. Standalone n+n2 rules
retain 20,224 B per cell across all 4/6/8 orders, plus vector headers. Streamed
use prepares only selected order-6/8 rules, at most 16,896 B per cell plus headers.
Line preparation is lazy and does not build unused refined 3D point arrays.

## Focused measurements

Seven release axial controls passed sequentially under an external 120-second,
2048 MiB supervisor in 4.922 s wall (4.72 s test time). Sampled peak private/RSS
were 14,192,640 / 32,710,656 B (20 ms lower bounds). Local output:
`debugging/p7-axial-tetra-final-controls/`.

| Outer orders | Line pairs | Old point pairs | New / old measured time |
|---|---:|---:|---:|
| 6 + 8 | 5,392 | 46,656 | 0.3896 / 0.5025 ms |
| 8 + 10 | 14,096 | 262,144 | 1.0217 / 3.463 ms |

These are fixture-specific timings, not a board-level speedup. Relative
differences from the original direct rules were 2.74044e-11 and 1.08683e-13.
A thick-tetrahedron refinement differs by 3.25e-15 between outer order pairs;
its analytic-source comparison differs by 1.43e-14. Other controls cover the
positive moment recurrence versus binomial evaluation, endpoint comparison,
zero transverse separation with disjoint segments, overlapping/unsupported
fallback, reciprocity, permutations and translation.

The existing parallel/signed-KVL/hybrid/captured controls also pass under a
separate external supervisor: `debugging/p7-axial-tetra-regression-controls/`.
The initial primitive addon build passed in
`debugging/p7-axial-tetra-native-build.log`; the integrated build also passed
(`BUILD_EXIT=0`, release 5m19s) in
`debugging/p7-axial-integration-native-build-final.log`.

## Experimental streamed integration

`P7_MAGNETIC_AXIAL_TETS=1` enables the candidate only for selected order-6/8
separated pairs; order 4 retains the original tensor. Each original tetrahedron,
three-component current, return path and physical gate is retained. Failed
geometry/series/outer-convergence admission falls back to original direct
cubature. The default is off; malformed values are refused.

Serial and per-worker admission each add `64 * 24 KiB + 64 KiB` before allocation,
covering retained line data and scratch. Worker caches prepare a rule only when
that order is actually requested, including a remembered unsupported result.
Progress distinguishes accepted/fallback counts and selected 4/6/8 orders. The
series estimate is reported separately and does not imply a total error bound.

Full and chunk magnetic certificates carry
`nearQuadraturePolicy: axial-tetra-estimated-v1` and
`nearQuadratureErrorCertified: false`. The existing interval/far-tail certificate
scope is unchanged; it does not certify the new quadrature estimate. Compact
reports preserve these tags, and TypeScript refuses missing/inconsistent tags
between basis, chunks and fresh-current checks. Cache identity is isolated at
both core and board-response levels; absent/0 preserve the off key. An executor
refuses changing the policy after its first checked call.

Additional validation: the forced thin separated-pair control proves candidate
acceptance and compares the complete near action against the old operator;
serial/parallel opt-in results match bits. An isolated real Streamed test checks
metadata on top-level and sequential chunks. These tests use external 120-second
limits; local logs are under `debugging/p7-axial-lazy-controls/`.

A real native mesh/channel control with the flag and four workers passed under
the 120-second/2048 MiB supervisor in 0.813 s. Its two solves recorded 648 and
504 accepted axial pairs, zero axial fallbacks, and maximum original-space KVL
relative residuals 2.322e-9 / 6.122e-9, below the unchanged 1e-8 gate. Local log:
`debugging/p7-axial-tetra-native-channel-control/stdout.txt`. This establishes
execution on that control, not board-wide applicability or physical convergence.

Twenty-one focused TypeScript cache/validation tests pass; three focused axial
identity/lifetime checks pass after the final guard addition, and typecheck
passes. Native compact-certificate provenance also passes its focused control.
Local evidence is under `debugging/p7-axial-tetra-ts-controls/`,
`p7-axial-tetra-ts-focused-final/`, `p7-axial-tetra-compact-control/` and
`p7-axial-integration-typecheck-final.log`.

The next bounded board run must measure actual acceptance/fallback coverage,
prefix kernel time and original physical residuals. No full-extraction,
<=600-second result or case-matrix acceptance is established here.
