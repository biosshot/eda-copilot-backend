# P7-R high band, step 2.1a and stage-2 band-step check (2026-11-12)

Physical accuracy of every model stays `unverified`. Step 2.4, step 1.1 and the five-board check were not done. No Rust change; `npm run native:build` was not repeated (the addon is the stage-2 build). Decision table followed: `P7_BARREL_SHEET=2` and `spatial_recovery.rs` were left alone (moved to step 3.1).

## Step 2.1a code

- `src/analyze/board-foil-validation.ts`, `validateSkinMesh`: a `roundBarrels` row failing the delta/3 rule (`validSpacing`) is admitted by `barrelRow` only if: policy `hybrid-planar-v3`; `copperSkinMesh.barrelSheet` exists; the row has `shells === 1`; the validator's t/delta (geometry `finishedHolePlatingM`, `barrelConductivitySM`, `effectiveUpperHz`) is <= 1, the row's `skinDepthM` agrees with it (1e-8) and `maximumRadialStepM` equals the plating (1e-4 relative, the extractor takes the wall from the polygon rings); then `barrelSheet` must pass the existing `validFoilSheetRecord` (approximation true, `maxThicknessOverSkin` and `estimatedResistanceError` equal to the recomputed maximum and `foilResistanceRatio(x) - 1`, 1e-6). Without `barrelSheet`, or for t/delta > 1, the old rule applies.
- `native/analyze/index.d.ts`: optional `copperSkinMesh.barrelSheet` type.
- Tests (`tests/analyze-board-foil-cache.test.ts`, hybrid block): wall accepted at t/delta 0.5; rejected at 1.2; rejected without `barrelSheet`; rejected with 2 shells. The block passes. `npm run typecheck`: clean. In that file `compact magnetic evidence is checked without bypassing ownership or power gates` (line 308) still fails, the known HEAD failure; the other 10 tests pass.
- The tag: `copperSkinMesh.barrelSheet.approximation: true` with `estimatedResistanceError` appears in every `dsl-result.json` (`.../provenance/.../barrelSheet`), next to the existing `foilSheet` record and `tier: approximation` entries.

## Stage-2 band-step check

Env: `P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_BARREL_SHEET=1 P7_MESH_MERGE=1 P7_ENRICH_MAX=64 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1 P7_MAGNETIC_PHASE_PROFILE=1`, 900 s cap, per-run cache directory (cold), sequential, command `verify-p7-stage5-scenario.ts band-step --band-hz <hz>` through `run-p7-matrix.py::run_command` (scratchpad driver).
Deviation: the cap is 3584 MiB, not 4096. With 4096 the supervisor preflight (cap + 1024 MiB reserve) refused all four runs for lack of Windows commit headroom (about 5.0 GiB free at the time); the first attempt is lost (no run happened). A smaller cap is stricter; the runs peaked at 1.0 GiB.

| band-step | status | iterations | final KVL | KVL uncertainty | unconvergedPairs | wall | peak private / RSS |
|---|---|---|---|---|---|---|---|
| 100 kHz | model | 8 (0..7) | 5.5e-4 | 2.8e-4 | 342488 | 137 s | 1043 / 820 MiB |
| 700 kHz | model | 17 (0..16) | 8.1e-4 | 1.69e-3 | 342488 | 154 s | 1044 / 770 MiB |
| 1 MHz | model | 21 (0..20) | 8.4e-4 | 2.43e-3 | 342488 | 168 s | 1040 / 832 MiB |
| 3 MHz | refused `LOCAL_FOIL_REDUCTION_RESIDUAL` | 17 (0..16) | 2.2e-2 (KCL 3.4e-4) | 6.3e-3 | 342488 | 153 s | 1042 / 754 MiB |

Pass criterion (all four produce a model with KVL <= 1e-3): met at 100 kHz, 700 kHz and 1 MHz (validator accepts the one-layer wall at 1 MHz, which stage 2 had refused); NOT met at 3 MHz. Uncertainty stays <= 1e-2 everywhere (3 MHz peak 6.3e-3). 3 MHz KVL history (worst input): 0.29, 0.43, 0.27, 0.24, 0.24, 0.20, 0.165, 0.12, 0.087, 0.071, 0.055, 0.051, 0.035, 0.024, 0.020, 0.026, 0.022 at 2..66 current modes: it falls slowly and ends at iteration 16 with `P7_ENRICH_MAX=64` (the cap itself, 64 modes, was not the stop: 66 modes at the last line; the stop reason was not isolated). 3 MHz needs more than 17 iterations and its convergence rate is far below 1 MHz (KVL 1e-3 at iteration 17 there). Per the plan this points to step 1.1; not started. The 3 MHz t/delta: wall 25 um / delta 38 um = 0.66, foil 10 um / 38 um = 0.26 (both below 1, so validator limits were not the cause).

Logs: `debugging/p7-r/h/stage2c-{100k,700k,1m,3m}/run/{stdout.txt,dsl-result.json,scenario-result.json}`, `record.json` per folder, `debugging/p7-r/h/stage2c-runs.log`.

Blocking the five-board check: the 3 MHz criterion of the stage-2 check. The plan says to return to step 1.1 when 3 MHz needs more than 30 iterations; here the run ended at 17 iterations with the residual unmet, so the decision (step 1.1, raise the iteration cap, or change the 3 MHz expectation) is the user's.
