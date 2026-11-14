# P7-R: five boards with mode 5 (2026-11-12)

Physical accuracy of every model stays `unverified`. Nothing committed. Switch set: `P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_BARREL_SHEET=1 P7_MESH_MERGE=1 P7_ENRICH_MAX=64 P7_ENRICH_INDUCTIVE=5 P7_ENRICH_GMRES_MAX=120 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1`. Command: `python scripts/run-p7-matrix.py --rows board/ --mode cold --row-cap 1800 --memory-mib 8192 --out debugging/p7-r/h/step11e-boards` (cap 1800 s / 8192 MiB instead of stage 6's 900 s so slow boards can finish; cold, sequential, 4 cores). Logs: `debugging/p7-r/h/step11e-boards/` (`summary.md`, `results.json`, `cold/board_*`).

| board | stage 6 (`p7-v1-stage6-2026-11-11.md`) | closure measurement (CHANGELOG, latest before P7-R) | this run | vs stage 6 |
|---|---|---|---|---|
| STM32G0_CAN_RS485 | fail LOCAL_FIELD_CUT, 175 s, 1256 MiB | model, 277 s | pass, model, 437.9 s, 6258 MiB | better; slower and 6.3 GiB private peak vs 4.0 GiB at closure (above the 4 GB medium-run limit; boards have their own 8192 cap) |
| NanoScope | fail LOCAL_POLYHEDRON_SOURCE_QUADRATURE, 536 s, 4868 MiB | truncated model, 1618 s | refusal `LOCAL_PLANAR_INPUT` (incomplete/nonmanifold far-domain coverage), 144 s, 1174 MiB, before the solver | same class (refusal) as stage 6, worse than closure |
| ThunderF722 | timeout 900 s, `LOCAL_COPPER_OVERLAP` | refusal `NETWORK_SINGULAR` | refusal `LOCAL_PLANAR_INPUT`, 523 s, 1162 MiB, before the solver | same class (refusal) as stage 6 |
| TPS63060DSCR | pass, 118 s | 101 s | pass, 134.6 s, 1041 MiB | equal |
| BandAmpV5 | fail LOCAL_FIELD_CUT, 191 s | model, 358 s | pass, model, 334.4 s, 2630 MiB | better |

No board is worse than stage 6. Against the closure measurement NanoScope is worse (truncated model before, refusal now) and STM32G0 uses more memory.

Both new refusals fail at `spatial-certified-end`, before any solver step, so mode 5 is not involved. Isolation run (one board, same set without `P7_MESH_MERGE`, cap 600 s, `debugging/p7-r/h/step11e-nano-nomerge`): NanoScope passes the planar-input stage and reaches `magnetic-actions-start` at 350 s, then the 600 s row cap stops it (timeout, 5251 MiB), so `P7_MESH_MERGE=1` is the cause of `LOCAL_PLANAR_INPUT` on NanoScope (ThunderF722 not isolated; same error). Final switch set for these boards therefore should omit `P7_MESH_MERGE` unless that is fixed (decision for the user; the plan's stage 2 kept it for the via coupon boards).

## Rerun without `P7_MESH_MERGE` (NanoScope, ThunderF722) and STM32G0 memory (2026-11-13)

Final switch set from here on (`P7_MESH_MERGE` removed; the switch itself is not fixed): `P7_FOIL_SHEET=1 P7_BARREL_NOISE=1 P7_BARREL_SHEET=1 P7_ENRICH_MAX=64 P7_ENRICH_INDUCTIVE=5 P7_ENRICH_GMRES_MAX=120 P7_HMATRIX_THREADS=4 RAYON_NUM_THREADS=4 P7_HMATRIX_CACHE=1`. One board at a time, cold, 4 cores, row cap 1800 s, 8192 MiB: `python scripts/run-p7-matrix.py --rows board/<id> --mode cold --row-cap 1800 --memory-mib 8192 --out debugging/p7-r/h/final-boards-<id>` (driver `debugging/p7-r/h/final-boards.sh`, log `final-boards.log`). Physical accuracy of every model stays `unverified`.

| board | stage 6 (`p7-v1-stage6-2026-11-11.md`) | closure P7 v1 (CHANGELOG) | this run |
|---|---|---|---|
| NanoScope | fail `LOCAL_POLYHEDRON_SOURCE_QUADRATURE`, 536 s, 4868 MiB | truncated model, 1618 s, 4.7 GB | `pass`, model+approximation (`domainPolicy.tier = truncated`, unresolved return 5.9%, limit 10%), ended at training iteration 6 / `return-expansion` phase, 1215.5 s, 5113 MiB private peak, no refusal code |
| ThunderF722 | timeout 900 s (cold), `LOCAL_COPPER_OVERLAP` (restart) | refusal `NETWORK_SINGULAR` | `timeout` at the 1800 s row cap, no refusal code, last stage `magnetic-actions-start` (1117.6 s), still inside the first local magnetic action; 7986 MiB private peak (cap 8192) |

Stage timeline. NanoScope: input/certified end 146 s, H-matrix build 268.6 s (cache miss, 4 threads, stored 994 MB: dense 449 MB, low-rank 544 MB, 1119348 unconverged pairs, max change 0.48), `magnetic-prepare-end` 431 s, magnetic actions 433-861 s, training 862-1185 s. ThunderF722: certified end 597.7 s (slow input stage), H-matrix build 286 s (stored 1095 MB: dense 518 MB, low-rank 577 MB, 2827 above noise, max change 0.145), charge actions end 1117.6 s, `potentials=27596 currents=95461 reservedModes=256 modeCap=344 plannedBytes=3088676352`; the 683 s left did not finish the first two local magnetic actions (NanoScope's first two magnetic actions took 428 s; the last ThunderF722 log line is `P7_COMPACT_ACTION stage=local-start action=2`, so whether it was slow or stuck is not known from the log).

Reading. Without `P7_MESH_MERGE` both boards pass the planar-input stage that refused them (`LOCAL_PLANAR_INPUT` with the switch). NanoScope returns the same class of result as the closure (truncated model) in 25% less time and 9% more memory (5113 MiB vs about 4.7 GB); it is better than stage 6 (refusal). ThunderF722 no longer refuses but does not finish in 1800 s: time-limited, not a physical refusal; compared with stage 6 (timeout 900 s) and the closure (refusal) it is the same non-answer, now stopped later in the pipeline. Logs: `debugging/p7-r/h/final-boards-NanoScope/`, `final-boards-ThunderF722/`, `final-boards.log`.

### STM32G0_CAN_RS485 memory: 4.0 GiB at closure vs 6.3 GiB (no new run)

Sources: closure log `debugging/p7-v1-stage2/final1/STM32G0_CAN_RS485/{stdout.txt,supervision.json}` (CHANGELOG closure measurement, 232 s wall; the 277 s in the CHANGELOG is the contended repeat); this run `debugging/p7-r/h/step11e-boards/cold/board_STM32G0_CAN_RS485/stdout.txt`. The closure log predates the dense/low-rank split (stage 0.2), so only `storedBytes` can be compared exactly.

| quantity | closure P7 v1 | step11e boards (mode 5, with `P7_MESH_MERGE`) |
|---|---|---|
| mesh nodes (`P7_MESH_COARSEN`) | 154116 | 150390 |
| H-matrix supports | 51287 | 50201 |
| H-matrix stored bytes | 1041258920 (993 MiB) | 1220936368 (1164 MiB): dense 373709744 (356 MiB) + low-rank 847226624 (808 MiB) |
| far / near blocks, max rank | 24442 / 197622, 22 | 28308 / 227790, 23 |
| unconverged pairs, max change | 665924, 0.463 | 974745, 0.493 |
| H-matrix disk cache | off | on, file 1234566354 B written (miss) |
| potentials (voltage modes basis size) / currents (cells) | 34121 / 101736 | 33496 / 99680 |
| retained modes / `reservedModes` / `modeCap` | 67 / 96 / 163 | 67 / 256 / 323 |
| `plannedBytes` of the mixed plan (`P7_MIXED_MEMORY_STAGE`) | 1311734464 (1251 MiB) | 3204319744 (3056 MiB) |
| training iterations | 2 | 2 |
| peak private (20 ms sampled) / peak RSS | 4045 MiB (4241391616 B) / 3669 MiB | 6258 MiB / 4718 MiB |

Change in the peak: +2213 MiB. Accounted for: the mixed plan +1805 MiB (reserved modes 96 to 256, `P7_ENRICH_MAX=64`; about 11.8 MB per reserved mode, 160 more modes) plus the H-matrix +171 MiB (both numbers are logged), 1976 MiB together; 237 MiB remain. The 1.23 GB cache file is written from memory after the build, a transient copy of the same order is plausible but the logs have no sample at that moment, so it cannot be tied to the peak. The mesh got smaller (merge removed 203 free and 418 Steiner vertices), which does not explain growth. The number of enrichment iterations is the same (2 outer iterations; the model is returned at the first checks), so extra iterations are not the cause: the allocation is a reservation sized by `modeCap` before the first iteration, which is why the memory rose while the work did not. Conclusion: the dominant cause is `P7_ENRICH_MAX=64` raising `reservedModes` (and so the planned mixed bytes) by about 1.9 GB; the H-matrix growth (+0.18 GB, more unconverged pairs with the merged mesh) is secondary; the last 0.24 GB cannot be assigned from the logs. The planned bytes are reserved, not necessarily touched (RSS grew by only 1.0 GiB while private commit grew 2.2 GiB). A cheaper cap (`P7_ENRICH_MAX` below 64, or a smaller reservation tied to the iterations actually used) is the lever; not tried here.
