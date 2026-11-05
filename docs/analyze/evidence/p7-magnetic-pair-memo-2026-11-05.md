# P7 exact magnetic pair reuse

The streamed 3D magnetic operator now supports `P7_MAGNETIC_PAIR_CACHE_MIB=0..4096` (integer MiB, default 0). This is a bounded direct-mapped cache of successful scalar quadrature results for ordered original-tetrahedron pairs. Geometry and quadrature order are immutable within an operator instance. Directions are not merged: `(i,j)` and `(j,i)` retain their original calculations. Replacement on collision affects reuse only. No tolerance, traversal, summation order, source ownership or physical domain changes.

The scalar array is reserved in the operator allocation plan before construction, together with existing geometry/quadrature working sets. Insufficient budgets refuse before allocation; allocation failure returns a memory error. Deadline checks also run on cache hits. `P7_MAGNETIC_TRACE=1` reports `pairCacheHits`, `pairCacheMisses` and `pairCacheEvictions`; kernel timings count computations, not cached returns. `P7_STREAMED_MEMORY_PLAN` records scalar bytes and slots. This cache is distinct from the optional cover-replay cache.

## Controls

- Two `bounded_pair_tests` check bitwise equality against uncached self/near/separated calculations, ordered reverse pairs, collision replacement, exact trace counts, parser and memory reservation, and expired deadlines on cached values.
- The signed-original-action/full-KVL enclosure control explicitly constructs the scalar cache using the same size plan. With 1 MiB it records 122 hits, 4 misses and no evictions; existing independent action and KVL reference checks pass. Setting an environment variable alone would not exercise the former manually constructed test cache, so its constructor was updated explicitly.
- Logs: `debugging/p7-pair-memo-control.log` and `debugging/p7-pair-memo-action.log`.

Final focused runs: pair controls 2/2 (0.04 s), cached action/KVL 1/1 (6.43 s), coupled charge coordinates 2/2 (0.05 s), shared-return controls 3/3 (22.64 s). The release addon build passed in 5m45s (`debugging/p7-split-pair-memo-native-build.log`). This build duration is not extraction time.

The small repeated-action control establishes exact reuse, not board-level speed. The first occurrence of each pair still pays the original integration cost. A working set larger than the configured cache can evict values before later axes/columns use them. The cache remains off by default pending a representative bounded measurement; no <=600-second STM32 extraction or case-matrix acceptance is claimed.

The parallel change to disconnected shared-return AC is documented in [the finite-domain EM controls](p7-shared-return-em-2026-11-05.md). Production automatic selection of that new candidate remains incomplete.
