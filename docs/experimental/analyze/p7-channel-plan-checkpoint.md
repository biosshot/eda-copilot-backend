# P7 real-channel implementation evidence — 2026-10-29

The closure plan is still active. The accepted P6/P7 integration and saved 51-drive evidence are retained; none of the additions below certifies every real PCB channel within 5%. The original `761b1a9` planning checkpoint has been superseded by these implementation changes.

| Checkpoint | Implemented and checked | Remaining acceptance |
|---|---|---|
| A | Ordinary channel without authored scope; confirmed physical endpoints, branches and surrounding captured copper; finite candidate domains instead of mandatory whole-plane volumes; shared planning/preparation/solve deadline; complex-voltage and total-loss expansion gate | Positive full real-board automatic domain admission; long homogeneous route segmentation; explicit path constraints |
| B | General constrained polygon P1 FEM, holes, regional dielectrics, finite physical reference, up to16 independent signals; full C/C0/L and generalized modes; smooth-surface R/internal L, regional G, exact coupled complex waves; physical board slicer | Floating/electrically connected multiple references; automatic longitudinal change detection and production channel integration; independent full accuracy matrix for all supported geometries |
| C | Source-preserving finite contact consistency; stable interior Cu-face terminal convention; interior-via continuity, separate return and Joule/KCL test; original STM32 request unchanged | Automatic section/local composition, ownership and common-band return continuity; positive complete slot/pad/bend/RF channel matrix |
| D | Conditional arbitrary-section loss physics, skin/material/wavelength screens and passive/reciprocal network checks; independent 1GHz coax loss/propagation and electrically small10GHz control | Local 3D HF tangential-current trial space, skin-transition overlap, continuous DC/low-frequency admission and causal transient spectral admission; positive real TPS time and BandAmp complete RF channel |
| E | One bounded1..4 pool; immutable factors shared between independent RHS; serial inner kernels, per-job scratch and retained-state admission; immutable geometry snapshot and board-independent explicit-section cache identity | End-to-end1/2/4-job cold measurements; segmented field/model reuse, length/axis-independent section-factor reuse; complete real cold/warm/restart matrix |

## STM32 diagnostic decisions

The native diagnostic origin is explicit: `geometry/mod.rs` emits `drill_topology_uncertain` when enclosing drill approximation changes component count; it retains enclosing drills rather than declaring a confirmed open. `geometry/topology.rs` emits `contact_uncertain` for an overlap below reliable representation accuracy and does not join that pair.

The actual selected endpoints are signal via `da3574692d072a0d` at(23.7483,3.9469)mm, TOP/BOTTOM, and return via `aa8ff23739620a07` at(22.9483,3.9469)mm. Their source polygons, confirmed plated span and reliable area/barrel witnesses are retained. Default-tolerance evidence has6 reliable contacts incident on these selected source regions; they do not rely on any uncertain contact. An earlier v2 witness query accidentally selected the first region's island rather than the positive source's island; use corrected v3 provenance, not that historical witness count.

| Diagnostic/source | Actual objects | Decision and impact |
|---|---|---|
| Drill uncertainty at pads0..3 | MH1..MH4 mounting-hole pads at board corners | Ignored for this local request: outside all three candidate domains; physical holes remain in source geometry |
| Drill uncertainty at tracks415/418/460 | GND tracks `9e2aa177b8a3fedf`, `8f4f29c75196f3ba`, `209faac2ef5e5a2c`, away from selected vias | Ignored for the bounded candidate domains; no inferred GND join or removed drill |
| Contact uncertainty at pads158/160/191 | U3.14 RS485_RX, U3.16 GPIO_PA5, U3.47 CAN_RX | Ignored outside candidate domains; uncertain pair never creates connectivity |
| Contact uncertainty at pads193/202/204/142 | U4.5 +3V3, U5.6 RS485_A, U5.8 +3V3, U2.1 +5V | Same decision; none is a selected physical port |
| Contact uncertainty at track163 | `e4d1d910f857270d`, RS485_TX | Same decision; native source gap/contact geometry preserved |
| Unknown CAD fill freshness | Captured filled polygons, including selected surrounding plane | Explicit snapshot assumption; conditional result, no claim of current CAD fill or certified5% |

The bounded curve-sensitivity inspection changes20 topology warnings at1um to6 at0.1um, without changing selected physical port sources. This resolves neither every remote contact nor the final observable accuracy. Raw source objects and per-diagnostic reasons are saved in `debugging/p7-channel-plan/stm32-diagnostic-sensitivity-v3.json` (default source snapshot) and historical v2 (two tolerances).

The first native automatic domain is[[22.2987501025,3.2973501086],[24.3978500962,4.5964499116]]mm;11 captured region bounding boxes intersect it. Three finite environment candidates are shown in the inspected `stm32-automatic-domains.png`. The whole connected plane envelope is audit metadata, not the required volume domain.

Full contacts exposed a source-union discrepancy: derived BOTTOM copper omitted2.2979511627e-9mm² of already prepared source copper, a1.0780239339e-8 relative contact gap. The bounded reconciliation restores that exact source polygon only if bore overlap is zero, component/hole counts are unchanged and original1e-8 material/contact guards remain satisfied. Measured resulting contact error5.04e-16 and material area error4.46e-10; no hole or topology repair. Original explicit STM32 geometry requires no reconciliation. An interior Cu face excludes barrel metal continuing through that face from its exposed terminal; the barrel remains connected and its DC KCL/Joule checks pass.

## Numerical evidence

