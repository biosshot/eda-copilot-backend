# Unified Telemetry block placement

The normal tree solver now uses the four accepted ordering improvements, grouped/released hypotheses, native beam/singles/pairs checkpoints and post-refinement of each legal checkpoint. Suspicious `decoupling_cap` roles with two connected nets that are not recognised as supply/ground are tried as `passive`, individually and together. Input roles, clearances and explicit constraints remain unchanged. A joint winner does not establish that each individual role was electrically wrong.

All finished candidates use one role-independent geometric/electrical objective. Local IC links pay `6*d + 32*max(0,d-3)^2`, with supply weight 0.25; multi-terminal MST edges also pay for excessive length. Area costs 0.35/mm², with modest perimeter and external-access costs. Coefficients are initial Telemetry calibration, not a universal optimum. `saved-c9-calibration.json` records the historical regression fixture: singles with C9 links 2.80/2.82 mm beats the smaller paired result with 8.65/7.52 mm links.

The final pool contains 1–3 comparable, nonduplicate layouts. Admission checks absolute hard constraints, inventory, orientation, layers and original clearances. Alternatives must remain within 12%+20 electrical score, 15%+1 weighted MST length, and per-IC-link growth of max(1.5 mm, 35%) relative to the winner. Shape and external-port differences preserve useful diversity. Board-context selection adds the same internal block objective to its board objective; it remains bounded coordinate descent, not an exhaustive global search.

The same-IC pad exception applies only when both segment endpoints are on one `main_ic`, and only to pads of that IC. Other-component pads and unrelated IC pads hit by external connections remain penalised, once per physical pad per segment. Actual routing obstacles remain intact. `internalIcPadHits` is shown separately from penalised foreign-pad hits.

## Results

Open [comparison.html](comparison.html). Its left panel is the grouped/original beam checkpoint in the current pipeline; its right panel offers the selected primary and retained alternatives. This is an intra-run stage comparison, not a rerun of legacy modes. The ignored-net checkbox changes visualisation and measurements only.

| Block | Initial beam MST → selected, mm | Line crossings | Foreign pad hits | Retained |
|---|---:|---:|---:|---:|
| current_iso | 39.61 → 35.20 | 0 → 0 | 1 → 0 | 3 |
| usb_charge | 33.43 → 30.19 | 0 → 0 | 0 → 0 | 2 |
| lte_power | 33.81 → 35.13 | 0 → 0 | 0 → 0 | 2 |
| logic_power | 20.65 → 19.85 | 0 → 0 | 0 → 0 | 2 |
| adc | 91.90 → 60.42 | 8 → 3 | 11 → 4 | 2 |

C9's selected links are 2.6727/2.67585 mm; C33–U10 is 1.936 mm. LTE's total MST increases slightly: the scalar objective includes individual local links and explicit relations, not just total length. The selected current_iso still has three immutable same-IC pad hits. Hidden/ignored nets introduce additional crossings; their counts are available in the report. These are straight-line estimates, not completed routes.

## Verification and reproduction

- Five Telemetry subtrees; all component inventories, rotations/layers, physical geometry and explicit local hard constraints pass. Input data was not mutated. Full-board Telemetry was not run.
- 91 TypeScript tests pass; 52 native tests pass, with one pre-existing benchmark ignored. Typecheck and package build pass.
- Mechanical families with fixed/edge components retain their group identities until board anchors are compiled. The fixed-connector and explicit-satellite-anchor regressions are covered by tests.
- HTML JavaScript, all 22 interactive combinations and 42 SVG files verified; rendered report visually inspected in headless Edge.
- Cold native searches were run for all five blocks. The final integrated replay reused the exact encoded native inputs after adding only the absolute-validation API. A direct old/new native USB comparison matched all checkpoint states and ranks exactly (`validator-compatibility.json`). Final result `captures` explicitly record the source binary hash for replayed native results. Final `ms` values are therefore **not cold performance benchmarks**. Post-refinement, strict admission, scoring and portfolio selection were rerun through the final tree solver.

Normal reproduction, without compatibility replay:

```powershell
npm run native:build
node --import tsx scripts/experiment-telemetry-unified.mjs
node --import tsx scripts/experiment-telemetry-unified-report.mjs
```

The harness has a binary/input-keyed local cache in `debugging/unified-native-cache`. Production flags for legacy/full/order/group A/B combinations are retired. Historical harnesses explicitly reject execution on this branch; replay those with their recorded historical commit. Low-level native search parameters remain explicit in the version-3 contract and cache keys for tests and diagnostics.
