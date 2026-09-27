# Telemetry: fixed connector families and board spacing

Open `comparison.html`. This experiment does **not** establish a global layout improvement.

- Baseline: branch commit `b747758`, unchanged saved Telemetry input (154 components).
- Fixed J5 is included in local family search; grouped and released USB support hypotheses use the actual board polygon, holes, constraint regions and other fixed components as obstacles. J5 remains at its original pose in every retained variant. The USB family retained one comparable variant; the solver does not pad the portfolio with inferior candidates.
- Additional board clearance is soft, capped at 3 mm and reduced with estimated occupancy. Telemetry receives 0.69 mm. Strong explicit proximity and satellite anchors are exempt from the additional margin, never from physical clearance.
- Board packing compares comfortable and tight packings plus up to two alternative block portfolios. It retains the best completed-board objective, then checks alternatives in their neighbourhood. These are bounded hypotheses, not exhaustive combinations. All four hypotheses were tried here; hypothesis 0 won. Native beam/candidate evaluation used six threads.

| Measurement | Before | Full after | USB-only control |
|---|---:|---:|---:|
| Full-board MST length, mm | 2261.00 | 2392.86 | 2258.51 |
| Full-board line crossings | 249 | 299 | 253 |
| Full-board foreign pad hits | 240 | 296 | 235 |
| Changed fixed components | 0 | 0 | 0 |

USB alone: MST length 27.15 → 25.16 mm; foreign pad hits 5 → 1; line crossings 2 → 3. Median nearest gap between the original DSL blocks increased from 0.418 to 0.508 mm. Thus the spacing mechanism changes density, but the present full-board search gives a material electrical regression. No result has been applied in EasyEDA.

The control replaces only R30, R31, F1 and C41 in the baseline layout. It passes the full placement report and native change validation. It is an ablation, **not** the production board-packer result. Its ASM contains only those four components; the full before/after ASMs contain 144 movable components. All ASMs omit the outline, fixed mechanics, holes and copper.

Validation: 79 distinct TypeScript tests passed across the targeted suites; 52 Rust tests passed, one existing test ignored; TypeScript typecheck and build passed. Serial/parallel native packing produced identical poses and ranks with soft spacing. Tests cover anchored pose precision, additional fixed obstacles, rejection of a shifted anchor, explicit external pin anchors, sparse/dense spacing and previous placement constraints. Final placement reports pass; the native packing rank retains the same three pre-existing fixed-geometry violations as the baseline. No routing test was performed.

Replay snapshots are stored as `.json.gz`; uncompressed working copies are ignored. `experiment-telemetry-anchored.mjs` runs the complete pipeline. `experiment-telemetry-repack.mjs` reuses its unchanged local portfolios to rerun board packing and global refinement after board-only adjustments. `after.json.gz` is the initial complete new run; `after-final.json.gz` is the final four-hypothesis board run. The input and final native binary hashes are in `summary.json`. These runs used different local worker counts and are not a timing benchmark.
