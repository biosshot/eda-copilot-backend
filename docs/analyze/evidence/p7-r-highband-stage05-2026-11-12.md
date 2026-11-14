# P7-R high band, stage 0.5: needle columns in via walls (2026-11-12)

Physical accuracy of every model stays `unverified`. Developer switches only, both default off; with them off the behaviour is unchanged.

## Code (`mixed_conductor.rs` only)

- `P7_BARREL_PROBE=1`: per barrel `P7_BARREL_PROBE barrel= id= columns= min_area_m2 median_area_m2 total_area_m2 length_m slivers=` and one `P7_BARREL_PROBE_SUMMARY` line. A sliver is a wall column whose triangle area is below 1e-6 of the barrel's summed column area (columns of one barrel have equal axial length and sigma, so area share equals conductance share).
- `P7_BARREL_NOISE=1`: those columns get no current mode and owner `Dropped`; they are counted in `sliver_columns_dropped` / `dropped_volume`, hence `quality.sliverPolicy`. The hanging-node collapse and the "dropped sliver columns would disconnect" check run as for other dropped columns; the sole-link retry runs with the switch off (all wall columns kept). Noise is decided before the radial-collapse planning, so dropped columns do not take part in it.
- Test `barrel_noise_test::needle_wall_column_is_flagged_by_conductance_share_per_barrel`.

Note: the "barrel 2141" of stage 0 is the index of the prism (`Owner::Barrel`), not the barrel; the barrel is `hole:pad:cd8747d128aec8e3e16` (barrel index 1 of 30).

## 0.5.1 Probe (700 kHz, `P7_FOIL_SHEET=1`, switch off; `debugging/p7-r/h/stage05-probe/`)

Board: 30 barrels, 2147 wall columns, **4 sliver columns, all in one barrel** (`...e16`); the other 29 barrels have none. That barrel: 137 columns, min area 4.76e-16 m2, median 7.71e-10 m2, total 8.83e-8 m2, length 1.6 mm. The neighbouring barrel `...e15`: 120 columns, min 2.7e-10 m2. So the diagnosis is confirmed: the needle area is 4.8e-16 m2 (about 1.6e-6 of the median; the read-only estimate was ~1e-16), the barrel and its prism 2141 is a sliver wall column. Run: refused as before, 182 s, 1103 MiB.

Probe-run cache: miss.

## 0.5.2 Checks

- `cargo test --lib mixed_conductor`: 19 pass, 2 fail. Both fail identically with `mixed_conductor.rs` reverted to HEAD (checked): `hybrid_barrel_test::thin_wall_barrel_potentials_match_the_electric_field_trace` (138 != 140 matched metal nodes) and `compact_via_dispatch_test::imported_via_two_drives_and_missing_contact` (`LOCAL_DEADLINE`, 25 s wall). Not caused by this change. The new test passes.
- Coupon `diag_compact_kvl_100k_vs_1m`, switch off: pass, 95 s; KVL 100 kHz 1.1e-3, 7.5e-6, 6.2e-8, 5.5e-9, 4.2e-9, 1.0e-9; 1 MHz 1.1e-2, 7.2e-4, 5.5e-5, 2.2e-6, ... identical to stage 0 (`debugging/p7-r/h/coupon-stage05-off.log`).
- Same with `P7_BARREL_NOISE=1 P7_BARREL_PROBE=1`: pass, 105 s; 0 sliver columns (58 and 114 columns, min area 6e-11 / 3e-11 m2), 0 dropped; same 100 kHz history; 1 MHz history the same to the last printed iteration (`coupon-stage05-on.log`).
- `npm run native:build` once (7 min 23 s, `native-build-stage05.log`).

## 0.5.3 Medium 700 kHz run (`P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_BARREL_PROBE=1 P7_MAGNETIC_PHASE_PROFILE=1`, `P7_HMATRIX_THREADS=4`, `RAYON_NUM_THREADS=4`, `P7_HMATRIX_CACHE=1`, 4096 MiB / 900 s caps; `debugging/p7-r/h/stage05-noise/`)

180 s, 1102 MiB peak (758 MiB RSS). `P7_BARREL_NOISE dropped_columns=4 cells=36`.

**H-matrix cache: miss** (rebuild, no time saved). Reason: the key hashes every `P7_*` variable not in the neutral list of `planar_hmatrix.rs`, so `P7_BARREL_NOISE` / `P7_BARREL_PROBE` change the key, and with the switch on the barrel prism set differs too. A hit needs the same switches on a repeat run. Outside the file list of this step: `P7_BARREL_PROBE` could go into the neutral list.

KVL history (worst over both phases/drives per iteration; the 17 iterations 0..17, compare stage 0 which stayed at ~1.0 from iteration 2 to 16):

| iter | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 8 | 10 | 12 | 14 | 16 | 17 (final) |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| maxKvl | 0.289 | 0.277 | 0.244 | 0.198 | 0.137 | 0.088 | 0.058 | 0.025 | 0.0122 | 0.0066 | 0.0027 | 0.00155 | 8.9e-4 |

maxKcl 3e-4 at iteration 0, 1.4e-4 at the end. The solver ran to `final-residual-done`; no `LOCAL_FOIL_REDUCTION_RESIDUAL` any more.

Worst rows: iteration 0 and 2 the same sheet cell as before (cell 7831, TOP sheet 3915, triangle 8959, axis 1, R 4.9e-4 ohm). Iteration 17: cell 4124, TOP sheet 2062, triangle 5573, axis 0, R 4.9e-4 ohm, terms R*q 1.7e-5, gradient 0, omegaL*q -1.7e-5. In between the worst row alternates between sheet cells (R 4.9e-4 ohm) and ordinary via barrels (`hole:via:...`, prism cells with R 0.056 to 0.32 ohm; none above 0.4 ohm). Cell 22001 / R 32.9 kohm is gone; the largest cell resistance among worst rows is 0.32 ohm.

Result of the band-step: `refused`, `CHANNEL_FOIL_EVIDENCE: copper skin mesh rows or physical spacing do not match the candidate geometry`, raised by `validateSkinMesh` in `src/analyze/board-foil-validation.ts` (it requires skin-depth/5 slab spacing for `hybrid-planar-v3`, which a one-sheet layer under `P7_FOIL_SHEET=1` does not provide). That gate runs after the solve; it is outside the file list and the plan and not changed here. So the compact solve now converges at 700 kHz, but the band-step still does not return a model because of this validation.

Fork verdict (plan 3a): the solver produced a converged result (KVL 8.9e-4, falling every few iterations), so stage 1 reduces to 1.3 and 1.4 as far as the solver goes. The `CHANNEL_FOIL_EVIDENCE` check of the sheet mode needs a decision from the user before a model is delivered.
