# P7-R high band, step 1.1 (2026-11-12)

Physical accuracy of every model stays `unverified`. The KVL criterion and the stop rule `enrichment_continues` are unchanged. Nothing committed. Outcome: step 1.1 is implemented, but it does not make the 3 MHz band-step converge in two attempts, so per the plan fallback work stopped there (1 MHz and 100 kHz were not run, since 3 MHz gave no model; no GMRES on the full system was started).

## Code (`native/analyze/src/extraction/local/foil_reduced.rs` only)

- `P7_ENRICH_INDUCTIVE` (default off). In `grow_phases`, `drop = kvl / R` is replaced by the solution `d` of `(R + j*omega*L) d = r`, where `r` is the real/imaginary KVL residual pair of one drive and omega is the worst frequency. The real 2n system `[[R, -wL], [wL, R]]` is solved by right-preconditioned GMRES (`gmres`, preconditioner `R^-1`, no restart, true residual, plain f64 like `compact_cg.rs`). One application of the operator is one call of the existing `magnetic` batch with two columns (real and imaginary part of the vector); at most 30 applications, stop at a 100x residual drop. Unchanged afterwards: `dc.solve` on `divergence(d) - kcl`, `candidate = conductive(bubble) - d`.
- `1`: the pair of the worst drive (`grow`). `2`: also the balanced-drive branch (two drives, dominant phase of each, one GMRES per drive). Signature change: `grow` and `grow_phases` take the omega / drive-residual pair (the non-compact `train` passes none).
- Log per solve: `P7_ENRICH_INDUCTIVE currentModes=.. omega=.. actions=.. residualDrop=.. seconds=..`.
- Unit test `gmres_tests::solves_complex_symmetric_block_system`.

## Checks

- `cargo test --manifest-path native/analyze/Cargo.toml --lib foil_reduced`: 30 passed, 2 failed, both the known HEAD failures (`partitioned_plan_omits_only_unused_global_sparse_assembly`, `local_hierarchy_encloses_signed_sparse_action`).
- Coupon `diag_compact_kvl_100k_vs_1m` (`P7_MAGNETIC_PHASE_PROFILE=1 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4`; logs `debugging/p7-r/h/step11-coupon-{off,on}.log`):
  - switch off: history equal to `stage2-coupon-off.log` to the printed digits (100 kHz 1.1e-3 .. 1.0e-9 in 6 iterations; 1 MHz 1.1e-2 .. 1.1e-9 in 10); pass, 112 s.
  - `P7_ENRICH_INDUCTIVE=1` (attempt-1 code; the coupon was not repeated after the `2` branch was added): 100 kHz 1.1e-3, 2.3e-6, 2.2e-8, 1.0e-8, 2.2e-9, 1.0e-9 (better at iterations 1-2, equal end); 1 MHz 1.1e-2, 5.9e-4, 5.8e-5, 6.5e-6, 1.7e-6, 5.7e-7, 8.6e-8, 2.7e-8, 6.3e-9, 4.8e-9, 1.0e-9 (11 iterations, off: 10; better at 1, 2 and 6, worse at 3-5 and at the end). GMRES used 2 actions per solve there (drop 230-1000x). So "history no worse" holds at 100 kHz but not at 1 MHz of the coupon; time 136 s (off: 112 s).
- `npm run native:build`: 8 min 22 s (attempt 1), 8 min 14 s (attempt 2), and once more after the final source (switch values 1/2 split), `step11-native-build*.log`.

## 3 MHz band-step

Env: `P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_BARREL_SHEET=1 P7_MESH_MERGE=1 P7_ENRICH_MAX=64 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1 P7_MAGNETIC_PHASE_PROFILE=1 P7_ENRICH_INDUCTIVE=<1|2>`, cap 3584 MiB, 900 s, cold per-run cache, same driver as the stage-2 check. Baseline (stage-2 check, no switch): KVL 0.29, 0.43, 0.27, 0.24, 0.24, 0.20, 0.165, 0.12, 0.087, 0.071, 0.055, 0.051, 0.035, 0.024, 0.020, 0.026, 0.022; refused at iteration 16, 66 modes, 153 s.

| run | iterations | KVL first / middle (it 8) / last | KVL uncertainty (last) | status | wall | peak private / RSS | GMRES actions per solve |
|---|---|---|---|---|---|---|---|
| attempt 1, `=1` (`step11-3m-attempt1`) | 17 (0..16) | 0.289 / 0.0213 / 7.4e-3 | 6.1e-3 | refused `LOCAL_FOIL_REDUCTION_RESIDUAL`, KCL 1.4e-4, stop `enrichment_continues` (KVL 1.1e-2 at it 12, 7.4e-3 at it 16) | 165 s | 1041 / 711 MiB | 30 in each of the 5 solves at iterations 0-4 (residual drop only 2.4-3.2x, 2.3 s per solve, about 0.08 s per action); iterations 5-16 used the legacy rule (balanced-drive branch) |
| attempt 2, `=2` (`step11-3m-attempt2`) | 17 (0..16) | 0.289 / 0.0300 / 3.1e-2 | 6.1e-3 | refused, same code, KVL plateau about 3e-2 from iteration 8 | 219 s | 1041 / 771 MiB | 30 in all 27 solves, drop 2.5-3.2x |

Attempt 1 history: 0.289, 0.217, 0.445, 0.185, 0.072, 0.066, 0.058, 0.025, 0.021, 0.017, 0.018, 0.017, 0.011, 0.0095, 0.0083, 0.0074, 0.0074. Attempt 2: 0.289, 0.217, 0.445, 0.185, 0.072, 0.066, 0.046, 0.044, 0.030, 0.030, 0.029, 0.030, 0.031, 0.029, 0.030, 0.031, 0.031.

Reading: with the first four iterations (inductive pair) the KVL falls about 3x faster than the baseline in the middle of the history (0.021 against 0.087 at iteration 8) and ends 3x lower, but it still stalls at 1e-2 and the stop rule ends the run at iteration 16. GMRES with the `R^-1` preconditioner is weak here: 30 actions reduce the residual only 2.4-3.2x, which means omega*L/R is far from 1 and the diagonal preconditioner does not capture the inductive coupling (the system is dominated by the L block). Time was not the limit: an action costs 0.08 s in this run, 30 of them 2.3 s per solve; attempt 2 added 54 s (27 solves) to the run. A larger action cap or a better preconditioner (for example a block or sparsified L) is the open direction; both are outside the plan wording (30 actions, `R^-1`), so the decision is the user's.

## Five-board check

Not unblocked: the 3 MHz criterion of the stage-2 check is still not met.

## Logs

`debugging/p7-r/h/step11-coupon-{off,on}.log`, `step11-3m-attempt1/`, `step11-3m-attempt2/` (`run/stdout.txt`, `dsl-result.json`, `scenario-result.json`, `record.json`), `step11-attempt1-runs.log`, `step11-native-build*.log`.
