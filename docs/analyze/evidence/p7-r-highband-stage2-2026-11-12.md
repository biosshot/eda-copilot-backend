# P7-R high band, stage 2 steps 2.1-2.3 (2026-11-12)

Physical accuracy of every model stays `unverified`. Step 2.4 and the stage-2 expensive check were not done. All switches are developer switches, default off; with them off the coupon KVL history is unchanged (`debugging/p7-r/h/stage2-coupon-off.log` against stage 1: 100 kHz 1.1e-3 ... 1.0e-9, 1 MHz 1.1e-2 ... 1.1e-9).

## Code

Files: `copper_skin_profile.rs`, `mixed_conductor.rs`, `board_mesh.rs`, `mesh_coarsen.rs`. `board_mesh.rs` also got `pub(super)` on the `CopperSkinBarrel` fields.

- `P7_BARREL_SHEET` (`copper_skin_profile::barrel_sheet_mode`): `1` makes `radial_plan` return one radial layer at any skin depth (no radial constraint work, step = wall thickness) and adds `copperSkinMesh.barrelSheet {approximation, criterion, warning (delta < 3t: wall radial step exceeds delta/3), maxThicknessOverSkin, estimatedResistanceError, resistanceScaled}`. `2` also multiplies the resistance of cells owned by a barrel prism by `foil_resistance_ratio(t/delta)` at the profile upper frequency (existing helper, one value per band) in `prepare_model`.
  Deviation from the plan text (the multiplication is part of the switch there): with the factor on, the coupon at 1 MHz is refused `LOCAL_SPATIAL_RECOVERY: recovered copper power disagrees with model` (`stage2-coupon-on.log`), because `spatial_recovery.rs:65` recomputes barrel power from the plain conductivity and its gate is 1e-8. That file is not in the step's list, so the factor is a separate value `2` and `1` does not scale. The planar one-sheet mode (`P7_FOIL_SHEET`) likewise only records the error estimate. The factor at t/delta 0.32 (this board, 1 MHz) is 1.0009 (0.09%).
- `P7_MESH_MERGE=1|2` (`mesh_coarsen::merge_slivers`, called in `board_mesh.rs` after the minimum-angle refinement, before the planar triangles are read). Faces with minimum angle below `P7_MESH_MERGE_ANGLE` (default 1 degree) are needles. Vertices present after constraint insertion are protected (copper, bore, contact contours). Per needle face the removable vertex with the largest angle is removed: a vertex without constraint edges, or an unprotected vertex with exactly two constraint edges lying within 1e-9 mm of their chord (a Steiner vertex of a split contour edge), after which the chord is restored as a constraint edge. Level 2 also removes a protected vertex with two constraint edges within `P7_MESH_MERGE_TOL_MM` (default 1e-6 mm) of its chord (moves a contour by at most that; measured 8.5e-8 mm). Up to 16 passes. Prints `P7_MESH_MERGE {...}` and records `quality.merge` in `P7_MESH_QUALITY`. At level 1 no copper contour vertex is moved.
- Tests added in `mesh_coarsen.rs`: free needle vertex removed and contour kept; collinear Steiner vertex removed and the bottom edge restored; contour vertex kept at level 1 and removed at level 2 within 1e-6 mm.

## Checks

- `cargo test --lib mesh_coarsen`: 4 pass. `board_mesh`: 8 pass. `copper_skin_profile`: 4 pass. `mixed_conductor`: 19 pass, 2 fail (`hybrid_barrel_test::thin_wall_barrel_potentials_match_the_electric_field_trace`, `compact_via_dispatch_test::imported_via_two_drives_and_missing_contact`: the two known HEAD failures). `foil_reduced` was not changed and not rerun.
- Coupon `diag_compact_kvl_100k_vs_1m`, switches off: pass, 103 s, history as in stage 1. With `P7_BARREL_SHEET=1 P7_BARREL_NOISE=1 P7_BARREL_PROBE=1 P7_MESH_MERGE=1` (`stage2-coupon-on1.log`): pass; barrel columns 58 at 100 kHz and 58 at 1 MHz (114 at 1 MHz without the switch); KVL history equal to the switches-off history to the printed digits; coupon mesh: 12 needle faces, 4 removed (8 remain, all contour vertices). With the resistance factor on (`stage2-coupon-on.log`, first implementation, which scaled at `=1`): 1 MHz refused `LOCAL_SPATIAL_RECOVERY` (see above).
- `npm run native:build` once, 8 min 26 s (`native-build-stage2.log`).

## 2.1 barrel count, uncertainty, validator (band-step, TPS63060DSCR, `P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_ENRICH_MAX=64 P7_BARREL_SHEET=1 P7_MESH_MERGE=1`, `P7_BARREL_PROBE=1`)

