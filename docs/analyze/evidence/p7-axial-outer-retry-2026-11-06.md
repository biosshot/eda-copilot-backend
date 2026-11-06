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
