# P7-R: probe of unconverged near pairs by class (2026-11-13)

Diagnostic only; no behaviour change. Physical accuracy of every model stays `unverified`. Nothing committed.

## What was added

`P7_HMATRIX_PAIR_PROBE=1` (default off) in `native/analyze/src/extraction/local/planar_hmatrix.rs` only. Every `Kernel::exact` entry (the pair integral that carries the `change` figure; moment-path pairs have none) is classified and summed in a per-block `Stats.probe`, merged into `Stats`; `Stats::json()` adds `pairProbe` to the existing `P7_COMPACT_HMATRIX` line. With the flag the H-matrix disk cache is bypassed (a cache hit evaluates no pairs). Values returned by the kernel are untouched.

Classes (3-D, tolerance 1e-9 m): xy vertices shared only count when the z intervals touch. `same-footprint` = 3 shared vertices (stacked prisms or a diagonal entry), `shared-edge` = 2, `shared-vertex` = 1, `touching-other` = bounding boxes touch, no shared vertex (T-junction, overlapping layers), then `gap<0.5d`, `gap<2d`, `gap>=2d` with gap = 3-D bounding-box gap and d = the smaller bounding-box diagonal. Aspect of a prism = max(longest triangle edge, thickness) / min(smallest triangle altitude, thickness); a pair takes the larger of its two prisms, buckets 10 / 100 / 1000. Unconverged = `change > 1e-5` (`PAIR_TARGET_RELATIVE_CHANGE`), above noise = `change > 2e-3`.

Action contribution proxy (the real action weights the pair integrals by mode vectors that the probe does not see): `sumAbs` = sum of |pair integral| (m^5) and `err` = sum of |pair integral| x relative change, the absolute error bound of the entry. Shares of the "unconverged error bound" are `err` of the unconverged entries divided by their total. Counts are `exact()` calls: ACA samples and dense-fallback repeats are counted again, exactly as the existing `unconvergedPairs` counter does (the totals 11126007 and 389253 equal `exactPairs` and `unconvergedPairs`).

## Run

`npm run native:build` once (8 min 1 s, `CARGO_BUILD_JOBS=4`, `debugging/p7-r/h/pairs-build.log`). One 700 kHz band-step, final switch set plus the probe flag and `P7_MAGNETIC_PHASE_PROFILE=1`, cap 3584 MiB, 900 s, 4 cores (`debugging/p7-r/h/pairs-probe.sh`, run `pairs-probe-700k/`, raw `pairs-probe-700k.probe.json`): `P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_BARREL_SHEET=1 P7_ENRICH_MAX=64 P7_ENRICH_INDUCTIVE=5 P7_ENRICH_GMRES_MAX=120 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1 P7_HMATRIX_PAIR_PROBE=1`.

Result: model, 13 iterations, KVL 9.8e-4, KVL uncertainty 3.6e-3, 157 s, 654 MiB peak private. This is the first 700 kHz point without `P7_MESH_MERGE` (earlier with it: uncertainty 1.7e-3, 342488 unconverged, 154 s). H-matrix: 12073 supports, stored 209 MB (dense 96 MB, low-rank 113 MB), 11126007 exact entries, 389253 unconverged (3.5%), 2528 above noise, max change 0.097.

## Class table (700 kHz)

| class | exact entries | unconverged | share of all unconverged | unconverged within class | above noise | unconverged sumAbs / all sumAbs | share of unconverged error bound |
|---|---:|---:|---:|---:|---:|---:|---:|
| same-footprint | 12073 | 7897 | 2.0% | 65.4% | 798 | 6.057% | 77.0% |
| shared-edge | 16790 | 7385 | 1.9% | 44.0% | 484 | 0.978% | 7.9% |
| shared-vertex | 68514 | 16095 | 4.1% | 23.5% | 716 | 0.788% | 5.1% |
| touching-other | 28490 | 4908 | 1.3% | 17.2% | 281 | 0.099% | 0.7% |
| gap<0.5d | 275585 | 6386 | 1.6% | 2.3% | 231 | 1.201% | 6.9% |
| gap<2d | 1258231 | 61686 | 15.8% | 4.9% | 18 | 0.944% | 0.5% |
| gap>=2d | 9466324 | 284896 | 73.2% | 3.0% | 0 | 3.280% | 1.8% |

| aspect | exact entries | unconverged | share of unconverged | within bucket | above noise | unconverged sumAbs / all | share of unconverged error bound |
|---|---:|---:|---:|---:|---:|---:|---:|
| <10 | 171827 | 5118 | 1.3% | 3.0% | 0 | 0.003% | 0.0% |
| 10-100 | 8713849 | 244045 | 62.7% | 2.8% | 1298 | 2.606% | 9.7% |
| 100-1000 | 2218574 | 139677 | 35.9% | 6.3% | 1226 | 10.736% | 90.3% |
| >=1000 | 21757 | 413 | 0.1% | 1.9% | 4 | 0.000% | 0.0% |

## Reading

- By count the unconverged pairs are separate pairs: 73% have a gap of at least twice the smaller size, 90.7% are not in contact. They barely miss the 1e-5 target (none is above noise) and carry 1.8% of the unconverged error bound.
- By error bound the contact classes dominate: same-footprint, shared-edge, shared-vertex and touching pairs are 9.3% of the unconverged entries but 90.7% of their error bound; same-footprint alone is 77%. All 2528 above-noise entries are in classes with contact or a gap under half the size.
- By shape, the aspect bucket 100-1000 holds 35.9% of the unconverged entries and 90.3% of the error bound. Contact pairs with aspect of at least 100 are 7521 entries and 81.8% of the error bound. This is the class of the ignored test `hybrid_via_solves_with_thin_wall_barrel_in_seconds` (adjacent prisms 4.5 mm x 48/100 um, aspect about 94 to 100, shared-edge Duffy, last change 6.8e-4 at order 64); the measured changes here (maximum 0.097) are of that order or above, so it is plausibly the same class, not proven by this probe alone.
- Summed over all exact entries the error bound is 2.3e-5 of the sum of |pair integral|, 2.1e-5 of it from unconverged entries. That is a bound on single entries, not on the action or on the KVL uncertainty (3.6e-3 at this point); the probe does not connect the two, so it does not say how much fixing these pairs would lower the uncertainty.
- Step 2.4 was not started. If it is, the targets by this probe are same-footprint and contact pairs of aspect 100-1000, not the far separate pairs.
