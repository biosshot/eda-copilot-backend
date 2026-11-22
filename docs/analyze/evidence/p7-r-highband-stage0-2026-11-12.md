# P7-R high band, stage 0: diagnostics and cheap loop (2026-11-12)

Physical accuracy of every model stays `unverified`. Developer switches only; solver numerics are unchanged.

## Core cap

The `threads: 11` in the 700 kHz log is the planar H-matrix pool: `Params::from_env` in `planar_hmatrix.rs` defaults to `available_parallelism - 1` (clamped 1..11). The other pools are already capped at 4 in code (`parallel.rs`, `P7_COMPACT_PL_THREADS`, `P7_MAGNETIC_NEAR_THREADS`). The variable that works is **`P7_HMATRIX_THREADS=4`** (also exported `RAYON_NUM_THREADS=4` for the global rayon/faer pool). Verified: `P7_COMPACT_HMATRIX` now prints `"threads":4`. `debugging/p7-r/q1/band1m-sheet` was run without it. H-matrix build at 4 threads: 117 s (earlier run with 11 threads: 165 s, other load on the machine differs).

## Changes

- 0.1 `foil_reduced.rs`: `full_residual_with_shape` records the row of the largest KVL numerator; the grid step log has `worstKvlRow` {cell, phase, frequencyHz, input, residual, resistanceTerm (R*q), gradientTerm, omegaLTerm, cellResistanceOhm, owner {sheet|barrel|volume ...}}. Owner is derived from `MixedMap` public fields (foils `first_current..+2`, barrels `mode`, else volume); no edit to `mixed_conductor.rs`.
- 0.2 `planar_hmatrix.rs`: `Stats.dense_bytes`, `Stats.low_rank_bytes` (also `threads`, `hmatrixCache`) in `P7_COMPACT_HMATRIX`.
- 0.3 `planar_hmatrix.rs`: opt-in disk cache `P7_HMATRIX_CACHE=1`, dir `P7_HMATRIX_CACHE_DIR` or `$PCB_ANALYSIS_CACHE_DIR/hmatrix`. Key = 128-bit hash of every prism coordinate, theta/eps/leaf, kernel-relevant `P7_*` variables (neutral ones such as `P7_ENRICH*`, thread counts, `P7_FOIL_SHEET` excluded) and `KERNEL_VERSION` (bump when the pair kernel changes; a rebuild of the addon alone does not invalidate). A truncated, wrong-size or foreign file is rejected and rebuilt.

`--mode restart` check (code reading): `restart` reuses only the TypeScript model-result cache (`PCB_ANALYSIS_CACHE_DIR`), so it hits only after a run that returned a model; the H-matrix was never persisted. A refused run such as 700 kHz stores no result and rebuilds the H-matrix. Also `s5/band-step@700000` is not a matrix item (the manifest lists other bands), so the plan's `run-p7-matrix.py --rows s5/band-step@700000` selects 0 items; the 700 kHz runs used the same command line as the matrix (`verify-p7-stage5-scenario.ts band-step --band-hz 700000`) through `run_command` of the runner (4096 MiB tree cap, 900 s cap), driver in the session scratchpad.

## Checks

- `cargo test --lib planar_hmatrix`: pass (new round trip, truncation, key test).
- `cargo test --lib foil_reduced`: 29 pass, 2 fail (`partitioned_plan_omits_only_unused_global_sparse_assembly`, `local_hierarchy_encloses_signed_sparse_action`); the same two fail on the unmodified tree (stashed and re-run), so they are not caused by this change.
- Coupon `diag_compact_kvl_100k_vs_1m` (4 threads): pass, 94 s; 100 kHz KVL 1.1e-3, 7.5e-6, 6.2e-8, 5.5e-9, 4.2e-9, 1.0e-9 (6 iterations); 1 MHz 1.1e-2, 7.2e-4, 5.5e-5, 2.2e-6, ... converges as before. Log `debugging/p7-r/h/coupon-stage0.log`.
- `npm run native:build` once (7 min).

## 0.4 700 kHz medium run (`P7_FOIL_SHEET=1`, `P7_HMATRIX_THREADS=4`, `RAYON_NUM_THREADS=4`)

Cold: refusal as before, 170 s, 1101 MiB peak (`debugging/p7-r/h/stage0-cold/run/stdout.txt`, `record.json`). Repeat with the H-matrix cache: 55 s, 567 MiB, `magnetic-prepare` 0.8 s (cache hit, load 0.23 s; was 117 s), identical KVL history (`debugging/p7-r/h/stage0-repeat/`). Cache file 213 MB.

H-matrix bytes: dense 95,623,688 + low-rank 113,832,816 = 209,456,504 (sum equals `storedBytes`). 12077 supports, 5801 far / 60883 near blocks, 390,703 unconverged pairs, max relative change 0.097.

Worst KVL row (max numerator, both phases, both drives):

| iter | maxKvl | cell | owner | R*q | gradient | omegaL*q | cell R, ohm | residual |
|---:|---:|---:|---|---:|---:|---:|---:|---:|
| 0 | 0.289 | 7831 | sheet 3915, TOP, triangle 8959, axis 1 | 1.2e-8 | -9.2e-5 | 6.6e-6 | 4.9e-4 | 8.6e-5 |
| 2 | 1.0 | 22001 | barrel 2141 `hole:pad:cd8747d128aec8e3e16` | 1.26e-2 | 2.2e-4 | -8.0e-8 | 32947 | 1.28e-2 |
| 16 | 0.997 | 22001 | the same barrel | 2.04e-4 | 1.0e-6 | 2.4e-6 | 32947 | 2.07e-4 |

Reading: at iteration 0 the worst row is a sheet cell with R*q ~ 0 and the gradient unbalanced, i.e. no current in the basis on that copper. From iteration 2 through 16 the same barrel cell (R = 32.9 kohm, about 7e7 times a sheet cell) dominates; there R*q is nonzero and the residual is R*q minus a gradient 60x smaller, so the cell carries too much current for its potential drop. Because this row also sets the scale (`kvl_scale` is the maximum term over cells), maxKvl is ~1. The 33 kohm value is suspect (a pad-hole barrel segment with a very small wall area) and is worth checking in the mixed map before the enrichment step.
