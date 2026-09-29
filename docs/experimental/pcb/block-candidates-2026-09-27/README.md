# Candidate and pad-crossing experiments

Open `comparison.html` for three block comparisons and all three complete boards. The selectors expose five separate runs, not automatic operating modes.

Research-branch defaults: corrected pair clearances, expanded pad candidates, local block postrefine, and additive foreign-pad penalties. The micro-router remains enabled. The pad penalty is a soft cost, not a routing prohibition.

Reproduce after `npm run native:build` and `npm run build`:

```powershell
node scripts/experiment-block-candidates.mjs Telemetry
node scripts/experiment-block-candidates.mjs ESPower
node scripts/experiment-block-candidates.mjs esp32c3
node --import tsx scripts/experiment-candidates-report.mjs
```

A/B controls: `PCB_BLOCK_CANDIDATES=0|1|2` (old / corrected clearance / expanded), `PCB_BLOCK_POST_REFINE=0|1`, `PCB_PLACEMENT_PAD_CROSSINGS=0|1`. Defaults: 2, 1, 1. Legacy disables these additions.

Images and intersection counts use MST connections, including multi-terminal nets, excluding configured ignored signals. Each segment/foreign-pad pair counts separately; same-net pads are exempt. `pairSum` and `pairMax` cover only two-terminal nets; `wireLength` covers the MST. Board placement validity does not establish routability. Timings are single samples with concurrent experimental processes.

The final USB layout improves substantially, but the changes do not dominate every earlier result: ESPower's charger regresses, and whole-board Telemetry two-terminal length increases while pad intersections decrease. All 15 layouts passed placement validation and preserved fixed components. Input and binary hashes, before/after metrics and local-refine diagnostics are in `measurements.json`.
