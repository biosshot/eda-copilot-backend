# P7-R step 1.1e: projected inductive GMRES, `P7_ENRICH_INDUCTIVE=5` (2026-11-12)

Physical accuracy of every model stays `unverified`. Nothing committed. Code: `native/analyze/src/extraction/local/foil_reduced.rs` only. Switch off by default; modes 0-4 unchanged (the preconditioner construction of modes 3/4 moved unchanged into `inductive_precond`).

## Method
`P x = x - G(dc(div x))`, `i_p = -G(dc(kcl))`, operator `w -> P R^-1 Z P w`, rhs `-P R^-1 (r + Z i_p)`, candidate `i_p + P w` (no DC bubble), potential mode `psi = -dc(div(R^-1 (r + Z i)))`. Right preconditioner: `y -> P(Z_block^-1 (R y))`, with the stage-C leaf blocks (the plan says "blocks of stage C, then P"; because the operator carries `R^-1`, the block inverse of `Z` is applied to `R y`, my reading). Solves with GMRES target 1e-2 (100x), cap `P7_ENRICH_GMRES_MAX=120`. Probe line (`P7_ENRICH_PROBE=1`): `cand2`/`candMax` = `|P R^-1 (r + Z i)|` against the GMRES right-hand side (same norm as `claimed`), `phys2`/`physMax` = `|R P R^-1 (r + Z i)| / |r|` (voltage form), `raw2` = `|r + Z i| / |r|` without a potential mode. Sign of `psi` does not change the basis (a basis vector).

## Probe: cand2 vs claimed (3 MHz, attempt 1, no fix needed)
`cand2 == claimed` to 12 digits at every solve (e.g. 8.692447401930302e-3 vs 8.692447401930939e-3; 8.7508e-3; 9.2382e-3; 9.3783e-3). No sign or projector bug.

## 3 MHz band-step (`P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_BARREL_SHEET=1 P7_MESH_MERGE=1 P7_ENRICH_MAX=64 P7_ENRICH_INDUCTIVE=5 P7_ENRICH_PROBE=1 P7_ENRICH_GMRES_MAX=120 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1 P7_MAGNETIC_PHASE_PROFILE=1`, cap 3584 MiB, 900 s, driver as the ladder; one `npm run native:build` 8 min 4 s; one run)
- Status: `model`, dslStatus complete; 149.6 s, peak private 1043 MiB, RSS 802 MiB.
- KVL history (it 0..4): 0.289, 0.157, 5.67e-2, 2.72e-3, 2.57e-4. KCL 1.3e-3 .. 6.1e-7. KVL uncertainty 6.04e-3 (unchanged operator accuracy; the plan leaves acceptance against it to the user). 18 current modes. 5 iterations (criterion: <= 30).
- GMRES residual drop at 30 / 60 / 120 actions: not reached, the 100x stop came earlier. Solves: 21, 20, 18, 27 actions for drops 115, 114, 108, 107 (modes 2, 6, 10, 14). The drops at 30/60/120 print as `-`.
- Cost per action 0.079-0.081 s (one magnetic call, two Cholesky solves per phase for each of the operator's `P`s and the preconditioner `P`); per solve 1.4-2.2 s GMRES plus two extra magnetic calls.
- Probe table (modes: claimed / candMax / phys2 / physMax / raw2): 2: 8.69e-3 / 7.1e-3 / 1.55e-2 / 9.1e-3 / 0.38; 6: 8.75e-3 / 6.5e-3 / 1.09e-2 / 6.4e-3 / 0.77; 10: 9.24e-3 / 1.6e-2 / 5.0e-3 / 4.4e-3 / 0.77; 14: 9.38e-3 / 1.1e-2 / 1.8e-2 / 6.5e-2 / 0.69.
- Branch of the plan table: row 1 (model at 3 MHz, KVL <= 1e-3, <= 30 iterations).

## Coupon `diag_compact_kvl_100k_vs_1m` (env `P7_MAGNETIC_PHASE_PROFILE=1 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4`)
- Mode 5: 100 kHz 4 iterations (off: 6; KVL 1.1e-3, 6.1e-8, 3.9e-9, 1.0e-9), 1 MHz 5 iterations (off: 10; 1.1e-2, 2.7e-5, 1.1e-6, 2.3e-8, 1.2e-9). Criterion (previous + 2): met. 90.3 s.
- Switches off: all 16 `P7_COMPACT_GRID_STEP` records (minus timing) identical to `debugging/p7-r/h/stage2-coupon-off.log`.
- `cargo test --lib foil_reduced`: 30 passed, 2 failed (the two known: `partitioned_plan_omits_only_unused_global_sparse_assembly`, `local_hierarchy_encloses_signed_sparse_action`), 4 ignored.

## Sweep (same switches, mode 5, one band-step each, sequential, 4 cores, cap 3584 MiB)
| band | status | KVL it 0 .. last | iterations | KVL uncertainty | modes | wall s | peak private MiB |
|---|---|---|---|---|---|---|---|
| 100 kHz | model | 8.7e-2 .. 5.5e-4 | 8 | 2.8e-4 | 30 | 138.7 | 1142 |
| 700 kHz | model | 0.29 .. 9.2e-4 | 13 | 1.7e-3 | 50 | 148.3 | 1039 |
| 1 MHz | model | 0.29 .. 1.0e-3 | 8 | 2.4e-3 | 30 | 144.6 | 1041 |
| 3 MHz | model | 0.29 .. 2.6e-4 | 5 | 6.0e-3 | 18 | 149.6 | 1043 |
| 10 MHz (record only) | killed, `process-tree private commit cap exceeded` at 3584 MiB, 889 s, in the H-matrix build (122505 of 1225058 blocks at 264 s; layered mesh at t/delta > 1) | | | | | 889 | 3584 |

(The 1 MHz last KVL prints as 1.0e-3; the run returned a model.) Logs: `debugging/p7-r/h/step11e-*` (`step11e-3m-a1`, `step11e-sweep-*`, `step11e-coupon-{on,off}.log`, `step11e-tests-foil_reduced.log`, `step11e-build.log`).
