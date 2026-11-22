# P7-R high band, stage 3 steps 3.1-3.2 (2026-11-13)

Physical accuracy of every number here stays `unverified`. Step 3.3, option (a) (per-frequency multiplier) and the validator for t/delta > 1 were not touched. Decision by the user 2026-11-13: option (b), one multiplier per band.

## 3.1 code (`P7_FOIL_SHEET=2`, default off)

Per-frequency application was not possible by a small edit: R is projected into one reduced `Model.resistance_ohm` (`congruence`) and reused at every grid frequency (`foil_reduced.rs`). So the multiplier is evaluated once at the band upper frequency (`max_frequency_hz` of `prepare_model`), as `P7_BARREL_SHEET=2` already did.

- `copper_skin_profile.rs`: `P7_FOIL_SHEET` is a mode (1 as before, 2 adds `foil_sheet_scales_resistance()`); `foil_sheet_admits` accepts any t/delta in mode 2.
- `mixed_conductor.rs`: mode 2 lets the foil-column admission (`thickness <= delta`) and the hybrid thickness screen pass at any t/delta; `MixedMap` gets `foil_resistance_scale` and `barrel_resistance_scale` (per prism, 1 when off); the sheet cell resistance is multiplied by `foil_resistance_ratio(t/delta)` with t the foil z span and delta at the band top; walls keep the `P7_BARREL_SHEET=2` factor, now via the same vector. Report: `provenance.compactOperator.foilSheet` has `resistanceScaled`, `resistanceMultiplierMin/Max`, `barrelResistanceMultiplierMax`, `multiplierTag` ("approximation, one multiplier per band ...") and `fieldApproximation` (one-sided field: for a plane between two signal layers this is an approximation); `copperSkinMesh.barrelSheet` has `multiplierTag` and `resistanceMultiplierMax`.
- `spatial_recovery.rs`: foil and barrel power use `sigma / scale`, the same multiplier as the model R (with the switches off the scale is exactly 1, bit identical). This is what stops `LOCAL_SPATIAL_RECOVERY` with `P7_BARREL_SHEET=2`; not run on the via coupon with `=2` in this session (the coupon run below has the switches off).
- Not done: the 10 MHz / 40 MHz band-steps on the TPS63060DSCR board (same mesh as 10 kHz), not requested in the option (b) instruction. No new board-level numbers, so mesh counts of the board at 10 kHz, 10 MHz and 40 MHz were not measured. On the microstrip coupon the one-sheet mesh is identical at all three frequencies: 174 copper cells, 420 nodes, 0 barrels (`stage3-ms-sheet-*`).

## Checks

- `cargo test --lib mixed_conductor` (`stage3-test-mixed.log`): 19 pass, 2 fail = the two known HEAD failures (`thin_wall_barrel_potentials_match_the_electric_field_trace`, `imported_via_two_drives_and_missing_contact`). `copper_skin_profile` 4 pass, `spatial_recovery` 1 pass. `foil_reduced` not rerun (not changed).
- Coupon `diag_compact_kvl_100k_vs_1m -- --ignored --nocapture`, switches off (`stage3-coupon-off.log`): pass, 101.7 s. The 16 `P7_COMPACT_GRID_STEP` lines with timing fields removed have the same md5 as `stage2-coupon-off.log` (`4e720c0c59bb39a136f47614eebda77a`): KVL history bit-identical.
- `npm run native:build` once (`stage3-native-build.log`).

## 3.2 microstrip coupon against the 2D cross-section

Test: `native/analyze/src/extraction/local/microstrip_reference_test.rs`, `diag_microstrip_r_l_against_cross_section` (ignored, env-driven, one frequency and one length per run). Board: `tests/fixtures/analysis-segmented-coupon.json` with 35 um copper (t/delta 0.53, 1.68, 3.35 at 1, 10, 40 MHz), 0.2 mm substrate (eps_r 4), signal track 0.1 mm on TOP, finite return track 0.5 mm on BOTTOM, length 5 or 10 mm, 3D ports at both ends, far end shorted in post-processing: `Z = 50 (1+G)/(1-G)`, `G = S11 - S12 S21/(1+S22)`; model R' = Re Z / length, L' = Im Z / (2 pi f length). Both conductors are included in R (signal plus return). 2D reference: `board_section::slice` at mid-length and `polygon_section::solve` (finite physical return, PEC): external L'.

