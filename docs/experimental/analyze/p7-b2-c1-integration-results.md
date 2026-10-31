# P7 B2→C1 integration corrections — 2026-10-31

Status: R1/R2 corrected; automatically selected original STM32 U5→J5 section passes actual board adapter C/C0/L/network and three genuine sensitivity controls. This is conditional local PEC section acceptance. Whole-channel distributed interfaces, finite-R return current and D/P7 remain open. No native/solver/public DSL/dependency/original-board/CHANGELOG changes or commits.

## Transport and admission corrections

R1: `extractWideBoardSectionCoefficients` checks positive finite length and sends it in `request.line.lengthMm`. It validates fixed slice origin/direction/reference contact, preserves physical source/region/layer mapping and includes complete board/stack/ports in audit identity. A per-invocation map reuses identical authoritative board slices (three actual slicing calls for six requested stages); existing bounded field/result cache remains the only persistent cache. Width holdout copper comes from its actual native slice, with field domain and dielectric contours taken from the final partner slice, so width comparison does not simultaneously move the exterior boundary. No cropped-width DC formula enters this path.

The adapter regression runs native on a finite physical coupon at1mm/2mm: identical per-length coefficients, changed complex response, exact requested frequency axis and zero new field assemblies for the second length. Zero/negative/NaN/infinite/missing lengths and absent stack refuse admission. An initially too-near exterior in this regression correctly remained unresolved; its corrected larger-air test retains the same finite original coupon.

R2: `validateSectionDomainPlan` checks real contours and requests before any field solve. Baselines are explicit internal steps, not certificates. Physical-width requires actual rectangular reference inclusion growth at fixed vertical placement, signals, materials, mesh and exterior. Air requires nested exterior growth, unchanged copper/mesh and only dielectric extensions attached to the original exterior; moving interior material boundaries is refused. Mesh requires strictly smaller triangle area and otherwise identical physics/domain. Simultaneous independent changes, narrowed width, translated reference, coarsened mesh and identical requests under changed axis labels are refused. Identical final request reuse is allowed through the existing cache but grants no new certificate by itself.

All three accepted comparisons terminate at the **same canonical final request**, using a six-step stencil: width holdout/final, air holdout/final, mesh holdout/final. Certificate sets are keyed by final physical request; successes in a different field domain cannot certify the current configuration. The2% criterion and1fF/m,1pH/m,0.01ohm,1e-4 complex-component floors remain unchanged. One/two-signal impedance spectrum support remains the bounded C1 scope.

## Finite B2 geometry and real automatic selection

The old inspected transverse extent grew to all board copper. `proposeCorridors` now inspects only the requested finite numerical window using complete physical-union contour segments, cutouts and bores. It retains every original source polygon and physical island outside that window; lateral boundaries remain numerical cuts, not physical plane edges. Existing full-channel admission and distributed-interface refusal stay intact.

`boardSectionCandidates` uses the unchanged selected physical signal/return islands and route graph. For every run, it repeats exact event checks at halfwidths2h/3h/4h (h is complete stack height), intersects the certified longitudinal intervals, and checks board material/cutout/bore invariance also in the maximum field-only exterior. No convenient slice samples establish homogeneity. Native board slicing then confirms the actual included conductor set/source mapping; any change between refinements refuses the coefficients path. Remote detours remain in source audit and D limitations. Intersected intervals are inset by1/8 at each end; deterministic choice is longest certified interval, then source id and coordinate, with unsupported conductor counts rejected before solving. The script uses this original-board selection and the corrected board adapter, without a user scope or substituted coupon.

Final STM32 selection:

- Original board SHA256 `0159c43d43b59df4f59fb943d59f0d841d17ca38d6d5a7d5576920a4aee3cce3`; original U5.7→J5.2 endpoints, D5/JP2 branches and negative terminals retained.
- Run `copper.fixed.tracks[56]/segment/0`, origin `[22.06374728679657,5.6639]mm`, direction `[1,0]`, transverse `[0,1]`, longitudinal endpoints21.937047511339188/22.190447062253952mm, length0.2533995509147644mm.
- Signal coordinates: conductor-0 is TOP tracks[45]/region-000306; conductor-1 is selected TOP tracks[56]/region-000327, signal island-000052. Both remain independently coupled coordinates.
- Physical return: BOTTOM `copper.fixed.zones[0].filledPolygons[0].polygons[0]`, region-000233, island-000001. No net-name join. Its retained source is wider than each numerical inclusion.
- Port order input-0/input-1/output-0/output-1, common physical return,50ohm reference,1/10/100MHz. This is a local uniform PEC extrusion, not an independently validated real STM32 response or a whole-channel certificate.

Final packet `debugging/p7-closure/1793436289092789600` includes original boardJson/stack, source mappings, chosen physical contacts, all staged slices, cold/warm results and OS supervision. Earlier successful packet1793436031671254400 predates the local slice-reuse optimization; final rerun validates the changed adapter, not a different numerical producer.

| Comparison, identical final configuration | C max | C0 max | L max | Z max | complex response max |
|---|---:|---:|---:|---:|---:|
| Included return halfwidth4.8→6.4mm |0.9202%|0.7726%|0.7931%|0.6604%|1.1804%|
| Air margin3.2→4.8mm; same included copper |0.4763%|0.7299%|1.5504%|0.8483%|1.9640%|
| Triangle area8e-8→4e-8m²; same full geometry |0.4151%|0.3227%|0.2275%|0.2217%|0.2559%|

Complete matrices, with no mutual entries discarded:

C F/m = [[5.814450945896188e-11,-2.9398917944980216e-11],[-2.9398917944980203e-11,5.828379720566458e-11]].

C0 F/m = [[2.2913687870286035e-11,-1.3102271721843916e-11],[-1.3102271721843919e-11,2.2946078701217306e-11]].

L H/m = [[7.209898898522765e-7,4.116871369854448e-7],[4.116871369854448e-7,7.199721359288524e-7]].

Complex four-port response is retained in real-result.json, including nonzero near/far-end coupling. No independent STM32 physical accuracy bound is inferred from these numerical sensitivities or from the small reference.

## Resource evidence and other boards

| Final real-board measurement | Result |
|---|---:|
| Geometry capture, exact candidate checks, native candidate preview |4.1101s|
| Cold adapter including slicing/cache/field/propagation |6.9319s|
| Cold end-to-end geometry + adapter |11.0420s|
| Same-input warm adapter, including authoritative slicing |3.6812s|
| Overall cold+warm work including artifact serialization |14.8944s|
| Supervisor process wall including startup |15.5470s|
|20ms sampled process-tree RSS / private peak |559689728 /601235456B|
| Native board slicing calls per adapter |3 (six stage requests)|
| Section executor calls / cold field assemblies / final-result cache hits |6 /4 /2|
| Warm section result hits / new field assemblies |6 /0|
| Final nodes / triangles / factor values |28451 /55451 /661286|
| Factor value bytes, excluding symbolic/workspace |5290288B|
| Native electric memory plan |96161007B|

Cold stage solve/cache wall:1.3668/1.1883/0.7308/0.2326/0.7624/0.2302s. Remaining adapter time includes native board slices and transport. OS peak covers startup/cold/warm/serialization; sampling is a lower bound, not solver-only live memory.180s/1024MiB internal budget; external210s/1536MiB with1GiB headroom reserve (initial commit headroom20030611456B). Native addon unchanged SHA256 `f0cd8d66a2a190ee32918cc8b73c7669b35a5e9b568640713d9afc166258cc85`. No whole-board magnetic solve or memory saving claim.

Geometry-only corpus `debugging/p7-b2-c1-geometry/`:

| Original channel | Before admitted | After finite candidates | Candidate length | Geometry wall |
|---|---:|---:|---:|---:|
| STM32 U5→J5 |0|4|0.6354457mm|3.0724s|
| NanoScope |0|0|0|34.6286s|
| ThunderF722 |0|0|0|13.6752s|

These geometry walls include capture/planning/candidate checks before render; Node cumulative RSS is retained, private is null for this separate geometry-only run. NanoScope and Thunder remain blocked by complete physical-union copper/bore event coverage inside the selected finite windows, not by sweeping all remote board edges. Saved events include narrow interruptions and branches; several NanoScope/Thunder runs also have bore events. Zero candidates do not imply electrical absence of coupling or authorize a fallback. Their internal copper-gap material limitations were not bypassed, and no EM solve is claimed.

## Regressions, plots and retained unsuccessful attempts

47 focused tests pass across section-domain, wide-return-domain, run-corridors, board-channel-plan and all three A closure guard suites;2 separate real-native adapter tests pass. Negative cases retain narrow internal slot, external slot/detour re-entry upon expansion, bore/branch, physically separate same-name islands, physical source subdivision and rotated geometry. Existing full STM32 planner still refuses distributed lateral interfaces and preserves D5/JP2 witnesses. Typecheck/build pass. No local-volume battery was run.

Independent image-reference coupon rerun through the new common-final admission: packet1793436260163135300 pass. Width/air/mesh maxima1.3349%/0.5375%/0.6188%; independent absolute C2.7270%, L1.1933%, Z1.4416%, complex S1.3072% plus unchanged3% engineering uncertainty<8%. Existing hash-bound A stripline accuracy/geometry-placement/comparator guards pass; their native producer/reference were unchanged, so no new A field solve was needed.

Retained real attempts:1793435788858911300 at area2e-8 hit existing polygon refinement cap; no node/memory budget increase. Changed area4e-8 and width1.6→3.2mm (1793435855501907700) failed real width sensitivity up to10.81%. Expanded3.2→4.8mm (1793435938971635600) still failed response2.996%. Only the further geometry-certified4.8→6.4mm expansion passes. No unchanged numerical failure was repeated; all failed attempts remain refusal evidence. No tolerance was relaxed.

`plot-real.py` rendered/opened final real-section.png: original full copper and selected contacts, refinement widths, native physical return, joint C/L, real frequency response and three sensitivities. Geometry PNGs for all three boards show full original copper/contacts and finite candidates; NanoScope/Thunder show no accepted region. These are section geometry/model figures, not a physical-error certificate.

Commands: `node --import tsx scripts/verify-p7-wide-return-domains.ts`; existing `verify-p7-closure-supervised.py` with `P7_SUPERVISOR_TEST_COMMAND=["node","--import","tsx","scripts/verify-p7-wide-sections.ts"]`, `P7_REAL_BOARD=1`, `P7_REAL_SOLVE=1`; same command without real flags for independent coupon; `python tests/fixtures/analysis-p7-wide-section/plot-real.py <packet>`; focused node tests; `npm run typecheck`; `npm run build`.

Remaining: distributed section/local interfaces and remote magnetic coupling, finite-R longitudinal return/current distribution, independently measured/solved real-board accuracy, multiple-reference/floating/fence cases, broader geometry selection and physical bandwidth/loss admission. Successful C/L does not justify1/(sigma*w*t) or mu*t/(12*w) for a cropped wide plane and adds no global plane energy. Stop for review.
