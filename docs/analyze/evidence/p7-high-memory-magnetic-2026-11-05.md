# P7 high-memory magnetic profiling, 2026-11-05

Continuation authorized by the user: raise memory limits and prioritize runtime. Original geometry, ports, numerical tolerances and case-matrix remain unchanged. GPU and via 2.5D remain excluded. This is performance evidence, not physical acceptance.

## Implemented

- `0a015c4`: explicit joint board-foil budgets up to 65536 MiB, unchanged defaults; opt-in ordered cover cache (`P7_MAGNETIC_COVER_CACHE_MIB`), default off. Cache reserves twice the configured capacity for transient Vec growth, falls back to recomputation if unavailable/overflowed, preserves ordered callbacks. Tiny off/on/one-byte-fallback controls match actions, bounds and cover/envelope signatures exactly. Five TS cache and seven supervisor tests plus typecheck pass; release addon build passes.
- Follow-up found the DSL local/group ingress still limited developer budgets to 8192 MiB. Both now accept the same explicit ceiling; typecheck passes. The first failed numerical launch is recorded below rather than presented as a successful budget test.
- Classifier candidate reuses interval powers sequentially (13 multiplications instead of 91 at maximum order 12) and hoists loop invariants, with unchanged operation order and goals. Bitwise power-equivalence and existing signed-action/KVL controls pass.
- Near-kernel diagnostics distinguish self, separated and adaptive integration, with inclusive cache-preparation times. Counts include failed calls; final/partial records must not be treated as completed action certificates. Signed-action/KVL proof passes with tracing off and on.

## Larger-memory baseline

Machine physical RAM is about 16 GiB. A proposed 18 GiB process-tree cap was refused before launch because current Windows commit headroom could not also cover the 1 GiB reserve (`debugging/p7-high-memory-baseline-600s-20261105/`). A 17 GiB cap passed preflight; its first attempt stopped after 3.015 s at the remaining DSL verification ceiling (`debugging/p7-high-memory-baseline-valid-600s-20261105/`).

After the DSL correction, the cold run used **16 GiB native / 17 GiB process-tree private cap**, 590 s native / 600 s supervisor, unchanged inputs and one enrichment iteration. Artifact: `debugging/p7-high-memory-dsl-fixed-600s-20261105/`. One iteration is an explicit diagnostic restriction, not proof that default 16-iteration extraction completes.

The run ended with native `LOCAL_DEADLINE` after **594.265 s** outer wall time. Peak sampled private/RSS: **1,872,437,248 / 1,039,785,984 B**. Memory admission was no longer the blocker. All 844 copper regions, 65 islands and 89 barrels were retained.

| Phase | Observed wall / work |
|---|---|
| Geometry moments | 4.030 s |
| Common cover walk | 477.943 s; 394,309,166 visited node pairs |
| Classification within cover | 428.703 s (nested timer; do not add to cover wall) |
| Accepted interactions | 45,020,385 far; 150,044,919 near; 2,137,435 zero |
| First source/upward pass | 1.107 s |
| First application prefix | 16.650 s; 789 near calls, no far callback reached |

This establishes two bottlenecks: expensive cover classification and costly near integration. The first 789 interactions are a traversal prefix, not a representative throughput estimate for all 150 million near pairs. No full magnetic action, final error certificate or electrical result completed. Cover caching alone cannot accelerate the first construction or eliminate near integration.

## Optimized comparison

Commit `b264668` contains the incremental classifier, near diagnostics and DSL ceiling follow-up. Release addon build passed (`debugging/p7-classification-native-build.log`). The attempted 17 GiB tree cap was refused before launch because Windows commit headroom had declined (`debugging/p7-classification-optimized-600s-20261105/`).

The comparison used **12 GiB native / 14 GiB tree cap**, identical geometry/ports/goals and one-iteration diagnostic setting, 590/600-second deadlines, with cover cache disabled. This budget difference is a comparison limitation; both runs passed preparation admission and produced exactly the same cover interaction counts. No extra heavy workload was deliberately run during the timed cover phases. These are single-run observations, not repeated benchmark statistics.

Artifact: `debugging/p7-classification-optimized-valid-600s-20261105/`. Outer wall **593.578 s**, terminal `LOCAL_DEADLINE`; sampled private/RSS peaks **1,873,068,032 / 1,046,114,304 B**. No completed magnetic action or electrical response.

| Metric | Before | After |
|---|---:|---:|
| Complete cover walk | 477.943 s | 297.503 s |
| Classification nested timer | 428.703 s | 247.229 s |
| Visited node pairs | 394,309,166 | 394,309,166 |
| Far / near / zero accepted blocks | 45,020,385 / 150,044,919 / 2,137,435 | identical |
| Near calls reached before deadline | 789 | 7,038 |

The measured cover stage is **1.61x faster (37.8% less wall time)**; classification alone is 1.73x faster. This is not a 1.61x full-extractor claim. The different reached near prefixes cannot establish a near-kernel speedup.

Final near trace: self 113 calls / 0.007686 s; separated 3,567 / 6.102996 s; adaptive 3,358 / 173.531130 s. Adaptive integration consumed **96.6% of measured near-category time**, including cache preparation. Failed calls are included; native deadline interrupted the action. Near integral tolerances were unchanged. Far callbacks were reached, but the traversal prefix is not representative of all far work.

## Disposition and next bottleneck

Memory admission no longer hides the runtime bottleneck. The verified classifier optimization is committed; cover replay is available only as an opt-in candidate and has no full-board speed claim. No geometry, remote via, island, model case or numerical gate was removed/relaxed. The case matrix remains physically unaccepted (42 pending); focused controls do not establish universal extraction acceptance.

**Full extraction within 600 seconds remains unachieved.** The remaining cost is not solved by a larger memory ceiling. A subsequent optimization must reduce the number/cost of adaptive near integrals: assess per-column/small-batch covers instead of the mixed maximum envelope, or a certified better near-field representation/shared-return localization. Common envelope combines spatial maxima from different columns; overconservatism is a hypothesis requiring a controlled comparison. Scalar near-pair caching can avoid work across columns but cannot remove the first expensive integration. Simply weakening its hardcoded quadrature tolerance is not justified by the current interval action certificate, which treats kernel values as points; any approximation needs separate numerical/physical controls.