Switches (`/tmp` runner, 4 threads, one run at a time): `P7_BARREL_NOISE=1 P7_BARREL_SHEET=2 P7_ENRICH_MAX=64 P7_ENRICH_INDUCTIVE=5 P7_ENRICH_GMRES_MAX=120 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1`, no `P7_MESH_MERGE`; one-sheet runs add `P7_FOIL_SHEET=2`. Logs: `debugging/p7-r/h/stage3-ms-{sheet,layered}-<Hz>-L<mm>.log`.

One-sheet model (`P7_FOIL_SHEET=2`), multiplier from the report:

| f | t/delta | multiplier | iterations / KVL | R' 5 mm | R' 10 mm | R' differential | L' 5 mm | L' 10 mm | L' differential | 2D external L' |
|---|---|---|---|---|---|---|---|---|---|---|
| 1 MHz | 0.53 | 1.0070 | 0 / 1.8e-9 | 5.724 | 5.856 | 5.987 Ohm/m | 563.5 | 570.5 | 577.5 nH/m | 510.4 nH/m |
| 10 MHz | 1.68 | 1.5407 | 0 / 6.7e-9 | 8.864 | 9.307 | 9.750 | 560.2 | 561.4 | 562.7 | 510.4 |
| 40 MHz | 3.35 | 3.3605 | 0 / 3.3e-10 | 19.404 | 20.542 | 21.679 | 559.5 | 559.1 | 558.6 | 510.4 |

Each run 5-7 s, 174 cells, 420 nodes. The 10 mm and differential values are given because end effects are present in single-length values; the differential of the 5 and 10 mm runs removes constant end terms.

Layered reference (no `P7_FOIL_SHEET`, per-layer skin mesh):

- 1 MHz (10 mm and 5 mm) and 10 MHz (5 mm): refused `COUPLED_PSD: L: nonpassive/indefinite` before the first iteration (minimum normalized eigenvalue -6.2e-3 at 10 mm, -2.1e-4 at 5 mm and 1 MHz, -2.4e-4 at 10 MHz, 12 modes; 10 MHz took 144 s). A smaller triangle area target (`MICROSTRIP_AREA_M2`) gave the identical eigenvalue and default enrichment switches gave the same refusal, so these were not pursued further. No layered R or L exists at 1 and 10 MHz.
- 40 MHz, 5 mm: model returned, 2958 cells, 2660 nodes, 572 s (H-matrix build 562 s), 3 grid steps, KVL 7.2e-4, KCL 7.1e-5, compact KVL uncertainty 1.1e-8, R' 8.426 Ohm/m, L' 572.9 nH/m. Memory within the 2048 MiB model budget (peak private not measured; run well under 4 GB budget).

Discrepancy (model minus reference, in %, `unverified`):

| f | R one-sheet vs layered | L one-sheet vs layered | L one-sheet vs 2D external |
|---|---|---|---|
| 1 MHz | no layered reference (PSD refusal) | no layered reference | +10.4% (5 mm), +11.8% (10 mm), +13.1% (differential) |
| 10 MHz | no layered reference (PSD refusal) | no layered reference | +9.8% (5 mm), +10.0% (10 mm), +10.2% (differential) |
| 40 MHz | +130.3% (19.404 vs 8.426, 5 mm single length) | -2.3% (559.5 vs 572.9, 5 mm) | +9.6% (5 mm), +9.5% (10 mm), +9.4% (differential) |

Reading, not acceptance:

- R at 40 MHz is over the 10% trigger of plan step 3.3 by a wide margin. Without the multiplier the same sheet solve would give about 19.40 / 3.36 = 5.77 Ohm/m (-31% against the layered 8.43), so the one-sheet resistance is between the unscaled sheet and the layered mesh only by the multiplier choice; a multiplier of about 1.46 would match, not 3.36. The one-sided multiplier applied on top of lateral crowding already present in the sheet model is a candidate cause (inferred, not isolated). The layered 40 MHz number itself has KVL 7.2e-4 and is a single mesh.
- L one-sheet against the 2D external (PEC, no internal inductance, no ends) is +9.4 to +13% at all three frequencies, almost independent of length (differential close to the 10 mm value), so the gap is not an end effect of the shorted coupon. The 2D loss-extended solve (total L including internal) was refused at 1, 10 and 40 MHz (`SECTION_SKIN_DEPTH: delta does not fit thickness/10`), so the internal inductance part of the 2D comparison is missing. The one-sheet L matches the layered L at 40 MHz within 2.3%.
- The plan's step 2.4 trigger "L discrepancy against the 2D section above 2%" is met by the 2D external comparison (about +10%), with the caveat above (the 2D side has no internal inductance, the 3D side has a 0.2 mm substrate gap and finite 3D ends).
- The validator for t/delta > 1 was not changed.

## 3.3a two-sided reading (`P7_FOIL_SHEET=3`, 2026-11-13)

Physical accuracy stays `unverified`. Same files as 3.1. `P7_FOIL_SHEET=3` is the same single sheet as `=2`, with multiplier `foil_resistance_ratio(x/2)` (x = t/delta at the band top) in the sheet cell resistance and in `spatial_recovery.rs` (same `foil_resistance_scale`). Default off. The model report (`compactOperator.foilSheet`) records both values: `multiplierOneSidedRatioX` = ratio(x), `multiplierTwoSidedRatioHalfX` = ratio(x/2), `twoSidedField`, and the applied `resistanceMultiplierMin/Max`. At 40 MHz (x = 3.350): ratio(x) = 3.3605, ratio(x/2) = 1.5407.

Checks: switches-off coupon `diag_compact_kvl_100k_vs_1m` (`stage3a-coupon-off.log`): pass, GRID_STEP history md5 `4e720c0c59bb39a136f47614eebda77a` again equal to `stage2-coupon-off.log` (bit-identical). `npm run native:build` once (`stage3a-native-build.log`). Not committed; validator unchanged; option (a) and 3.3 not started.

Runs (40 MHz, 5 mm, same switch set as 3.2, 4 threads, one at a time; logs `debugging/p7-r/h/stage3a-*`). Coupon A = 0.1 mm signal over 0.5 mm finite return, 0.2 mm substrate. Coupon B (new, `MICROSTRIP_KIND=plane`) = 1 mm signal over a 4 mm wide solid return track, 0.1 mm substrate, 35 um Cu, 2D section at mid-length.

| coupon | mode | R Ohm/m | R vs layered | L nH/m | L vs layered | L vs 2D external | cells | seconds |
|---|---|---|---|---|---|---|---|---|
| A | layered (3.2 run) | 8.426 | - | 572.9 | - | +12.2% | 2958 | 572 |
| A | =2 (ratio(x) 3.3605) | 19.404 | +130.3% | 559.5 | -2.3% | +9.6% | 174 | 7 |
| A | =3 (ratio(x/2) 1.5407) | 8.917 | +5.8% | 559.1 | -2.4% | +9.4% (2D 511.1) | 174 | 5 |
| B | layered | not obtained: 900 s limit hit in the H-matrix build (1736 of 4344 blocks at 548 s), process stopped, exit 124 | - | - | - | - | - | >900 |
| B | =2 (3.3605) | 2.677 | n/a | 109.60 | n/a | +17.2% (2D 93.50) | 312 | 22 |
| B | =3 (1.5407) | 1.229 | n/a | 109.52 | n/a | +17.1% | 312 | 22 |

