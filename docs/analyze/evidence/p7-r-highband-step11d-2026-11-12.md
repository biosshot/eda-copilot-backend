# P7-R step 1.1d: enrichment probe at 3 MHz (2026-11-12)

Physical accuracy of every model stays `unverified`. Nothing committed. Solver behaviour unchanged (probe only under `P7_ENRICH_PROBE=1`); the outer KVL history is the same as ladder stage C (final 1.5545e-2, KCL 7.84e-4, refused `LOCAL_FOIL_REDUCTION_RESIDUAL`).

## Pre-checks on the code before the edit
- `cargo test --manifest-path native/analyze/Cargo.toml --lib foil_reduced`: 30 passed, 2 failed, 4 ignored; the failures are exactly the two known HEAD ones (`partitioned_plan_omits_only_unused_global_sparse_assembly`, `local_hierarchy_encloses_signed_sparse_action`). No other failure.
- GMRES unit test `gmres_tests::solves_complex_symmetric_block_system`: ok.
- Coupon `diag_compact_kvl_100k_vs_1m -- --ignored --nocapture` (all P7 switches off): ok, 91.6 s. The 16 `P7_COMPACT_GRID_STEP` records (frequency, iteration, maxKclRelative, maxKvlRelative) are identical to `debugging/p7-r/h/stage2-coupon-off.log`: MATCH.

## Probe
Code: `foil_reduced.rs` only. `inductive_drops` now also returns the claimed reduction; `probe_apply_z` (one magnetic call, two actions); in `grow_phases`, only under `P7_ENRICH_PROBE=1` and a single-drive solve, one `P7_ENRICH_PROBE` line per GMRES solve after both phase candidates are built. Two extra magnetic calls per solve.
Definitions: `claimed` = final/initial residual reported by GMRES. `true2` / `trueMax` = `|r - Z x| / |r|` for the returned `x` (2-norm over re and im of all cells; max = largest per-cell modulus over largest per-cell modulus of `r`), `Z x` by one new magnetic action. `cand*`: the final candidates (phase 0 and 1, `-d + G(bubble)` with `G = R^-1 grad(bubble)`) taken as correction `-x_c`, `x_c = d - G`; `cand = |r - Z x_c - R*G| / |r|`, i.e. the potential mode is taken to cancel the resistive part `R*G` of the candidate current (the sign of the potential term is my reading, so the variant without it, `Z x_c` only, is printed too; they differ by under 5%). Both are single-step estimates; the real Galerkin solve picks its own coefficients.

Run: env `P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_BARREL_SHEET=1 P7_MESH_MERGE=1 P7_ENRICH_MAX=64 P7_ENRICH_INDUCTIVE=4 P7_ENRICH_PROBE=1 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1 P7_MAGNETIC_PHASE_PROFILE=1`, cap 3584 MiB, 900 s, cold cache, same driver as the ladder. One `npm run native:build` (6 min 54 s), one run: 167 s, peak private 1043 MiB, RSS 835 MiB. Row `it` = outer iteration (probe n is the solve of iteration n, modes = 2 + 4n); "outer KVL after" is the KVL printed for the next iteration (the effect of that enrichment).

