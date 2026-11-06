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

## Bounded STM32 result

Commit `587faf4` was exercised at 10 kHz, native 240 s/12 GiB, four near workers,
global Rayon one, RHS chunks one, maxIterations=4, cold isolated cache. All 844
copper regions, 65 islands and 89 barrels remain, with the same executed DSL
hash `fc8343d11f674ae24e39875294b689aab2cbf983eb6b62b0262552ce3a9c2c9d`.
The first launch was refused before computation because Windows commit headroom
could not cover a 14 GiB tree cap plus reserve. The actual run used a 12 GiB
tree cap, preserving the 12 GiB native request. It ended on `LOCAL_DEADLINE:
streamed tetra magnetic action` after 243.093 s, before the 250-second external
cap; memory did not trigger the exit.

| Partial operator counters | New axial run | Previous persistent-cache run |
|---|---:|---:|
| Successful pairs | 150,528 | 134,528 |
| Separated calls / summed elapsed | 136,702 / 155.478 s | 125,647 / 186.050 s |
| Adaptive calls / summed elapsed | 13,917 / 148.096 s | 8,885 / 78.526 s |
| Kernel-window wall | 111.644 s | 83.711 s |
| Axial accepted / fallback | 77,876 / 58,826 | not applicable |
| Selected order 4 / 6 / 8 | 0 / 110,094 / 26,608 | not recorded |

The new path accepts approximately 57% of separated calls; the remainder
retains original cubature. At the same nominal wall limit, about 11.9% more
pairs complete. This is a partial throughput observation, not full-board
speedup: later work contains more expensive adaptive calls. In particular the
new trace reaches 142,336 successful pairs at 71.661 s kernel-window wall versus
the old 134,528 at 83.711 s, but exact per-pair identities were not captured for
a controlled common-prefix benchmark. Do not infer a precise speedup factor.

Sampled peak private/RSS were 1,883,979,776 / 1,097,822,208 B (20 ms lower bounds),
far below the actual tree cap. The last estimated series allowance is at most
2.693e-13 relative; this excludes outer/coordinate/summation error and is not a
physical error certificate. The final progress record remains partial with a
window failure. No completed action/channel or board physical-gate admission
is established by these counters.

Local evidence: `debugging/p7-axial-tetra-stm32-250s-12g-tree/`, including stdout,
comparison, supervision and identity. Native core identity:
`b1f4aef88fc15de2f862ee4c9c3512c3c8b9c5735d68dfbaeb839b8502367e89`.
The refused preflight is preserved under `debugging/p7-axial-tetra-stm32-250s/`.

The <=600-second goal and case-matrix acceptance remain unmet. Before another
heavy attempt, classify axial fallback reasons and retain bounded exact geometry
samples for offline reproduction. Vertical integration alone leaves long-XY
anisotropy; general longest-edge integration is a candidate requiring separate
mathematical and numerical validation, not an accepted implementation.

## Fallback diagnostics

The detailed API preserves the existing candidate/wrapper result and records
four refusal categories: unsupported geometry, inner-series q cap,
range/roundoff-allowance refusal, and outer n/n+2 convergence. Parallel workers
retain at most one exact-geometry sample per category. The parent emits at most
four `P7_MAGNETIC_AXIAL_FALLBACK` records per operator, selecting representative
workers rather than guaranteeing the earliest traversal pair. Repeated reports
do not duplicate a category. Geometry is transported as original f64 bits with
indices, order and available coarse/fine/change/limit values.

Metadata reservation increases conservatively to 8192 bytes per worker; a size
control checks the bound. Logging is best effort and cannot fail the numerical
calculation on an output error. Samples and counters require the parallel path;
they do not alter tolerance, cached response identity policy, or physical input.

The combined external 120-second/2048 MiB supervisor passed thirty focused
controls in 9.188 s, including forced categories, unchanged wrapper values,
single-emission latch, axial and original actions, parallel bits, KVL, full R/L/C
controls and isolated chunk metadata. Sampled peak private/RSS were
21,688,320 / 34,222,080 B (20 ms lower bounds). Local output:
`debugging/p7-axial-reasons-line-final-controls/`. The native addon build also
passed in `debugging/p7-axial-reasons-line-native-build.log`.

The actual board fallback distribution remains to be measured. The next board
attempt is diagnostic and shorter than a full acceptance run; it must not be
reported as progress toward the ten-minute target merely because its timer ends.