KVL on all sheet runs 1e-9 to 6e-9 at iteration 0. The 2D loss-extended solve (internal L, R) was refused again on both coupons (delta does not fit thickness/10), so 2D external (PEC) L is the only 2D value. On coupon B only the one-sheet R and L exist; R changes by the factor 2.18 between =2 and =3 (the ratio of the two multipliers), L by 0.07%. For coupon A, =3 is within 6% of the layered R at this one frequency and one mesh; this is a single point, not a general statement. Choice of plan branch left to the user.

## 3.3b coupon B' mesh check (2026-11-13) - stopped, no solver run

Test-only change in `microstrip_reference_test.rs`: env `MICROSTRIP_PLANE_MM`, `MICROSTRIP_WALL_S`, `MICROSTRIP_MESH_ONLY` (prints `MICROSTRIP_MESH cells=... nodes=...` after the mesh build and returns). Coupon B' = 1 mm trace, 0.1 mm substrate, 2 mm long, 3 mm wide plane, 40 MHz, 35 um Cu. Mesh-only checks (`debugging/p7-r/h/stage3b-meshcheck-*.log`): layered mesh 5304 copper cells (2584 nodes), one-sheet mesh 312 cells (408 nodes). The condition "layered at most 2958 cells (coupon A)" fails (5304 > 2958), so per the instruction no layered, `=2` or `=3` run was made. No R or L for B'. Behaviour of the code unchanged.

### 3.3b follow-up: B' at 1 mm length (2026-11-13) - stopped, no solver run

Mesh-only check (`debugging/p7-r/h/stage3b-meshcheck1-*.log`, `stage3b-meshcheck-L5.log`, `-L0.5.log`): layered copper cells for the 3 mm plane coupon are 5304 at lengths 0.5, 1, 2 and 5 mm (nodes 2736, 2584 (2 mm), 2432 at 0.5, 2, 5 mm), so shortening the coupon does not reduce the layered cell count; the one-sheet mesh is 312 cells. The user's condition (layered at most 3300 cells) fails, so no layered, `=2` or `=3` run was made. Only the test file changed.

### 3.3b result: coupon B' with the cell limit lifted (2026-11-13)

Coupon B': 1 mm trace over a 3 mm wide return track, 0.1 mm substrate, 2 mm long, 35 um Cu, 40 MHz (t/delta 3.35), final switch set, 4 threads, one run at a time. Logs `debugging/p7-r/h/stage3b-plane2-{layered,s2,s3}.log`. Full-coupon values are the shorted-far-end input impedance (R = Re Z, L = Im Z / omega). Per-metre values on a 2 mm coupon are not representative (end terms dominate); the L-vs-2D column uses L/length against the 2D external (PEC) L of the section, 93.77 / 93.50 nH/m (the two differ by the 2D mesh/ROI only).

| mode | R (mOhm) | R vs layered | L (nH) | L vs layered | L/length vs 2D external | time | cells |
|---|---|---|---|---|---|---|---|
| layered | 3.535 | - | 0.1808 | - | -3.6% | 1499 s (H-matrix 1465 s to the local stage; 3900 blocks) | 5304 |
| =2 (ratio(x) 3.3605) | 5.02 | +42.0% | 0.1919 | +6.2% | +2.6% | 24 s | 312 |
| =3 (ratio(x/2) 1.5407) | 2.31 | -34.7% | 0.1917 | +6.0% | +2.5% | 24 s | 312 |

Layered run: finished within the 3600 s limit, 4 grid steps, KVL 2.8e-4. Peak memory: not measured exactly; a 5 s sampler of the test process working set peaked at 71 MiB (planned bytes about 68 MiB), far under the 4 GB cap. R on a 2 mm, 3.5 mOhm coupon is small and includes end/pad terms; one mesh, one frequency, physics `unverified`. No branch chosen.

## 3.3b.1-3.3b.2 two half-sheets (`P7_FOIL_SHEET=4`, 2026-11-13)

