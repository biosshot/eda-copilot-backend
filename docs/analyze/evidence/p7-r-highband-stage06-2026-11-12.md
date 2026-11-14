# P7-R high band, stage 0.6: one-sheet acceptance (2026-11-12)

Physical accuracy of every model stays `unverified`. The one-sheet level is an approximation tier (user-approved criterion: t/delta <= 1; not via barrels, not t/delta > 1).

## Code

- `src/analyze/board-foil-validation.ts`, `validateSkinMesh`: a copper-layer row of `hybrid-planar-v3` whose step exceeds delta/5 is admitted only if all hold: `provenance.compactOperator.foilSheet` exists; the row has `slabs === 1`; the validator's own t/delta (geometry `thicknessM`, `conductivitySM`, `copperSkinMesh.effectiveUpperHz`) is <= 1 and the row's `skinDepthM` and `maximumStepM` (= thickness) agree with it; `foilSheet.approximation === true`, `maxThicknessOverSkin` equals the recomputed maximum t/delta over the sheet rows (1e-6) and `estimatedResistanceError` equals the validator's `foilResistanceRatio(x) - 1` (1e-6). Without `foilSheet` the old rule is unchanged. Barrel rows (`roundBarrels`) are untouched (delta/3).
- `native/analyze/index.d.ts`: optional `compactOperator.foilSheet` type. No Rust change; Rust already emits `evidence["foilSheet"]` (`mixed_conductor.rs:2178`) as `provenance.compactOperator.foilSheet` with `approximation`, `maxThicknessOverSkin` (max over foils, only layers above 0.2), `estimatedResistanceError`, `bandTopHz`, `criterion`, `warning`. It carries the maximum, not a per-layer value, so the validator compares against its own maximum over sheet rows (a layer with no foil but sheet spacing would make the recomputed maximum exceed the declared one and be refused: conservative).
- Tests (`tests/analyze-board-foil-cache.test.ts`, inside the hybrid/RF block): accepts a sheet at t/delta 0.5 and 1.0; rejects 1.2, missing `foilSheet`, a wrong declared error, a declared 0.5 on a real 1.2, two slabs, and a barrel row with a coarse radial step while a sheet record is present. Error figures checked: 0.7% at t/delta 0.53, 9% at 1.

## Checks

- `npm run typecheck`: clean.
- `node --import tsx --test tests/analyze-board-foil-cache.test.ts`: 10 pass, 1 fail. The failing test, `compact magnetic evidence is checked without bypassing ownership or power gates`, fails identically on HEAD (verified by stashing the two files): its last loop, `changes[15]` (one of the `...=1e-7` tolerance mutations), does not throw. My new cases run earlier in that test and pass. Not fixed here (outside this step).

## 0.6.4 Medium 700 kHz run (`debugging/p7-r/h/stage06-sheet/`)

Env: `P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_MAGNETIC_PHASE_PROFILE=1 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1`, 4096 MiB / 900 s caps, via `run_command` of `scripts/run-p7-matrix.py` with `verify-p7-stage5-scenario.ts band-step --band-hz 700000` (driver script in the scratchpad, not in the repo). H-matrix cache: miss.

Result: **model** (`scenario-result.json status=model`, DSL `complete`, solution `complete`), 205 s, 1124 MiB private peak (826 MiB RSS). 18 grid iterations (0..17); KVL 0.289 at iteration 0, 0.0027 at 14, 0.00155 at 16, 8.9e-4 at 17; KCL 1.4e-4 at the end; `final-residual-done`. `residualPolicy: tier approximation, limit 1e-3`. Diagnostics: `fill_freshness`, `CHANNEL_FOIL_PHYSICAL_UNVERIFIED`, `DATA_UNAVAILABLE`.

0.6.2: `dsl-result.json` carries `foilSheet {approximation: true, maxThicknessOverSkin: 0.4431, estimatedResistanceError: 0.00342, bandTopHz: 700000, ...}` at `reports/0/calculations/1/result/solution/provenance/compactOperator/foilSheet` (and in `foilPlan.native`, `segments`, `plots/0/data`), next to `quality.residualPolicy.tier = approximation`. Thickness at 700 kHz is t/delta 0.44 (estimated sheet resistance error 0.34%).

Note: `sliverPolicy` in the result reports 81 columns, droppedVolumeRelative 5.1e-8 (noise tier), more than the 4 needle columns of the stage 0.5 probe (that probe counted only the wall columns by conductance share; this counter covers all dropped sliver columns).

## Clean-tree check of the two failing `mixed_conductor` tests

Run with `foil_reduced.rs`, `mixed_conductor.rs`, `planar_hmatrix.rs` stashed to HEAD (restored afterwards), `CARGO_BUILD_JOBS=4`; log `debugging/p7-r/h/stage06-clean-tree-tests.log`. Both still fail on HEAD:

- `hybrid_barrel_test::thin_wall_barrel_potentials_match_the_electric_field_trace`: 138 != 140 matched metal nodes.
- `compact_via_dispatch_test::imported_via_two_drives_and_missing_contact`: `LOCAL_DEADLINE: streamed tetra magnetic action` after 25 s.

So they are not caused by stages 0 to 0.6.
