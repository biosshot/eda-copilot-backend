# P7 exact magnetic pair reuse

The streamed 3D magnetic operator now supports `P7_MAGNETIC_PAIR_CACHE_MIB=0..4096` (integer MiB, default 0). This is a bounded direct-mapped cache of successful scalar quadrature results for ordered original-tetrahedron pairs. Geometry and quadrature order are immutable within an operator instance. Directions are not merged: `(i,j)` and `(j,i)` retain their original calculations. Replacement on collision affects reuse only. No tolerance, traversal, summation order, source ownership or physical domain changes.

The scalar array is reserved in the operator allocation plan before construction, together with existing geometry/quadrature working sets. Insufficient budgets refuse before allocation; allocation failure returns a memory error. Deadline checks also run on cache hits. `P7_MAGNETIC_TRACE=1` reports `pairCacheHits`, `pairCacheMisses` and `pairCacheEvictions`; kernel timings count computations, not cached returns. `P7_STREAMED_MEMORY_PLAN` records scalar bytes and slots. This cache is distinct from the optional cover-replay cache.

## Controls

- Two `bounded_pair_tests` check bitwise equality against uncached self/near/separated calculations, ordered reverse pairs, collision replacement, exact trace counts, parser and memory reservation, and expired deadlines on cached values.
- The signed-original-action/full-KVL enclosure control explicitly constructs the scalar cache using the same size plan. With 1 MiB it records 122 hits, 4 misses and no evictions; existing independent action and KVL reference checks pass. Setting an environment variable alone would not exercise the former manually constructed test cache, so its constructor was updated explicitly.
- Logs: `debugging/p7-pair-memo-control.log` and `debugging/p7-pair-memo-action.log`.

Final focused runs: pair controls 2/2 (0.04 s), cached action/KVL 1/1 (6.43 s), coupled charge coordinates 2/2 (0.05 s), shared-return controls 3/3 (22.64 s). The release addon build passed in 5m45s (`debugging/p7-split-pair-memo-native-build.log`). This build duration is not extraction time.

The small repeated-action control establishes exact reuse, not board-level speed. The first occurrence of each pair still pays the original integration cost. A working set larger than the configured cache can evict values before later axes/columns use them. The cache remains off by default; no <=600-second STM32 extraction or case-matrix acceptance is claimed.

## Actual STM32 bounded measurement

Release `c699d33`, unchanged RS485_B geometry/physical ports, cold isolated response cache, `P7_MAGNETIC_BATCH_COLUMNS=1`, `P7_MAGNETIC_PAIR_CACHE_MIB=1024`, cover replay off, one diagnostic iteration, 590 s native / 600 s supervisor, 12 GiB native / 14 GiB process-tree budget, one Rayon thread. No concurrent builds or numerical tests ran. Artifacts: `debugging/p7-pair-cache-singleton-600s-20261105/`.

The native operator reserved exactly 1,073,741,824 scalar-cache bytes (33,554,432 slots). All 844 copper regions, 65 islands and 89 barrels remained. First-column cover: 28.1088344 s, 39,743,535 visits, 3,531,639 far, 15,604,494 near and 739,382 zero blocks. This is not a common-batch cover or a full action.

The process ended after 593.25 s with `LOCAL_DEADLINE: streamed tetra magnetic action`; the supervisor labels this native refusal `validation-failure`, not a supervisor kill. First action reached 379,063 visits, 58,680 far and 106,918 near callbacks. Scalar cache: **0 hits, 106,918 misses, 182 replacements**. Kernel time: 358.2583671 s adaptive (8,285 calls) and 94.7564653 s separated (98,633 calls); counts include the failed final call. No completed magnetic action or electrical response.

Sampled process-tree private/RSS peaks: 2,951,393,280 / 2,153,713,664 B (20 ms sampled lower bounds). The configured cache therefore consumed memory without eliminating any computation in this prefix. It is not an effective first-pass acceleration for this STM32 run and stays disabled by default. Next acceleration must reduce unique near work or its integration cost; larger memoization cannot fix a prefix with no repeated pairs. The earlier shorter singleton run has a different prefix, so its wall time is not a paired speed comparison.

The parallel change to disconnected shared-return AC is documented in [the finite-domain EM controls](p7-shared-return-em-2026-11-05.md). Production automatic selection of that new candidate remains incomplete.