Physical accuracy stays `unverified`. Default off. Validator, option (a), step 2.4 and via walls (`P7_BARREL_SHEET` behaviour) not touched.

Code (`copper_skin_profile.rs`, `mixed_conductor.rs`; `spatial_recovery.rs` not edited because it already divides sigma by `map.foil_resistance_scale`, the same per-prism vector, so it follows the new multiplier): in `Profile::pieces` a layer with t/delta > 1 at the band top returns exactly 2 slabs (existing stratified/layered path; foil prisms are the two half slabs); t/delta <= 1 follows the `=1` branch unchanged. `foil_sheet_admits` stays x <= 1 only in mode 4. `foil_resistance_scale` per prism = `foil_resistance_ratio((t/2)/delta)` at `max_frequency_hz` when the whole layer has t/delta > 1, else 1. Report `compactOperator.foilSheet`: `halfSheets: true`, `halfSheetLayers: 2`, `multiplierTag` ("approximation, two half-sheets ... one multiplier per band"), `criterion`, `fieldApproximation`, `resistanceMultiplierMin/Max`. At 40 MHz: x = t/delta 3.350, half x 1.675, multiplier 1.5407 on each half.

Checks: `npm run native:build` once (`stage3c-native-build.log`, 8.5 min). Switches-off coupon `diag_compact_kvl_100k_vs_1m` (`stage3c-coupon-off.log`, 153.8 s, pass): the 16 `maxKvlRelative` values are identical to `stage2-coupon-off.log` (same md5 of the extracted values; the GRID_STEP lines with timing removed are also identical). Focused tests (`stage3c-tests.log`): `copper_skin_profile` 4 pass, `spatial_recovery` 1 pass; `foil_reduced` 30 pass, 2 fail = the two known (`partitioned_plan_omits_only_unused_global_sparse_assembly`, `local_hierarchy_encloses_signed_sparse_action`); `mixed_conductor` 17 pass, 4 fail = the two known plus `hybrid_via_without_mesh_slivers_solves_with_thin_wall_barrel` and `thin_wall_barrel_self_inductance_matches_shell_partial_inductance`, which failed with `LOCAL_DEADLINE` under parallel test load and passed alone with `--test-threads=1` (`stage3c-tests-rerun.log`). After the main runs only two report strings (`criterion`, `fieldApproximation`) were edited and the test binary rebuilt; `npm run native:build` was not repeated.

Runs (40 MHz, final switch set of 3.2 plus `P7_FOIL_SHEET=4`, `P7_BARREL_NOISE=1 P7_BARREL_SHEET=2 P7_ENRICH_INDUCTIVE=5 P7_ENRICH_GMRES_MAX=120 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1`, 4 threads, one at a time, layered mesh not recomputed, references from 3.2 / 3.3b). Command: `cargo test --manifest-path native/analyze/Cargo.toml --lib diag_microstrip_r_l_against_cross_section -- --ignored --nocapture` with `MICROSTRIP_HZ=4e7` and, for B', `MICROSTRIP_KIND=plane MICROSTRIP_PLANE_MM=3 MICROSTRIP_LEN_MM=2`. Logs `debugging/p7-r/h/stage3c-{A-s4-40M-L5,Bp-s4-40M-L2}.log` and `.peak` (peak working set of the test process, sampled every 3 s).

| coupon | mode | R | R vs reference | L | L vs reference | cells | time | peak memory |
|---|---|---|---|---|---|---|---|---|
| A (5 mm) | layered reference | 8.426 Ohm/m | - | 572.9 nH/m | - | 2958 | 572 s | not measured |
| A | =2 (ratio(x) 3.3605) | 19.404 Ohm/m | +130% | 559.5 nH/m | -2.3% | 174 | 7 s | - |
| A | =3 (ratio(x/2) 1.5407) | 8.917 Ohm/m | +5.8% | 559.1 nH/m | -2.4% | 174 | 5 s | - |
| A | =4 (two halves, 1.5407 each) | 9.744 Ohm/m | +15.6% | 567.1 nH/m | -1.0% | 348 | 25 s solve, 36 s test | 42.5 MiB |
| B' (2 mm, 3 mm plane) | layered reference | 3.535 mOhm | - | 0.1808 nH | - | 5304 | 1499 s | 71 MiB |
| B' | =2 | 5.02 mOhm | +42.0% | 0.1919 nH | +6.2% | 312 | 24 s | - |
| B' | =3 | 2.31 mOhm | -34.7% | 0.1917 nH | +6.0% | 312 | 24 s | - |
| B' | =4 | 3.736 mOhm | +5.7% | 0.1860 nH | +2.9% | 624 | 143 s solve, 161 s test | 52.7 MiB |

