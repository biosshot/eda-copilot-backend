# Telemetry: electrical frontier, pad access and relaxed groups

Open `comparison.html`. Five complete block families, ten controlled variants each; no full-board packing or live EasyEDA changes. `blocks.png` is a compact overview. The HTML also shows ignored nets and distances from passives to matching pins on the main IC.

## Findings

- C33 was not simply missing candidates. A legal position 2.01 mm from U10.3 lost to the 9.67 mm position even with micro-routing disabled. The main contributor was U10's external relation to U6: extending the block envelope added roughly 4508 score despite no additional foreign-pad intersections. `c33-probe.json` records the controlled fixed-neighbor grid probe and relation ablations. Its base score excludes the route correction.
- `island:` endpoints were unresolved in the native block solver. Island targets were therefore treated as missing external targets. New access scoring resolves them and distinguishes an unplaced internal endpoint from a truly external one.
- Electrical ordering and expanded candidates alone are not consistently better. Correcting the score made C33 reach 1.94 mm. Core pairs U10/L1 and U12/L2 retain their geometry and both connections remain within 5 mm.
- Releasing satellite/cap rigidity helps ADC and current isolation, but hurts the LTE result after access scoring is corrected. It must remain a competing hypothesis, not an unconditional rewrite.

Selected comparisons use the configured ignored-net list:

| Block family | Old local MST / pad hits | New score + ordering + candidates | Also release groups |
|---|---:|---:|---:|
| LTE power | 43.39 mm / 3 | 35.55 mm / 0 | 44.54 mm / 0 |
| Logic power | 22.80 mm / 4 | 19.96 mm / 1 | 19.96 mm / 1 |
| ADC, including satellites | 86.22 mm / 12 | 77.34 mm / 11 | 76.45 mm / 7 |
| Current isolation, including satellites | 53.37 mm / 5 | 57.39 mm / 1 | 41.96 mm / 1 |
| USB charge | 31.25 mm / 1 | 32.05 mm / 0 | 32.05 mm / 0 |

For current isolation with ignored nets also displayed, foreign-pad hits improve 14 → 3 when groups are released. C10.1–U2.12 improves 8.60 → 2.65 mm; C11.1–U2.12 improves 9.69 → 3.19 mm. ADC's best pad-hit count in the earlier score ablation is 4; this does not imply that every other metric also wins. Full tables retain regressions and unchanged trials.

## Implementation and scope

Code commit: `dd5662c`. Full-profile branch defaults now enable `PCB_BLOCK_FRONTIER=1`, `PCB_BLOCK_PAD_OWNER=1`, and `PCB_BLOCK_LOCAL_ACCESS=1`. Legacy/raw native fixtures retain their optional-field defaults. Rebuild the native addon when checking out this code.

`PCB_BLOCK_RELAX_GROUPS=off|satellites|caps|all` is an experimental representation control; default is `off`. It does not change the input netlist. Satellite anchors are distributed to members. Cap targets remain explicit; a cap cluster without an explicit row topology may be split. Core-pair, bypass, line, fixed and explicit row-topology geometry remains protected. Group-level hard distance constraints prevent splitting. Distributed weights retain the native minimum-weight clamp, which can strengthen very large split groups; Telemetry's tested groups do not reach that case.

New pad candidates use the owning component's box inside an island, all matching net anchors, and a larger spatially diverse shortlist before route evaluation. Electrical frontier ordering fixes the largest main IC first and prioritizes critical pairs and normalized local connectivity. Ground and configured ignored nets do not dominate that order. Without a main IC the old selection remains available.

The new external-access proxy measures occupied lengths along four straight escape directions, with same-layer bodies and opposite-layer through-hole obstacles. Free space contributes no cost. This remains a placement heuristic: it does not prove a routable external connection, inspect copper pours, or evaluate the source island's internal escape. Board-level spacing and full-board validation were deliberately not run.

Every variant uses the same input, full profile, micro-routing, candidate rings, direct pad penalties and block postrefine. The extra board-context block portfolio is disabled for every trial. Baseline is a fresh local subtree solve, not the old full-board SVG, whose portfolio could select different internal layouts. Timing comes from concurrent runs and is not a speed benchmark. Initial ablation native hashes were not captured; final native identity and this limitation are recorded in `manifest.json`.

## Reproduce

From the backend root in PowerShell:

```powershell
npm run native:build
node --import tsx scripts/experiment-telemetry-frontier.mjs
node --import tsx scripts/diagnose-c33-frontier.mjs
node --import tsx scripts/experiment-telemetry-frontier-report.mjs
```

Pass block names to the experiment script to limit the scope. `PCB_EXPERIMENT_VARIANTS` optionally selects comma-separated internal experiment IDs from the script. These are ablation switches, not user-facing placement modes.

Validation: all 50 local layouts preserve inventory, allowed layers/rotations, internal clearances, and tested local hard pairs. PCB suite: 163 passed, then the additional C33 end-to-end regression passed together with the three group tests (164 distinct PCB tests). Native suite: 50 passed, one pre-existing ignored test. Typecheck/build pass. All 100 comparison SVG references and embedded HTML JavaScript were checked; the contact sheet and headless HTML rendering were visually inspected.
