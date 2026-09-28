# Isolated block solver: micro vs geometric

Frozen native block inputs, beam + singles, up to two selected pair continuations. No board pack or post-refine. Common role-independent checkpoint selection.

Two freshly computed runs per mode (no solve-cache or saved-result replay). Six native workers; no concurrent compiler, placement process or tests. Ordinary host background load is uncontrolled. Report generation is outside timed regions. Both rounds must produce identical poses; the harness asserts this. Input and binary hashes and per-hypothesis input hashes are in manifest.json. Frozen native inputs include child geometry from the earlier micro run, deliberately identical in A/B; this does not measure recursive all-geometry block solving or a full PCB. No post-refine or board packaging is called.

Native timings include beam/singles plus at most two separately selected pair continuations. Both modes keep checkpoints and use the same role-independent blockQuality and legality gate. A missing legal candidate is explicitly reported, not treated as an acceptable layout. Every selected component rotation and layer is checked against input. Connection metrics use the same MST renderer inputs; intersections are geometric proxies, not proof of routing.

Run: node --import tsx scripts/experiment-isolated-block-metric.mjs

Report: node scripts/report-isolated-block-metric.mjs

Summary: micro 451.28 s, geometry 346.14 s (sum of per-block medians). These sums must not be compared directly with full-board elapsed time.
