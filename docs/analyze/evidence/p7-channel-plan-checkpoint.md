# P7 real-channel implementation evidence — 2026-10-29

The closure plan is still active. The accepted P6/P7 integration and saved 51-drive evidence are retained; none of the additions below certifies every real PCB channel within 5%. The original `761b1a9` planning checkpoint has been superseded by these implementation changes.

| Checkpoint | Implemented and checked | Remaining acceptance |
|---|---|---|
| A | Ordinary channel without authored scope; confirmed physical endpoints, branches and surrounding captured copper; finite candidate domains instead of mandatory whole-plane volumes; shared planning/preparation/solve deadline; canonical50Ohm power-wave/Cu-loss expansion gate with declared weak floor | Positive full real-board automatic domain admission; long homogeneous route segmentation; explicit path constraints |
| B | General constrained polygon P1 FEM, holes, regional dielectrics, finite physical reference, up to16 independent signals; full C/C0/L and generalized modes; smooth-surface R/internal L, regional G, exact coupled complex waves; physical board slicer | Floating/electrically connected multiple references; automatic longitudinal change detection and production channel integration; independent full accuracy matrix for all supported geometries |
| C | Source-preserving finite contact consistency; stable interior Cu-face terminal convention; interior-via continuity, separate return and Joule/KCL test; original STM32 request unchanged | Automatic section/local composition, ownership and common-band return continuity; positive complete slot/pad/bend/RF channel matrix |
| D | Conditional arbitrary-section loss physics, skin/material/wavelength screens and passive/reciprocal network checks; independent 1GHz coax loss/propagation and electrically small10GHz control | Local 3D HF tangential-current trial space, skin-transition overlap, continuous DC/low-frequency admission and causal transient spectral admission; positive real TPS time and BandAmp complete RF channel |
| E | One bounded1..4 pool; immutable factors shared between independent RHS; serial inner kernels, per-job scratch and retained-state admission; immutable geometry snapshot; measured1/2/4-job preparation; board-independent, length/axis-independent section coefficient cache with native physical revalidation | Segmented channel/local model reuse; complete admitted real-channel cold/warm/restart matrix |

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


## Automatic-domain validation revision (before new measurement)

Unloaded unit-current voltage contains the entire connected island's capacitive pole; a small local domain cannot certify that global observable. Automatic domain/spatial validation therefore explicitly tests canonical50Ohm power waves and intrinsic Cu Joule loss. These are mathematical reference terminations; no actual PCB receiver is inferred. Norton excitation d gives outgoing normalized wave(2V/50-d)/norm(d). The predeclared weak-amplitude floor is0.01(-40dB); change gate remains2%, loss floor1e-18W. Unloaded reactive Z, regional recovery and whole-channel5% remain unverified. The intrinsic returned network has no incorporated external resistor; loaded sample boundary conditions are retained explicitly.

A four-slab scope-free vertical-refinement hypothesis was measured and rejected: on the actual four-terminal STM32 it increased magnetic fill enough to refuse numeric-factor admission at2,191,028,731bytes against2,048,755,272bytes available. Saved `automatic-supervised-1793287303104228800`,23.3398s process wall,571,109,376byte process-tree peak (largest child451,227,648bytes); DSL/native wall18.8451s and native high-water454,340,608bytes use different sampling/measurement scopes. Geometry/contact/field checks passed. The production current extrusion remains the original2mm maximum; finer explicit/internal meshes retain their vertical boundaries in electric refinement. The old authored ROI request stays unchanged. Pinned faer symbolic memory admission counts actual shared supernode row indices and panels, checked before numeric allocation, instead of charging a full simplicial pattern for each supernode. These changes require new cold evidence; older failures remain valid historical results.


## Preserved final direct-FEM resource evidence for this revision

Ordinary addon build PASS22m12s, binary SHA256 `e1e68b2f4072e0769b26b26f0143d7816332a17229e18de97d8378b628257f47`. The unchanged2mm vertical mesh with corrected shared-pattern storage completed the actual four-terminal RS485_B first-domain coarse preparation (29voltage/38current modes, saved7.86MB cache entry). Fine trace6/body8 numeric storage requires3,082,548,473bytes versus2,067,234,792bytes available; no sensitivity result or channel is admitted. `automatic-supervised-1793287576423322800` records110.3211s process wall,1,097,433,088byte process-tree peak,1,020,162,048byte sampled largest child; DSL107.2698s/native OS high-water1,026,813,952bytes. The supervisor records the scenario failure separately from normal process exit. These are conditional candidate/resource measurements, not cold acceptance.