`=4` runs: KVL 1.1e-9 at iteration 0 (A), 0 enrichment iterations on both. B' L from Z = 0.04676 j Ohm at 40 MHz over the shorted far end (full-coupon values, as in 3.3b). The two half-sheets double the cell count (A 174 to 348, B' 312 to 624). Single mesh, single frequency, single reference each; the layered references themselves are `unverified`.

Reading against the plan table (no branch chosen here): R is within 10% on B' (+5.7%) and +15.6% on A, which falls in the 15-20% row of the 3.3b.2 table; L is within 6% on both (-1.0%, +2.9%). The A value is above the paper estimate of the half-sheet limit (the single-sided limit would give -8%), so the half-sheet current split found by the solver on A is not at the equal-current value.

## 3.3c checks of `P7_FOIL_SHEET=4` (2026-11-13)

Physical accuracy stays `unverified`. No code changed in this step. Validator, option (a), step 2.4 not touched; no branch chosen. Logs `debugging/p7-r/h/stage3d-*` (the `.log`/`.peak` coupon files; `stage3d-tps-*` run folders and `.rec` records).

### 3.3c.1 native build and the two deadline tests
`npm run native:build` once (`stage3d-native-build.log`, 10 min 38 s), so the module carries the two corrected report strings. Then one invocation each with `--test-threads=1`: `hybrid_via_without_mesh_slivers_solves_with_thin_wall_barrel` pass, 81 s wall (80.2 s test); `thin_wall_barrel_self_inductance_matches_shell_partial_inductance` pass, 110 s wall (109.8 s test) (`stage3d-test-*.log`, `stage3d-tests.txt`). Both are the tests that had `LOCAL_DEADLINE` in the parallel run.