| run | barrels / wall columns | KVL uncertainty (max) | result |
|---|---|---|---|
| 10 kHz and 700 kHz without the switch (earlier runs, `p7-r-band-plan.md` 1.1) | 2147 columns | 700 kHz stage 1: 3.6e-3 | model |
| 1 MHz without the switch (earlier run, one-sheet only) | 4413 columns | 1e11 | no convergence |
| 700 kHz, `P7_BARREL_SHEET=1`, merge 1 | 30 barrels, 2117 columns | 1.7e-3 | model |
| 1 MHz, `P7_BARREL_SHEET=1`, merge 1 | 30 barrels, 2117 columns (same as 700 kHz) | 2.4e-3 (solver converged: KVL 8.4e-4 at iteration 20, 21 iterations) | refused `CHANNEL_FOIL_EVIDENCE` |

The column count differs from 2147 only because merge 1 changes the mesh. The 2.1 done criterion (same barrel count as at low frequency; uncertainty not 1e11) is met by the numbers.

Validator outcome: `validateSkinMesh` (`src/analyze/board-foil-validation.ts`; barrel rows use `validSpacing` with divisor 3) refuses the one-layer wall at 1 MHz: plating 25 um is above delta/3 = 22 um. The stage 0.6 agreement does not cover via barrels, so the validator was not changed and no 1 MHz model is returned with `P7_BARREL_SHEET`. Decision for the user: accept a one-layer wall with its `barrelSheet` record at t/delta <= 1 in `validateSkinMesh` (like the planar sheet). 700 kHz is unaffected (delta/3 = 26 um >= 25 um, the wall is one layer there anyway).

## 2.2 mesh needles (700 kHz, `P7_MESH_QUALITY`, `stage2-700k-m1`, `stage2-700k-m2`)

| run | needle faces (< 1 degree) | minimum angle | dropped sliver columns (`sliverPolicy.columns`) | barrel needle columns (probe) |
|---|---|---|---|---|
| baseline (stage 1) | not counted | 1.34e-4 degrees | 81 | 4 (one barrel, 36 cells) |
| `P7_MESH_MERGE=1` | 381 -> 173 | 1.34e-4 degrees | 19 | 1 (9 cells) |
| `P7_MESH_MERGE=2` | 381 -> 161 | 1.62e-4 degrees | 14 | 1 |

The goal (minimum angle > 1 degree) is NOT met. Level 1 removed 272 Steiner and 6 free vertices; level 2 additionally 17 contour vertices (max shift 8.5e-8 mm). The worst face is the same triangle before and after: three nearly collinear vertices 0.034 mm apart, the middle one about 2e-7 mm off the chord (area 3e-9 mm2, far below the 1e-4 mm2 floor of the refinement). By the rule result it is not removable: no vertex is free and the middle one is not a vertex with exactly two constraint edges (inferred; its constraint edges were not printed). The largest remaining needle has height 0.256 mm, so some of the rest are long thin triangles, not noise; `improve` stops incomplete (`complete:false`, cap 1000 Steiner vertices). Fixing the rest needs either moving a contour vertex onto the shared edge (sub-nanometre, but a copper contour change; not done) or more Steiner splitting. Needs a decision.

Needle barrel columns: the 4 needles of stage 0.5 became 1 (the merge changed the mesh around three of them); `P7_BARREL_NOISE` still drops the remaining one. `sliverPolicy` 81 versus 4: the count is `sliver_columns_dropped`, which holds the barrel needles (via `P7_BARREL_NOISE`) plus the planar columns dropped in `mixed_conductor.rs` (altitude below thickness/200, or annulus-source slivers), so about 77 are not barrel needles. The merge removes most of them (81 -> 19), consistent with them being mesh needles; the per-class split was not printed.

## 2.3 700 kHz band-step with the new switches (`stage2-700k-m1`; standard env, `P7_BARREL_SHEET=1 P7_MESH_MERGE=1`, per-run cache directory)

| quantity | stage 1 (baseline) | stage 2 (merge 1) | stage 2 (merge 2, `stage2-700k-m2`) |
|---|---|---|---|
| status | model | model | model |
| `unconvergedPairs` | 389253 | 342488 | 342791 |
| `maxRelativeChange` | 0.0974 | 0.159 | 0.159 |
| KVL uncertainty (`maxCompactKvlUncertaintyRelative`) | 3.6e-3 | 1.69e-3 | 1.36e-3 |
| iterations (0..n) | 18 | 17 | 17 |
| final KVL | 8.9e-4 | 8.1e-4 | 8.1e-4 |
| wall / peak private / RSS | 165 s / 1102 / 836 MiB | 154 s / 1042 / 658 MiB | 153 s / 1049 / 813 MiB |
| H-matrix build | 122 s | 109 s | 108 s |

H-matrix cache: miss in all runs (cold per-run directory). Logs: `debugging/p7-r/h/stage2-700k-m1/run/stdout.txt`, `stage2-700k-m2`, `stage2-1m-m1`; each run directory has `dsl-result.json`, `scenario-result.json` and `record.json`.

## 2.4 condition

`unconvergedPairs > 0` (342488) AND KVL uncertainty > 1e-3 (1.69e-3 with merge 1, 1.36e-3 with merge 2): both hold, so the plan's condition for step 2.4 is met. It was not started. The uncertainty roughly halved against the baseline while the pair counter barely moved; the mesh changed, so the pair set changed too, and the cause of the change was not isolated.
