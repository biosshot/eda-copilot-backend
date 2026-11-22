# P7-R step 3.4 and stage-3 check (2026-11-13)

Physical accuracy of every model stays `unverified`. Nothing committed. Status of the two-half-sheet and wall-layer records: "approximation".

## Step 3.4 change

- `src/analyze/board-foil-validation.ts` `validateSkinMesh`:
  - A planar layer row with 2 slabs of t/2 and the validator's own t/delta > 1 is admitted only when `compactOperator.foilSheet` has `halfSheets: true`, `halfSheetLayers: 2`, `status: 'approximation'`, a `statusNote` containing "R завышено до 16% на двух купонах при 40 МГц, эталон не проверен" and `resistanceMultiplierMax` equal to the validator's `foilResistanceRatio((t/2)/delta)`. The extractor measures each half slab, so the record's `maxThicknessOverSkin` and `estimatedResistanceError` are those of the half slab (x/2, ratio(x/2) - 1); the validator recomputes them that way (largest of the one-sheet x and the half-slab x/2). Without the record, with 3 slabs or with a wrong multiplier the old refusal (`CHANNEL_FOIL_EVIDENCE`) stays.
  - A via-barrel row (one shell) with the wall t/delta > 1 is admitted only when `barrelSheet.resistanceScaled` is true (`P7_BARREL_SHEET=2`), the declared `resistanceMultiplierMax` equals `foilResistanceRatio(t/delta)` and the same `status`/text are present. `barrelSheet` with `resistanceScaled: false` at t/delta > 1 refuses as before.
- `native/analyze/index.d.ts`: optional fields on `foilSheet` and `barrelSheet`.
- `native/analyze/src/extraction/local/mixed_conductor.rs`: `foilSheet` and `barrelSheet` records gain `status: "approximation"` and `statusNote` (the text above; null when not in half-sheet / scaled-wall mode). `npm run native:build` once, 9 min 10 s (`debugging/p7-r/h/stage3e-native-build.log`). No Rust test was run (record strings only).
- First band-step attempt after the build (`stage3e-tps-10000000-attempt1-validator-fix-pending`, `...40000000-attempt1-...`): both solved and were still refused `CHANNEL_FOIL_EVIDENCE`. A temporary diagnostic print (removed) showed the record's `maxThicknessOverSkin` is the half-slab value (0.837 at 10 MHz, layer x 1.675) while my first validator version compared it with the layer x. Fixed once in the validator; no Rust change needed for it.
- Test: `tests/analyze-board-foil-cache.test.ts` (inside the existing foil-sheet block): accepts the two-half-sheet record at x = 3.35 and the scaled wall at x = 3.35; refuses without the record, with the multiplier ratio(x) instead of ratio(x/2), `halfSheets: false`, `halfSheetLayers: 3`, a note without the text, `status: 'verified'`, three slabs, wall with `resistanceScaled: false`, wall with ratio(x/2) multiplier, wall without note, and wall at x = 3.35 with the old one-layer record. Result: 10 of 11 tests pass; the only failing test is the known `compact magnetic evidence is checked without bypassing ownership or power gates`, which fails at its last loop (line 341, after the new block, unchanged by this step). `npm run typecheck` clean.

## TPS63060DSCR band-step 10 and 40 MHz (step 3.4 check)

Flags: `P7_FOIL_SHEET=4 P7_BARREL_NOISE=1 P7_BARREL_SHEET=2 P7_ENRICH_INDUCTIVE=5 P7_ENRICH_GMRES_MAX=120 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1 P7_MAGNETIC_PHASE_PROFILE=1` (`stage3e-env.sh`); no `P7_MESH_MERGE`, no `P7_ENRICH_MAX`. 4 cores, 4096 MiB cap, 900 s, cold cache, driver `run-p7-matrix.py::run_command` (`s5/band-step`). Both return a model (`scenario-result.json` status `model`, diagnostics `CHANNEL_FOIL_PHYSICAL_UNVERIFIED`). No refusal on KVL or uncertainty.

## Stage check (a): five boards

Command per board: `python scripts/run-p7-matrix.py --rows board/<id> --mode cold --row-cap 1800 --memory-mib 8192 --out debugging/p7-r/h/stage3e-boards-<id>` with the flag set above; standard frequencies of the queue (the dsl results show a 1 kHz, 3.16 kHz, 10 kHz grid, band top 10 kHz, t/delta 0.038, so no half-sheet record is produced: `foilSheet` appears only above t/delta 0.2; the `barrelSheet` record with status/note appears with multiplier 1.0000002). "iterations" = grid steps of the last solve. Cells = currents (H-matrix supports in brackets).

| board | stage 6 (2026-11-11) | boards 2026-11-12 (nomerge rerun for Nano/Thunder) | this run | iterations | final KVL | KVL uncertainty | time | peak private | vs 11-12 / vs stage 6 |
|---|---|---|---|---|---|---|---|---|---|
| STM32G0_CAN_RS485 | fail LOCAL_FIELD_CUT, 175 s, 1256 MiB | model, 437.9 s, 6258 MiB | model, 101736 cells (51287) | 2 | 3.2e-4 | 1.1e-3 | 401.7 s | 5484 MiB | better (faster, less memory) / better |
| NanoScope | fail LOCAL_POLYHEDRON_SOURCE_QUADRATURE, 536 s, 4868 MiB | truncated model, 1215 s, 5113 MiB | truncated-tier model, unresolved return 5.9%, 90862 cells (38966) | 7 | 1.8e-6 | 7.6e-2 | 1025.2 s | 5279 MiB | better time, memory +3% / better |
| ThunderF722 | timeout 900 s, LOCAL_COPPER_OVERLAP | timeout 1800 s, 7986 MiB | no result: `resource-failure`, "Windows commit headroom fell below reserve" at 1089.8 s, 8009 MiB, stage `magnetic-prepare-start` (791 s); rerun not possible, see below | - | - | - | - | 8009 MiB | same (no answer) / same |
| TPS63060DSCR | pass, 118 s, 609 MiB | pass, 134.6 s, 1041 MiB | model, 23141 cells (12210) | 3 | 1.7e-5 | 3.9e-4 | 156.4 s | 1061 MiB | same (slower, +16%) / same |
| BandAmpV5 | fail LOCAL_FIELD_CUT, 191 s | model, 334.4 s, 2630 MiB | model, 49785 cells (28016); run with cap 4096 MiB, see below | 3 | 9.1e-4 | 3.0e-4 | 405.6 s | 2666 MiB | worse time (+21%), same memory / better |

