# Bounded outer-order retry for axial tetrahedron integration

The actual STM32 prefix classified all 19,200 recorded axial refusals as
outer n/n+2 convergence, rather than unsupported geometry or inner series
failure. See [the captured diagnostic](p7-axial-tetra-2026-11-06.md).

With `P7_MAGNETIC_AXIAL_TETS=1`, separated original orders 6/8 now retry only
this refusal using n+2/n+4. The already calculated n+2 value is reused; only
the n+4 rule and integral are added lazily. The 1e-10 relative convergence gate
and all inner guards are unchanged. One retry is permitted. Unsupported,
unresolved and near/self cases retain their original fallbacks. Default axial
mode remains off. Physical source geometry, current DOFs and cover are unchanged.

Both original orders may coexist in one cached cell. Admission therefore raises
the axial extra allowance from 24 to 40 KiB per cell, in serial and every worker.
The arrays and headers require less than 40 KiB; the test checks both orders
together. Cache capacity is unchanged. Additional counters distinguish attempted
outer retries, accepted retries and final fallbacks. A fallback sample reports
the actual comparison order (original n+2 after a retry).

The deterministic control embeds original f64 coordinate bits from the captured
pair. It requires the original 6/8 refusal and successful 8/10 retry, compares
direct volume 8/10 and independent analytic-source integration at order 16,
and checks reciprocity and refusal. Measured retry/source-reference difference
is 1.6192e-14 relative. A second control exercises `BoundedPairs`, cache reuse,
serial/parallel identity and exactly one accepted retry with zero final fallback.

The initial combined release supervisor passed 34 controls in 11.625 s under
120 s / 2048 MiB, sampled private/RSS 21,065,728 / 34,271,232 B (20 ms lower
bounds), in `debugging/p7-axial-retry-island-initial-controls/`.

After the island helper's lazy-factor correction, the final 34-control suite
passes in 9.969 s in `debugging/p7-axial-retry-island-final-controls/`, with
sampled private/RSS 20,701,184 / 34,275,328 B (20 ms lower bounds).
The native addon build passes (`debugging/p7-axial-retry-island-native-build.log`,
`BUILD_EXIT=0`). TypeScript typecheck passes after adding explicit numerical
environment metadata to the STM32 diagnostic identity.

This is an estimated quadrature scheme. The returned inner-series estimate does
not certify coordinates, outer integration or the complete physical error.
Existing `axial-tetra-estimated-v1`/`nearQuadratureErrorCertified:false` provenance
remains mandatory; native core identity isolates changed cached results.
One captured pair is not all-case admission or full-board speedup evidence.
The <=600 s complete channel and shared-return automatic route remain unproven.

## Bounded STM32 result

Code commit `d6c8bc3`, cold isolated output
`debugging/p7-axial-outer-retry-stm32-250s-8g/`: native 240 s / 8192 MiB,
external 250 s / 8192 MiB tree cap, four near workers, one RHS per cover,
four native reduction iterations, global Rayon/training jobs one. Trace,
near-bound omission and pair/cover caches are unset. The identity records these
effective environment values explicitly. Planning selects all 844 authored
copper regions, 65 islands and 89 barrels; physical acceptance remains unverified.

Board SHA256:
`0159c43d43b59df4f59fb943d59f0d841d17ca38d6d5a7d5576920a4aee3cce3`.
Executed DSL SHA256:
`817e427e109f58e50b21267a98cec5b4ac4322c2b37c1445e9e1b471bc967b61`.
Native core identity:
`33f028f0c19122676ceef3c4eef65b699dbd7dc35063118a4ace167011bb35ec`.

The native deadline occurs inside streamed magnetic action at external wall
244.359 s. No complete electric/magnetic channel or S matrix is produced.
Sampled peak private/RSS is 1,885,405,184 / 1,097,347,072 B, a 20 ms lower bound;
memory admission is not the stop reason.

Final partial counters: 143,104 committed pairs in 1,118 completed windows,
132,335 separated calls, 10,886 adaptive calls, 127,790 axial acceptances,
58,645 attempted outer retries, 54,100 accepted retries and 4,545 final
fallbacks. Approximately 92.25% of attempted retries are accepted. Every final
fallback in this prefix is still outer convergence; no inner or unsupported
refusal is recorded. All selected orders are 6/8 (105,826 / 26,509).
Counters include work in a failed window and must not be equated to committed
pair totals or complete-action evidence.

Kernel-window wall is 103.076 s; summed worker separated/adaptive elapsed is
146.891 / 136.491 s, not OS CPU seconds. Geometry cache builds/hits are
136,622 / 149,820. No speedup is established: the earlier axial prefix completed
150,528 pairs by its 243.093 s deadline. Budgets, code and host conditions differ,
and successful prefixes are not whole-channel timing comparisons. Removing most
fallbacks did not demonstrate the required overall acceleration.

Next concrete preparation candidate: `ensure` still constructs all order-4/6/8
volume cubatures (792 points per cache miss), including accepted axial pairs and
analytic near work that do not use those points. A geometry-only/lazy-direct
path can preserve default behavior and worst-case admission, but needs bitwise
fallback, cache and parallel controls plus measured benefit before selection.
