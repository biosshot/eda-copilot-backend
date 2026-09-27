# Telemetry: ordering experiments, 2026-09-27

30 offline runs: six variants on five local subtrees. Open comparison.html for aligned before/after SVGs, insertion order, direct passive-to-IC distances, normal and hidden-net metrics. No full board or live EasyEDA changes.

## Controlled variables

All runs: full profile, beam width 4, micro routing, pad crossing penalties, current candidate generation and local access fixes, group relaxation `all`, postrefine enabled, portfolio disabled, cache disabled. The base reproduces the preceding access-all layouts. Ordinary placement scoring and hard constraints are unchanged.

Only the ordering flags differ:

- base: previous electrical frontier.
- equal (`PCB_BLOCK_ORDER_EQUAL=1`): every explicit critical pair contributes 30 instead of hard pairs contributing 100. Does not relax hard constraints.
- core (`PCB_BLOCK_ORDER_CORE=1`): shared-net contributions to a placed core multiplied by 4; connections and critical bonuses through other placed components multiplied by 0.25.
- branch (`PCB_BLOCK_ORDER_BRANCH=1`): expand the top three next-component choices; retain up to three different placed subsets, four position states per subset. This has a larger search budget. Selection between subsets still uses partial cost and remains heuristic; this is not exhaustive search and need not retain the baseline trajectory.
- scarcity (`PCB_BLOCK_ORDER_SCARCITY=1`): up to 60 ordering points for the fraction of blocked near-IC candidate poses when some remain legal; 40 per additional IC-connected pad, capped at two additional pads. Nearby means total direct distance within one mm per connected pad of the best legal core-only candidate. Counts are based on generated poses, not exhaustive free-space analysis. Does not enforce adjacency of the target IC pins or add a direct-wire constraint.
- combined: all four flags.

New flags default to zero. Previous placement improvements remain enabled. The experiment runner explicitly sets every flag. Native contract includes them for cache correctness. No automatic best-of-these-six selection was installed.

## Findings

C9: base distances to U2.6/U2.7 5.33/4.63 mm. Equal unchanged; core 12.21/12.22; branch 6.31/6.26; scarcity 6.32/6.45; combined 6.00/7.07. Moving C9 earlier did not fix its final position. This does not identify whether promising candidates were missing, pruned by beam scoring, or moved during refinement; intermediate stage geometry was not captured.

LTE: core reduced normal-net MST 44.54 to 37.55 mm, line crossings 2 to 0, pad hits stayed 0. With ignored nets displayed, pad hits increased 3 to 7: this is not an unconditional routing-quality win.

USB: combined reduced normal-net MST 32.05 to 30.19 mm, line crossings 1 to 0, pad hits stayed 0. Area increased 150.57 to 180.93 mm2. With ignored nets displayed, crossings increased 3 to 4 and pad hits 4 to 6.

ADC: combined worsened MST 76.45 to 91.75 mm and line crossings 3 to 7 despite reducing normal-net pad hits 7 to 5. Logic power unchanged. There is no universal winner, so the new rules remain experimental.

All 30 local geometry/inventory/orientation/hard-pair checks passed. Rust: 52 passed, one preexisting ignored; targeted TS: 9 passed; typecheck and build passed. These are placement checks, not routing or complete-board validation. Timing includes parallel execution and is not a controlled performance benchmark.

## Reproduce

From the backend root after `npm run native:build`:

```powershell
node --import tsx scripts/experiment-telemetry-ordering.mjs
node --import tsx scripts/experiment-telemetry-ordering-report.mjs
```

Optional block names follow the runner command. `PCB_EXPERIMENT_VARIANTS` selects comma-separated variant IDs. Regenerate the full report only after all 30 result files exist.