The lower trace5/body7 profile was checked once on the same independent complete three-source reference:0.6537886% operator error plus inherited0.6780636% reference screen totals1.3318522%, failing the unchanged1% gate. The trace6/body8 control remains0.9875288% combined and passes. A regression retains this distinction; the lower profile does not replace the required fine level. No budget or physical/numerical threshold was relaxed.

A read-only ranking of other captured STM32 via/return geometries was followed by one geometry-only preflight for RS485_DE. Its21arrangement atoms and7011current coordinates exceed the original19/6678; no additional full solve was justified or run. Its tiny source-union discrepancy was resolved from exact existing copper with unchanged bore/topology/area guards, not by joining a net or deleting a hole. Saved `stm32-additional-physical-seeds.json` and `additional-de-geometry*`. This is planning/geometry evidence, not another accepted channel.

Focused current checks pass: local physical adapters6, domain metrics4, native contacts3, preparation3, response7, electric field8, pinned sparse-layout control1 and independent magnetic reference1. The layout control covers upper and complete symmetric matrices and both symbolic branches; the source-space test also exercises cell-private ordering. Ordinary authored STM32 request and numerical guards remain unchanged. The complete positive no-scope STM32 channel remains blocked by the fine factor storage, rather than geometry uncertainty.


## Final verified source revision,2026-10-30

Section coefficient reuse, consistent polygon boundary mass recovery, bounded sharp-corner grading, and physical total/component loss-energy checks are implemented and tested; see `p7-section-field-cache.md`. Weak-energy eigenvalue clipping belongs only to the error metric and does not alter physical R/G. Numerical residual/passivity/current-normalization guards remain active. The general solver supports1..16 independent conductors and a finite physical reference; actual board slicing and full matrices do not imply an automatically composed whole channel.

The final ordinary addon build passes, SHA256 `943195ea6648fbb464041453becef412bc80170234512d1852174130a183e17f`. TypeScript typecheck/build and26 focused tests pass. Current native controls pass: arbitrary polygon4, coefficient replay3, analytic signed/weak loss metric2, legacy cross-section9 and line loss3. The historical contact/preparation/response/field and immutable-factor checks remain recorded above; unchanged expensive board sweeps were not repeated.

Current direct-DSL evidence `section-field-reuse-1793346191994` also covers explicit mesh controls with native-null normalization. New20/40/60mm,100/200/300MHz requests use one transverse preparation then memory/disk coefficients: call walls0.490/0.177/0.411s; worker OS high-water136,523,776/160,133,120/122,068,992bytes. First cold+warm scenario wall1.0537873s; restart0.7278058s. These call times exclude startup. Preparation includes all convergence solves; counts1/0/0 are preparation calls, not individual matrix factorizations. The reviewed PNG and full scenario code/reports are retained. Original preparation timing56.6605ms and original100MHz axis now survive repeated validation.

The actual captured BandAmp loss case is still rejected by production, not replaced with a template: `captured-section-field-reuse-1793346472358` (1.3301025s call,186,875,904byte worker OS high-water) returns SECTION_CONVERGENCE with the same4.78607% physical loss-energy change against2%. The native producer's complete numerical trace, OS memory evidence and reviewed convergence PNG are in `captured-section-graded-1793345418709168600`. Static C/C0/L converges to0.0942864%; neither that number nor a smooth hypothetical uniform extrusion certifies RF losses or the end-pad/bend/return path.

Whole P7 remains unclosed. Required positive no-scope STM32 admission is blocked by its fine3.08GB factor; actual BandAmp loss convergence and complete RF-channel acceptance remain absent. Longitudinal segmentation/local ownership/composition, full common return coupling, general floating/multiple-reference conditions,3D HF tangential-current/transition operators and continuous DC/common-band causal accuracy admission are not implemented by these additions. The15 TPS low-frequency holdouts and conditional finite-front compatibility case do not substitute for those missing certificates. No complete real-board5% claim or arbitrary10GHz claim is made.

## Complete-channel continuation,2026-10-30

See [the continuation report](../archive/design/p7-complete-channel-continuation.md). Added an actual remote U5.7→J5.2 no-scope planning regression retaining D5/JP2 branches and BOTTOM copper, plus per-region inclusion/exclusion audit. Earlier same-via TOP/BOTTOM examples remain local transitions, not full remote channels. Explicit bounded developer memory/time verification no longer stops at the historical2GiB/300s guards; production defaults and physical band remain unchanged. Longitudinal electrical segmentation/composition and local3D HF operators remain absent; P7 stays open.


**Actual section/local continuation:** [implementation and independent evidence](p7-segmented-channel-evidence.md) records the real no-scope numerical slab/P6 path and retained delay failure. Historical statements that no assembler exists are superseded; complete P7 and P8/P11 physical admission remain open.
