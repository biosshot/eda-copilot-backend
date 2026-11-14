# P7-R high band, ladder after step 1.1 (2026-11-12)

Physical accuracy of every model stays `unverified`. Nothing committed. Outcome: stages A, B and C all fail the stage criterion at 3 MHz (model with KVL <= 1e-3 within 30 outer iterations). Per the plan the sweep (100 kHz to 10 MHz), the coupon check on a chosen mode and the five boards were NOT run; the ladder stops here for the user's decision. The mode `=2` was not developed.

Env of every run: `P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_BARREL_SHEET=1 P7_MESH_MERGE=1 P7_ENRICH_MAX=64 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1 P7_MAGNETIC_PHASE_PROFILE=1 P7_ENRICH_INDUCTIVE=<mode>`, cap 3584 MiB (Windows commit headroom, as in step 1.1), 900 s, cold per-run cache, same driver as step 1.1 (`run-p7-matrix.py::run_command`, `verify-p7-stage5-scenario.ts band-step --band-hz 3000000`), sequential, 4 cores. Each stage: one edit, one `npm run native:build` (6 min 55 s, 6 min 48 s, 6 min 50 s), one 3 MHz run.

## Code

`native/analyze/src/extraction/local/foil_reduced.rs`:
- `gmres` takes the preconditioner as a closure and returns the residual history per action (the unit test is adapted).
- A: `P7_ENRICH_GMRES_MAX` (1..=512, default 30); the log line `P7_ENRICH_INDUCTIVE` now carries `drop30/60/90/120` (residual drop after that many actions, `-` when the 100x stop came earlier).
- B: `P7_ENRICH_INDUCTIVE=3`: preconditioner `(R_j + j*omega*L_jj)^-1` applied as a 2x2 block on the (re, im) pair of each cell. `L_jj = sum over supports carrying the cell of 1e-7*|d|^2*I_ss`, `I_ss` = the support's own integral (prism: `pair_integral_compact(s,s)`, tetrahedron: `tetra_self_integral(.,16)`), method `CompactMixedMagnetic::self_inductance_diagonal` (21630 cells, 0.4 s, `P7_ENRICH_LDIAG` line).
- C: `P7_ENRICH_INDUCTIVE=4`: for each dense diagonal leaf of the planar H-matrix (<= 16 supports) the matrix `L_leaf[p][q] = 1e-7*sum (d_sp . d_tq) K(s,t)` in the cell space is assembled from the stored dense block, `[[R, -wL],[wL, R]]` is inverted once per solve (Gauss-Jordan, 2m x 2m, m <= 24) and applied block-wise; cells outside the H-matrix leaves (local tetrahedra) use stage B. 1024 blocks, 20640 cells covered, `P7_ENRICH_LBLOCKS` line.
- `4` and `3` behave like `1` otherwise (worst-drive pair only); `2` (balanced-drive branch) is now `== 2` only.
`native/analyze/src/extraction/local/planar_hmatrix.rs`: one read-only accessor `PlanarHMatrix::diagonal_blocks()` (dense diagonal leaf blocks and support order, no behaviour change); the plan allowed read access there, a private field needed this getter.

## Results, 3 MHz band-step

Baselines (same env): no switch, KVL 0.022 at iteration 16 (stage-2 check); step 1.1 `=1`, 30 actions, `R^-1`: 7.4e-3 (step 1.1 evidence).

| stage | mode / GMRES cap | GMRES residual drop per solve | KVL first / it 8 / last | iterations | status | wall | peak private / RSS |
|---|---|---|---|---|---|---|---|
| A (`ladderA-3m`) | `1`, cap 120 | 30 actions 2.4-4.7x, 60: 3.4-8.4x, 90: 5.4-14x, 120: 9.2-24x | 0.29 / 0.048 / 0.017 | 17 (0..16) | refused `LOCAL_FOIL_REDUCTION_RESIDUAL`, KCL 8.0e-4, KVL 1.69e-2 | 262 s | 1041 / 808 MiB |
| B (`ladderB-3m`) | `3`, cap 30 | 30 actions 11-25x | 0.29 / 0.071 / 0.016 | 17 | refused, KCL 7.1e-4, KVL 1.56e-2 | 165 s | 1039 / 734 MiB |
| C (`ladderC-3m`) | `4`, cap 30 | 100x reached in 23-30 actions for modes up to 30 (drop 100-114x), then 30 actions give 45-81x | 0.29 / 0.072 / 0.016 | 17 | refused, KCL 7.8e-4, KVL 1.55e-2 | 161 s | 1042 / 798 MiB |

Stage A, GMRES drop at 30/60/90/120 actions (R^-1 preconditioner), first solves: modes 2: 2.4 / 3.4 / 5.4 / 9.2; modes 6: 2.9 / 4.1 / 6.4 / 10.8; modes 10: 4.0 / 6.3 / 11.0 / 18.6; modes 22: 4.7 / 8.4 / 14.1 / 24.0; last solve (modes 62): 3.6 / 5.7 / 9.0 / 15.6. Per solve 7.4-9.0 s (0.065 s per action); the whole run 262 s.

KVL histories (it 0..16):
- A: 0.29 0.11 0.26 0.17 0.15 0.14 0.11 0.077 0.048 0.033 0.024 0.018 0.015 0.016 0.016 0.017 0.017
- B: 0.29 0.26 0.21 0.17 0.16 0.14 0.12 0.098 0.071 0.045 0.040 0.030 0.026 0.026 0.022 0.019 0.016
- C: 0.29 0.21 0.23 0.18 0.16 0.15 0.12 0.095 0.072 0.051 0.035 0.028 0.023 0.019 0.016 0.016 0.016
KVL uncertainty 6.0-6.1e-3 from iteration 2 on in all three.

## Reading

The criterion is not met by any stage. The inner solve is not the limit: from B on the GMRES drop is 11-25x in 30 actions and with C it reaches 100x in 23-30 actions (the target of the solve), yet the outer KVL history is nearly identical for A, B and C (about 1.5-1.7e-2 at iteration 16, plateau 0.015-0.017 in the last 3-4 iterations). Better solves of `(R + j*omega*L) d = r` do not turn into a faster outer drop: the enrichment adds 4 modes per iteration (66 current modes of 21630 cells at the end) and the stop rule `enrichment_continues` (KVL must fall 2x over the last 4 iterations; unchanged) ends the run at iteration 16, with KVL 0.016 against the 1e-3 target. The limit therefore looks like the enrichment direction / basis growth, not the preconditioner. This is a reading, not isolated: the KVL still falls at 0.016-0.019 at the end of B and C, so a longer limit might still help, but that would change the stop rule or the iteration budget (not allowed in this ladder). The decision is the user's.

Not run: coupon check (`diag_compact_kvl_100k_vs_1m`) and `cargo test --lib foil_reduced` on the final code (no stage passed; only `cargo check --lib --tests` and the build were run, both clean; the gmres unit test was run at stage B before the closure change and not repeated), the switch-off bit-identity check, the sweep and the five boards. `P7_ENRICH_INDUCTIVE=0` code path is unchanged apart from the closure signature of the private `gmres`, used only by the switch.

Logs: `debugging/p7-r/h/ladderA-3m/`, `ladderB-3m/`, `ladderC-3m/` (`run/stdout.txt`, `dsl-result.json`, `scenario-result.json`, `record.json`), `ladder-{A,B,C}-build.log`.
