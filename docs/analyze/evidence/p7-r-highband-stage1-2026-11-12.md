# P7-R high band, stage 1 steps 1.3 and 1.4 (2026-11-12)

Physical accuracy of every model stays `unverified`. Steps 1.1 and 1.2 were not done; the KVL criterion is unchanged.

## Code (`native/analyze/src/extraction/local/foil_reduced.rs` only)

- 1.3 `P7_ENRICH_MAX=<1..256>` (`enrich_max_override`, read in `solve_compact_adaptive`): replaces `grid.max_iterations` (16, or 24 with the continuation extension). `growth = 4 * max_iterations`, `voltage_cap`, `current_cap`, the training loop and `enrichment_continues` all read that value, so the vector caps follow the limit. Unset: unchanged. Invalid value: `HYBRID_INPUT` error. Log line `P7_ENRICH_MAX limit=64`. The 1..=24 check in `mixed_conductor.rs:1501` runs before this point on the requested value, so no other file was touched.
- 1.4 `append_current_mode` took `resistive_action` of every basis mode per appended vector, and `metric_mode` recomputed the whole Gram (k^2 dot products over the full current vector) per appended vector. New `ModeCache` (field `ReducedSpace::resistive`, local in `solve_compact_adaptive`/partitioned setup) holds `actions[i]` and the lower Gram `gram[b][a] = sum(basis[a], actions[b])` next to `basis.current`; an append computes one new row. The cached values are the same expressions `metric_mode` evaluated before, and the new action is the `image` `metric_mode` already returned. Modes added elsewhere (harmonic seeds) are filled in lazily. First attempt stored only the actions: grow time did not change (700 kHz iteration 17: 6.53 s vs 6.29 s), the Gram was the cost. Both are in the code now.

## Checks

- `cargo test --manifest-path native/analyze/Cargo.toml --lib foil_reduced`: 29 pass, 2 fail (`partitioned_plan_omits_only_unused_global_sparse_assembly`, `local_hierarchy_encloses_signed_sparse_action`), the same two as on HEAD.
- Coupon `diag_compact_kvl_100k_vs_1m` (`debugging/p7-r/h/stage1-coupon-after.log`, after the actions-only change, and `stage1-coupon-after2.log`, final): passes; the `maxKvlRelative` history at 100 kHz (6 iterations, 1.1e-3 ... 1.0e-9) and 1 MHz (10 iterations, 1.1e-2 ... 1.1e-9) is bit-identical to `coupon-stage05-on.log` (diffed on the printed values).
- `npm run native:build` twice (8.5 min each; once per Rust state): logs `native-build-stage1.log`, `native-build-stage1b.log`.

## Grow time per iteration, 700 kHz band-step (seconds, difference of the cumulative `grow=` in `P7_COMPACT_ITER`; `grow` includes `makeModel`)

| iteration | 1 | 4 | 8 | 12 | 16 | 17 |
|---|---|---|---|---|---|---|
| before, grow | 0.22 | 0.57 | 1.63 | 3.29 | 5.66 | 6.29 |
| before, grow - makeModel | 0.19 | 0.39 | 1.02 | 2.06 | 3.61 | 3.97 |
| after, grow | 0.24 | 0.45 | 0.94 | 1.61 | 2.71 | 2.77 |
| after, grow - makeModel | 0.20 | 0.25 | 0.33 | 0.34 | 0.50 | 0.48 |
| after, makeModel | 0.04 | 0.20 | 0.61 | 1.27 | 2.21 | 2.29 |

Cumulative `grow` at iteration 17: 42.07 s before, 21.21 s after; cumulative `makeModel` 15.24 s before, 15.53 s after. So the part of `grow` that is not `makeModel` (append, Gram, DC solve, charge action) went from 26.8 s to 5.7 s and grows roughly linearly (0.2 to 0.5 s). `makeModel` (the `congruence` pair sums for C, L, R, each k^2 over the full vectors, called inside every grow) is untouched and is still quadratic; it is now most of `grow`. Making it linear needs cached L/R/C pair sums with exact-reciprocity handling and is outside the files and step named; left open.
Before-data: `debugging/p7-r/h/stage06-sheet/run/stdout.txt`; actions-only attempt: `stage1-attempt1/stage1-700k/run/stdout.txt`; after: `stage1-700k/run/stdout.txt` (all in `debugging/p7-r/h/`).

## Stage 1 check, band-step (driver `run-p7-matrix.py::run_command` via a scratchpad script, not in the repo)

Env: `P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_ENRICH_MAX=64 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1 P7_MAGNETIC_PHASE_PROFILE=1`, 4096 MiB cap, 900 s cap, per-run cache directory (cold H-matrix cache), runs sequential. Command: `node --import tsx scripts/verify-p7-stage5-scenario.ts band-step --band-hz <hz>`. Logs: `debugging/p7-r/h/stage1-{100k,700k,1m}/run/stdout.txt`, `record.json`, `scenario-result.json`, `dsl-result.json`.

| band-step | status | grid iterations (0..n) | final KVL | wall | peak private / RSS |
|---|---|---|---|---|---|
| 100 kHz | model (DSL complete) | 7 (0..6) | 9.19e-4 (KCL 6.1e-4) | 135 s | 1123 / 814 MiB |
| 700 kHz | model | 18 (0..17) | 8.91e-4 (KCL 1.4e-4) | 165 s | 1102 / 836 MiB |
| 1 MHz | model | 24 (0..23) | 7.68e-4 (KCL 6.1e-4) | 361 s | 1325 / 1001 MiB |

Pass criteria: 100 kHz and 700 kHz produce a model with KVL <= 1e-3 (met), 100 kHz <= 19 iterations (met: 7). The 700 kHz KVL history is the same as stage 0.6 (8.9e-4 at iteration 17), as expected from numerically equivalent code. No run hit `MEMORY_LIMIT` or `FOIL_REDUCTION_RESIDUAL`; with limit 64 the admission line reads `reservedModes=256 modeCap=266 plannedBytes=1101674880` at 700 kHz. None of the three runs needed more than 24 iterations, so the 64 limit was not exercised past the first 24; the memory plan and the cap logic at 64 were exercised.

1 MHz: converged by KVL on the 24th grid iteration (iteration 23; 1.0e-3 at iteration 21 and 22, then 7.7e-4), i.e. 8 iterations beyond the 16 default and within the 24 of the continuation rule; it needed no more than 30 iterations and did not stall on KVL. Residual policy is the approximation tier (limit 1e-3). The 1 MHz KVL history falls 3-5x per 4 iterations, so step 1.1 is not triggered by this criterion (the plan's trigger: more than 30 iterations or no KVL convergence).