`polygon-coax-reference.json` records independent analytic references: capacitance error0.15137%, delay relative error2.15e-14, surface resistance at1GHz error0.01044%, attenuation error0.05179%, phase constant error0.0003784%. Radial two-material coax capacitance error0.15972%. These are declared controls, not a5% whole-channel claim. The electrically small10GHz control includes the actual mesh-convergence and network passivity report.

The actual BandAmp RF_OUT_PORT track `d334a42d8fb15d25` is sliced at its midpoint. Four finite physical conductor pieces remain independent: the signal, two coplanar pieces and the explicit bottom reference. They are not replaced with a nearest microstrip. Full3x3 C/L, physical source mapping, P1 fields and a conditional1GHz R/G solve with explicit58MS/m copper and0.02 dielectric tangent are saved in `bandamp-captured-section*.json`. Those material values are authored hypotheses. One local slice does not establish uniform geometry along the track or model its end-pad/bend. The field/matrix PNG was visually inspected.

A full no-scope STM32 worker run with the unchanged legacy high-order magnetic configuration reached electric assembly and then refused magnetic symbolic storage:2,764,410,585 requested bytes versus2,075,843,048 available. Wall34.9176621s; the136,278,016-byte supervisor RSS is not worker RSS. Saved `automatic-run-1793278365782.json`. No1/2/4-job repetitions of the same pre-training failure are counted. The automatic path now requires two bounded polynomial patch levels of the same direct-FEM operator and retains the old configuration for authored ROI regression; the new full run completed the coarse candidate and refused the finer level at2,517,682,953 requested bytes. That attempt took113.6915744s and recorded980,312,064-byte worker high-water RSS, saved as `automatic-run-1793279648174.json`; no channel is admitted.

Focused checks: TypeScript typecheck/build and13 adapter/cache/domain tests pass. Native contact geometry3/3, response7/7, actual BandAmp section1/1, polygon4/4, legacy cross-section9/9 and legacy line-loss3/3 pass. Three larger `line_losses_review` debug cases timed out under concurrent compilation, with unchanged120s per-case caps; these retained failures were rerun in isolation with optimization:3/3 PASS in20.71s, with unchanged budgets/thresholds. Final ordinary native build PASS7m43s; addon SHA256 `cbaa78657f85b1ef0aa745afb0d49a7a2fc13ae63c8bbde4851829b80ba09424`.

Source commit `dbb6cdc` contains the polygon/physical-loss/slicer implementation. No push. User-owned closure-plan input and `native/analyze/debugging/` remain untouched. Missing positive real time/HF channel classes mean whole P7 remains open.


## Bounded training and cache measurements

One fixed level1 real STM32 candidate was measured in isolated sequential1/2/4-job runs, all on that same addon. These include native board geometry, electric/magnetic assembly, training, reduction, worker startup and response; they do not include the higher-level automatic environment/spatial admission. The candidate is not certified within5%.

| Jobs | Cold s | Training s | Worker peak MiB | Warm s | Restart s |
|---|---:|---:|---:|---:|---:|
|1|83.0791|50.5832|710.79|0.3458|0.6362|
|2|61.7352|30.4706|729.04|0.3666|0.6199|
|4|57.4534|22.7097|785.65|0.3405|0.6187|

All three produce bit-identical complex port responses and total losses; all memory/disk hits report zero assembly and zero training. Independent solves share the same immutable factors, each owns work buffers;4 jobs reserve236,920,386 extra bytes and report factorCopies0. The default remains serial. Artifacts: `training-jobs-{1,2,4}-*/evidence.json`, with raw inputs/results. The source-bound recovered TOP current/Joule PNG was generated from a verified disk hit and visually inspected; it labels the unadmitted candidate explicitly.

Public direct DSL also verifies identical explicit section reuse across STM32 and BandAmp attribution contexts: cold/memory/disk statuses, one cache key and assembly1/0/0, saved in `section-reuse-1793279818133`. This does not yet provide length/axis-independent field-factor reuse or automatic segmentation.

The initial low-frequency TPS developer probe was superseded: `Options.frequencies_hz` controlled the reduced output axis while the full verification loop still used the historical17-frequency axis. Its first300.076s attempt saved7 points; its second was explicitly interrupted after287.889s, with17 points total, all at1kHz or above. None counts as low-band validation, none is migrated, and `tps-low-band/superseded.json` records the actual frequencies. The explicit verification-axis helper and regression now preserve the exact legacy default while prioritizing requested positive ordered holdouts. A fresh requested-axis journal is required. Production band/transient admission remains unchanged.


## Corrected low-frequency continuation

The fresh `tps-low-band-requested-axis-v2` journal verifies the actual requested1/10/100/316.227766/1000Hz axis for two unit drives and their opposite drive:15/15 PASS. Max complex-port error1.13e-14, total-loss error5.23e-14, current-coordinate error0.13980%, cell-loss error0.005946%; original weak-coordinate and numerical guards are retained. The first supervised attempt stopped at300.0805s,1244.32MiB, after4 points; continuation resumed those4 and completed15 in204.3117s,1247.08MiB. Each call has its own unchanged300s/2GiB cap; cumulative time is not a single cold acceptance. No solved points from the superseded wrong-axis experiment or old51-drive campaign were migrated. This supplies new full/reduced evidence below1kHz, without independently certifying physical geometry, every intersample frequency, DC component-charge conditions or default transient applicability.

A later ordinary native build PASS7m00s, SHA256 `093575992dc2b13672a0597db62971502f943f485b8cdd814eba92048e98c5b5`, includes explicit verification-axis priority, per-source single-component ownership and a larger independent-solve scratch reservation. The1/2/4 timing table remains tied to its stated earlier addon; the allocation guards became stricter and the original physical operators did not change. Focused optimized native geometry/axis checks pass5/5.
