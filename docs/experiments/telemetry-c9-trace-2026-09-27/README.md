# C9 placement trace — 2026-09-27

Open comparison.html. Only the Telemetry current_iso subtree was run. Production candidates, scoring, defaults, input fixture and live EasyEDA were not changed. Instrumentation is opt-in with PCB_BLOCK_TRACE_C9=1 and emits JSONL to stderr. Trace archives are gzip-compressed JSONL.

## Findings

Combined ordering: ten legal poses with both C9-to-U2 distances <=3 mm exist at insertion on the winning parent subset U2,C13,C14. Four survive the route shortlist and micro ranking. The close pose survives the entire beam: 2.67/2.68 mm.

The first C9 local_improve move changes its rotation 90 to 0 and distances to 5.20/3.39 mm. Hard geometry count stays zero. Base score decreases 18544.2260 to 17862.3321. Exact decomposition:

| Term | Change |
|---|---:|
| power_yield | -511.8379 |
| foreign pad crossings | -180 |
| signal spread | +9.944 |
| all other base terms | 0 |
| micro route correction | -0.118 |

Thus the initial loss is primarily a score issue, not candidate absence, beam pruning or micro routing. The later C9+R7 pair move increases distances again; final block postrefine rotates C9 by 180 degrees, yielding 6.00/7.07 mm. Its reported effective gain is just 1.325. These are different optimization stages and should not be conflated.

The input role of C9 is decoupling_cap, explicitly set in telemetry-design/pcb-placement-placer.js, although its pins belong to I_FILTER_P and I_FILTER_N. isPowerComponent accepts that role without checking those nets. power_yield excludes such primitives from signal endpoints and treats them as obstacles to signal corridors; it does not exempt the obstacle's own nets. This permits self-repulsion of a signal capacitor.

A controlled third run changes only C9's native powerComponent field to false. It retains 2.70/2.72 mm through local_improve. However C9+R7 pair optimization later moves it to 5.23/5.36 mm. That move reduces relation penalties by 420.0942 and facing by 127.2715, increases pad penalties by 180 and signal spread by only 26.004; remaining geometric terms reduce the score by 703.4720. Total base improvement is 1044.8337. Correcting the role is therefore necessary to remove this misleading penalty, but is not a complete fix for close placement.

The original ordering behaves differently: C9 is inserted seventh, no generated legal candidate has both distances <=3 mm, and global beam keeps ~12.18 mm. Pair optimization subsequently improves this to 5.33/4.63 mm. It would be incorrect to say that refinement always harms C9 or that every run loses it for the same reason.

## Scope and validation

Three local runs; all inventory, orientation, geometry and hard-pair checks passed. Base and combined final poses reproduce the preceding experiment to 0.00001 mm after normalization around U2. Native unit tests: 52 passed, one preexisting ignored. Raw candidate distances use native connection points; rendered/table distances use transformed footprint pads and can differ by sub-micron rounding.

The counterfactual changes the native power flag only; it does not edit the source role or introduce a placement hint, freeze C9, disable refinement or change micro routing. It also changes whether C9 contributes endpoints to power_yield, so it is a classification ablation rather than a subtraction of a single fixed scalar penalty.

No general fix is enabled. Follow-up work should validate role against connectivity, avoid own-net repulsion, and compare the weights of explicit pin relations against ordinary multi-terminal net length. Forcing C9 to stay fixed would hide these underlying issues.

## Reproduce

```powershell
npm run native:build
node --import tsx scripts/experiment-telemetry-c9-trace.mjs
node --import tsx scripts/analyze-telemetry-c9-trace.mjs
```

Runner emits raw worker logs under .test-output/telemetry-c9-trace. The analyzer archives traces, renders seven stages per run and checks the two unchanged baselines. analysis.json contains every surviving beam state, candidate counts, close-candidate scores, local moves and accepted pair changes. summary.json contains hashes and compact measurements.