| it | modes | claimed | true2 | trueMax | cand2 | candMax | cand2 (no potential term) | candMax (no potential term) | outer KVL after |
|---|---|---|---|---|---|---|---|---|---|
| 0 | 2 | 8.79e-03 | 8.79e-03 | 9.42e-03 | 8.40e-01 | 9.24e-01 | 8.44e-01 | 9.24e-01 | 2.09e-01 |
| 1 | 6 | 9.04e-03 | 9.04e-03 | 7.75e-03 | 7.96e-01 | 9.98e-01 | 8.08e-01 | 9.98e-01 | 2.30e-01 |
| 2 | 10 | 9.30e-03 | 9.30e-03 | 1.04e-02 | 9.91e-01 | 7.42e-01 | 9.99e-01 | 7.42e-01 | 1.76e-01 |
| 3 | 14 | 9.90e-03 | 9.90e-03 | 5.85e-03 | 1.09e+00 | 8.61e-01 | 1.10e+00 | 8.60e-01 | 1.64e-01 |
| 4 | 18 | 9.78e-03 | 9.78e-03 | 7.66e-03 | 1.10e+00 | 9.31e-01 | 1.10e+00 | 9.30e-01 | 1.48e-01 |
| 5 | 22 | 8.95e-03 | 8.95e-03 | 8.18e-03 | 1.17e+00 | 9.88e-01 | 1.17e+00 | 9.88e-01 | 1.20e-01 |
| 6 | 26 | 9.83e-03 | 9.83e-03 | 1.40e-02 | 1.08e+00 | 9.99e-01 | 1.09e+00 | 9.99e-01 | 9.52e-02 |
| 7 | 30 | 1.23e-02 | 1.23e-02 | 2.47e-02 | 9.26e-01 | 7.98e-01 | 9.38e-01 | 7.98e-01 | 7.17e-02 |
| 8 | 34 | 1.63e-02 | 1.63e-02 | 3.02e-02 | 9.84e-01 | 8.48e-01 | 9.97e-01 | 8.52e-01 | 5.06e-02 |
| 9 | 38 | 2.20e-02 | 2.20e-02 | 4.20e-02 | 1.01e+00 | 8.44e-01 | 1.03e+00 | 8.38e-01 | 3.52e-02 |
| 10 | 42 | 1.56e-02 | 1.56e-02 | 3.62e-02 | 1.16e+00 | 1.09e+00 | 1.18e+00 | 1.09e+00 | 2.78e-02 |
| 11 | 46 | 1.79e-02 | 1.79e-02 | 3.52e-02 | 1.37e+00 | 1.28e+00 | 1.38e+00 | 1.28e+00 | 2.25e-02 |
| 12 | 50 | 1.49e-02 | 1.49e-02 | 4.21e-02 | 1.28e+00 | 1.31e+00 | 1.29e+00 | 1.32e+00 | 1.87e-02 |
| 13 | 54 | 1.57e-02 | 1.57e-02 | 5.32e-02 | 1.30e+00 | 1.75e+00 | 1.31e+00 | 1.77e+00 | 1.63e-02 |
| 14 | 58 | 1.45e-02 | 1.45e-02 | 5.83e-02 | 1.41e+00 | 1.77e+00 | 1.42e+00 | 1.78e+00 | 1.62e-02 |
| 15 | 62 | 1.51e-02 | 1.51e-02 | 6.06e-02 | 1.32e+00 | 1.65e+00 | 1.33e+00 | 1.66e+00 | 1.55e-02 |

Outer KVL history (it 0..16): 0.289 0.209 0.23 0.176 0.164 0.148 0.12 0.0952 0.0717 0.0506 0.0352 0.0278 0.0225 0.0187 0.0163 0.0162 0.0155.

## Reading (against the plan's branch table)
- `true2 == claimed` to 12 digits at every solve: the GMRES claim is honest for its own vectors. Row 1 (true2 much worse than claimed) does not apply.
- `trueMax` is 0.6-1.0e-2 up to iteration 6, then grows to 2.5e-2...6.1e-2 while `true2` and `claimed` stay 1.2-2.2e-2 (it 7+) : from iteration 7 on the per-cell maximum is 2-4x the 2-norm ratio and is not falling.
- `cand2` and `candMax` are 0.7-1.8 at every iteration, from iteration 0: the final candidate (after `bubble`) does not reduce the residual at all, although the raw solution `x` reduces it about 100x in the 2-norm.
Closest row: row 3 (`true*` low, `cand*` not), clearly from iteration 0 to 6 where `trueMax` is also low; from iteration 7 the `trueMax` growth shows the row 2 pattern as well. Caveat: `cand*` is my single-step estimate of the candidate with the bubble, and that bubble correction is O(1) of the solution; a joint current/potential solver is what row 3 prescribes, so per the plan this is a point to stop and ask. Not acted on.

Logs: `debugging/p7-r/h/probe11d-3m/` (`run/stdout.txt`, `record.json`, `native-build.log`, `tests-foil_reduced.log`, `coupon-off.log`).