Not run as specified (machine memory, recorded, not code):
- After ThunderF722 was killed by the supervisor ("Windows commit headroom fell below reserve", not the 8192 MiB cap), the supervisor preflight (cap + 1024 MiB reserve = 9216 MiB commit headroom) refused TPS63060DSCR and BandAmpV5 (headroom 8.4 GB then 4-7 GB; other applications of the machine were holding commit). Logs `stage3e-boards-attempt1/`.
- TPS63060DSCR was repeated once at the specified cap 8192 MiB when headroom allowed it (result above).
- BandAmpV5 could not get 9216 MiB headroom in two 25 and 20 minute waits (peak seen about 7.3 GB); it was run with `--memory-mib 4096` instead (its previous peak was 2630 MiB, so the cap is not binding). Logs `stage3e-boards-attempt1/b2/`, `stage3e-seq4.log`, `stage3e-seq5.log`.
- ThunderF722 could not be rerun (headroom 7.3 GB < 9216 MiB after a 20 minute wait). Its single result is the spoiled run above (killed by machine commit headroom, not a physical refusal and not a solver failure). Its earlier no-merge run (11-12) reached the 1800 s cap at 7986 MiB in the first local magnetic action; the present run reached `magnetic-prepare-start` at 791 s, in line with that.

## Stage check (b): TPS63060DSCR band-step sweep

Same flags, 4 cores, 4096 MiB, 900 s, cold, one at a time. 10 and 40 MHz are the step-3.4 runs above (same build and flags, not repeated). Logs `debugging/p7-r/h/stage3e-tps-<Hz>/`, `.rec`.

| f | status | cells (currents; H-matrix supports) | iterations (grid steps) | final KVL | KVL uncertainty | time | peak private |
|---|---|---|---|---|---|---|---|
| 100 kHz | model | 22867; 12073 | 7 | 2.7e-5 | 6.3e-4 | 173 s | 1067 MiB |
| 1 MHz | model | 22867; 12073 | 9 | 8.8e-4 | 5.0e-3 | 172 s | 1067 MiB |
| 3 MHz | model | 22867; 12073 | 5 | 2.4e-4 | 1.2e-2 | 166 s | 1063 MiB |
| 10 MHz | model | 43683; 22049 | 5 | 7.5e-4 | 1.1e-2 | 392 s | 2496 MiB |
| 40 MHz | model | 43683; 22049 | 9 | 5.5e-4 | 1.2e-2 | 364 s | 2504 MiB |

Reading: up to 3 MHz (t/delta <= 1) the mesh is the one-sheet mesh; at 10 and 40 MHz two half-sheets double the cell count and the time is 2.1-2.4x, memory 2.4x the 100 kHz run (the known cost of two half-sheets; the stage criterion of 1.5x is not met). Operator KVL uncertainty at 3, 10 and 40 MHz is 1.1-1.2e-2, above 1e-2 (step 2.4 condition, not started).

## Approximation tags seen in `dsl-result.json` (TPS 40 MHz, `stage3e-tps-40000000/run/dsl-result.json`)

- `compactOperator.foilSheet`: `approximation: true`, `halfSheets: true`, `halfSheetLayers: 2`, `maxThicknessOverSkin` 1.6748 (half-slab value), `estimatedResistanceError` 0.5407, `resistanceMultiplierMax`/`Min` 1.5407, `multiplierTag` "approximation, two half-sheets per copper layer with t/delta > 1 ...", `fieldApproximation` "approximation, two half-sheets: ...", `status: "approximation"`, `statusNote` "R завышено до 16% на двух купонах при 40 МГц, эталон не проверен".
- `copperSkinMesh.barrelSheet`: `approximation: true`, `resistanceScaled: true`, `maxThicknessOverSkin` 2.3926, `resistanceMultiplierMax` 2.3552, `multiplierTag` "approximation, one multiplier per band; one-sided-field approximation", `status: "approximation"`, `statusNote` (the same text plus "оценка для плоских полулистов; стенка via против эталона не сверялась").
- Boards at 1-10 kHz: only `barrelSheet` (multiplier 1.0000002) with the same `status`/`statusNote`; no `foilSheet` record (t/delta 0.038 < 0.2). The wall note therefore also appears where the multiplier is 1 and the 16% statement does not apply; flagged, not changed.
- Diagnostic codes of the models: `CHANNEL_FOIL_PHYSICAL_UNVERIFIED`, `fill_freshness`, `DATA_UNAVAILABLE`.

Logs: `debugging/p7-r/h/stage3e-*` (`stage3e-env.sh`, `stage3e-tps*.sh`, `stage3e-boards*.sh`, `stage3e-seq4.sh`, `stage3e-seq5.sh`, `stage3e-wait.py`, per-run folders and `.rec`).
