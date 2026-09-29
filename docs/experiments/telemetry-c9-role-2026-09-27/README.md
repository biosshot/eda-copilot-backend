> Historical experiment: its retired runner was removed from the current branch. Use the recorded historical commit for exact reproduction; use `npm run debug:pcb-layout` for current captures and replays.

# C9 full role substitution — 2026-09-27

Open comparison.html. Four current_iso subtree runs, comparing decoupling_cap and passive under previous and experimental combined ordering. The only input change is C9.pcb.role, before graph construction, clearance calculation, encoding, placement and block postrefine. The source DSL, fixture, production defaults and live EasyEDA remain unchanged. No role detector or role portfolio is added.

| Ordering / role | C9 to U2 distances, mm | Normal MST, mm | Line crossings | Foreign pad hits | Area, mm2 |
|---|---|---|---|---|---|
| Previous / decoupling_cap | 5.33 / 4.63 | 41.96 | 1 | 1 | 399.82 |
| Previous / passive | 4.74 / 2.93 | 36.86 | 0 | 1 | 404.57 |
| Combined / decoupling_cap | 6.00 / 7.07 | 43.67 | 1 | 3 | 396.66 |
| Combined / passive | 8.65 / 7.52 | 45.74 | 1 | 3 | 456.03 |

The old ordering benefits on the measured normal nets, but with ignored nets displayed foreign pad hits increase 3 to 5. In the combined case ignored-net crossings also worsen (6 to 7 line crossings, 7 to 10 pad hits). This is not a universal quality improvement.

Full role substitution sets native powerComponent to false and increases C9-U2 clearance from 0.5075 to 0.6125 mm. Group membership and electrical connectivity are preserved. Therefore it differs from the preceding experiment that changed only the native power flag. Four ordering experiments, micro routing, candidate rings, pad penalties, group relaxation all, portfolio off and cache off are held constant within each before/after pair.

Combined/passive puts C9 at 2.80/2.82 mm after beam, and preserves that through single-component local improvement. Pair improvement then moves it to 8.77/7.38 mm; postrefine rotates it and yields 8.65/7.52 mm. The role mismatch is real, but changing it does not remove the later score tradeoff. These results do not justify introducing automatic role enumeration as the next general remedy without addressing that tradeoff.

All four runs pass local inventory, orientation, geometry and hard-pair checks. Both original-role controls reproduce the preceding traces to 0.00001 mm after U2 normalization. The runner asserts that restoring the original role yields the exact original input object, proving no other fixture edits. It records source and effective input hashes and the native binary hash. No production solver code changed for this experiment; validation consists of these controlled reproductions and layout checks.

Reproduce from the backend root:

```powershell
node --import tsx scripts/experiment-telemetry-c9-role.mjs
node --import tsx scripts/experiment-telemetry-c9-role-report.mjs
```

summary.json contains compact results and stage distances. measurements.json contains placements and native captures. Compressed stage-event traces retain beam survivors, local decisions and accepted pair changes; complete candidate logs remain under ignored debugging/telemetry-c9-role. HTML offers both ordering families, four stages, and an ignored-net toggle. Distances and MST are straight geometric measurements, not routed wire lengths. No complete-board test was run.
