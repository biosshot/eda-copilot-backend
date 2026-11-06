# Actual slow callbacks and adaptive source order

## Capturing the actual work

`P7_MAGNETIC_SLOW_PAIR_PATH` enables bounded capture of successful order-12 adaptive callbacks taking at least 10 ms. The destination must be a new file. Fixed records include ordered original-cell indices, geometry hash, elapsed time and exact coordinate/result bits. An 8 KiB writer buffer is reserved in the memory plan; limits are 16 records and 64 KiB. Each record is flushed immediately. Capture I/O failures disable diagnostics rather than changing the physical solve. Without capture or trace, no per-pair timer is created.

Focused capture/file-bound tests, existing bounded pair controls and release addon/test builds pass. Runtime artifacts are in `debugging/p7-slow-pair-stm32-250s/`; this run kept the original STM32 geometry/ports, cold response cache, one Rayon thread, singleton covers, no scalar/cover cache and no section hints. Native/supervisor time limits were 240/250 s; memory budgets were 12/14 GiB.

The native deadline ended the run at 243.188 s outer wall. Sampled private/RSS peaks: 1,873,395,712 / 1,061,724,160 B. The final prefix had 1,471 adaptive calls taking 57.781 s and 62,526 separated calls taking 45.676 s. No complete magnetic action or electrical result.

Sixteen actual slow pairs were saved (9,878 bytes); their ordered geometry hash is 7372091862248227345. Replay passes bitwise comparisons in 1.21 s (`debugging/p7-slow-pair-replay.log`). Individual production pair times are about 37–40 ms. The separate fixed-all-order diagnostic visits 2,312 source points per pair and reports source order 10 for every point.

Here source order 10 specifically means `positive_source` selected its bounded far-tetrahedron cubature, evaluating 1,000 positive source samples per target point. It does not mean the later 15/31/63 source-XY refinement branch. This explains the expensive nested integration for these sampled pairs; it is not a census of all remaining pairs.

## Reducing primary source work

The source rule now selects the first order n in 4..10 satisfying the existing geometric remainder and rounding estimate, then checks the actual generated mass error. It preserves the tolerance and falls back to existing positive refinement if the complete bound does not pass.

The former exponent 18 is generalized to 2n−2: an n-point Gauss rule integrates the Duffy-transformed tetrahedron's Cartesian polynomials through degree 2n−3, including its Jacobian. The same remainder factor, rounding allowance and mass-error term remain. No negative value is clipped or physical support removed.

Each order has its own immutable `OnceLock` cache. A single monotonically growing rule was rejected during review because results would depend on cache history. The seven deterministic caches retain at most 2,989 sample points. Streamed per-source slots increase from 128 to 192 KiB, with existing construction scratch; direct quadrature's per-cell extra allowance increases to 128 KiB. The memory preflight test reflects the new reservation.

All 16 saved fixed-order-10 integrals pass a relative 1e-10 comparison against the adaptive implementation. This explicitly compares two quadrature representations; it is not a claim of bitwise equality to the old method. An initial release microbenchmark took 0.054 s total versus roughly 0.61 s for the old captured-pair replay. This is a local kernel measurement, not yet a full-board acceleration factor.

Original signed-action/full-KVL reference control, bounded-cache eviction/bitwise controls and two polyhedron controls pass. Logs: `debugging/p7-adaptive-source-action.log`, `p7-adaptive-bounded-tests.log`, `p7-adaptive-polyhedron-tests.log`, `p7-adaptive-captured-comparison.log`. Full shared-return automatic integration and case-matrix physical acceptance remain incomplete.

Final immutable-cache release comparison passes in 0.052685 s for all 16 pairs. Release addon build passes (`debugging/p7-adaptive-source-native-build.log`). The existing cache identity hashes the native binary, so previous cached responses cannot silently substitute for this changed implementation.