### 3.3c.2 boundary t/delta = 1 (3.4 and 3.6 MHz, `=4`)
Same coupons, command and switch set as 3.3b.2 (coupon A 5 mm; B' 2 mm, 3 mm plane); t/delta 0.977 at 3.4 MHz (`=4` behaves as `=1`: one sheet, multiplier 1) and 1.005 at 3.6 MHz (two half-sheets, multiplier ratio((t/2)/delta) = 1.0057). Logs `stage3d-{A,Bp}-s4-{3.4M,3.6M}.log`.

| coupon | f | R | L | cells | time (solve) | peak memory |
|---|---|---|---|---|---|---|
| A | 3.4 MHz | 5.7249 Ohm/m | 561.36 nH/m | 174 | 5.0 s | 42.2 MiB |
| A | 3.6 MHz | 5.7783 Ohm/m | 571.92 nH/m | 348 | 19.3 s | 42.6 MiB |
| A | jump | +0.93% | +1.88% | | | |
| B' | 3.4 MHz | 1.4830 mOhm | 0.19471 nH | 312 | 19.8 s | 52.3 MiB |
| B' | 3.6 MHz | 1.5318 mOhm | 0.19426 nH | 624 | 82.6 s | 52.5 MiB |
| B' | jump | +3.29% | -0.23% | | | |

The frequency step 3.4 to 3.6 MHz itself changes R by physics (about +3% from sqrt(f) scaling of the skin part), so the jump is an upper bound of the switching artefact; a same-frequency comparison of `=1` and `=4` was not run.

### 3.3c.3 10 MHz, `=2`, `=3`, `=4`
Note: at 10 MHz t/delta = 1.68 for the full layer (3.35 at 40 MHz); the half-sheet is 0.84 and its multiplier 1.0429; `=2` uses ratio(1.68) = 1.5407, `=3` ratio(0.84) = 1.0429. There is no layered reference at 10 MHz (`COUPLED_PSD` refusal, not recomputed).

| coupon | mode | R | L | cells | time (solve) | between `=2` and `=3`? |
|---|---|---|---|---|---|---|
| A | =2 | 8.864 Ohm/m | 560.16 nH/m | 174 | 5.7 s | |
| A | =3 | 6.015 Ohm/m | 559.67 nH/m | 174 | 5.0 s | |
| A | =4 | 6.135 Ohm/m | 569.80 nH/m | 348 | 24.8 s | R yes (6.135 in 6.015..8.864); L no (above both, +1.7% over `=3`) |
| B' | =2 | 2.3038 mOhm | 0.19305 nH | 312 | 22.5 s | |
| B' | =3 | 1.5635 mOhm | 0.19266 nH | 312 | 21.1 s | |
| B' | =4 | 1.8155 mOhm | 0.19101 nH | 624 | 115.7 s | R yes; L no (below both, -0.9% under `=3`) |

Peak memory of every coupon run 42-53 MiB (test process working set).


### 3.3c.4 board TPS63060DSCR band-step at 10 and 40 MHz, `=4`
Scenario `s5/band-step` (two-port channel C1 to C2 on the TPS63060DSCR board, `tests/fixtures/analysis-p7-stage5/band-step.js`); the manifest lists only 10 kHz, 1, 100, 1000 MHz, so the runs use the driver of the step 1.1e sweep (`run-p7-matrix.py::run_command`, `verify-p7-stage5-scenario.ts band-step --band-hz <Hz>`), cap 4096 MiB (4 GB), row cap 900 s, cold cache, 4 cores, one run at a time. Switches: `P7_FOIL_SHEET=4 P7_BARREL_SHEET=2 P7_BARREL_NOISE=1 P7_ENRICH_MAX=64 P7_ENRICH_INDUCTIVE=5 P7_ENRICH_GMRES_MAX=120 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1`, no `P7_MESH_MERGE`, plus `P7_MAGNETIC_PHASE_PROFILE=1` (observability only; without it the KVL history is not printed). The first attempt of each without that flag (`stage3d-tps-*-noprofile*`; 10 MHz refused at 422 s with the same code; the 40 MHz one stopped at 101 s inside the H-matrix build with status `fail` and no result, cause not examined) was repeated once with the flag because the KVL numbers were missing.

| f | status | cells / unknowns | time | peak memory | iterations | KVL (it 0 .. last) | KVL uncertainty |
|---|---|---|---|---|---|---|---|
| 10 MHz | solver finished, validator refusal `CHANNEL_FOIL_EVIDENCE` (copper skin mesh rows or physical spacing do not match the candidate geometry) | 22625 supports, 43683 currents, 7186 potentials, 5017 planar vertices, 8 z planes | 415.7 s | 2509 MiB private (RSS 1837) | 5 (20 current modes) | 0.289 .. 7.5e-4 | 1.08e-2 |
| 40 MHz | same refusal | same mesh counts (supports 22625, currents 43683, potentials 7186) | 419.0 s | 2497 MiB private (RSS 1912) | 9 (36 current modes) | 0.289 .. 5.5e-4 (non-monotonic: 0.35 at it 1, 7.8e-3 at it 3, 1.0e-2 at it 4) | 1.24e-2 |

Training stage ended at 414.4 s (10 MHz, 568 magnetic actions) and 417.6 s (40 MHz, 628); the H-matrix build is about 307 s of that. The validator refusal at t/delta > 1 was expected; no model was returned, so no S-parameters were recorded. Final KVL is below 1e-3 in both, the operator KVL uncertainty is above it (1.1e-2, 1.2e-2).
