# P7 route corridors, exact interfaces and bounded acceptance — 2026-10-30

Status: **partial implementation; STM32 / whole P7 not accepted**. Public DSL, P0–P15 scope, physical ports, material inputs and strict numerical guards remain unchanged. This report supersedes the initial BSP candidate handoff. Full real-board output/reference and the resource/accuracy goals are not supplied by geometry tests or coupon results.

## Implemented corrections

- **R1:** native connected multi-source finite contacts, layer/source/interval checks, exact overlap union, canonical identity, current-space boundary conditions and numerical conservation controls. Precisely one of legacy `sourcePath` or internal `sourcePaths` is permitted. Disconnected same-net pieces remain separate; no representative first-source shortcut. See [native results](p7-oriented-boundaries-results.md).
- **R2:** explicit run rectangles replace the retired global-axis slabs and rejected BSP automatic path. Reliable source connectivity preserves all branches. Degree-two collinear source subdivisions coalesce; overlapping parallel/meander legs share a coupled transverse extraction. Full source copper, finite return, neighbouring conductors and off-diagonal matrices survive. Connected event pieces are solved jointly; arbitrary rectangular remainders cannot become sections. Unsupported distributed lateral copper continuation is explicitly refused.
- **R3:** complete physical layer-union, board exterior, cutout and bore boundary segments determine longitudinal events. Features between convenient slice locations cannot escape. Native slices supply actual cross-sections only inside proven invariant intervals. Tests include varying board edges, internal cutouts, source subdivision, rotation, bend, meander and coupled neighbours.
- Ownership records carry original source polygons, explicit clipping domains, physical region/layer and area. Validation independently reclips original copper, checks area, coverage and pairwise domain nonoverlap. Automatic ownership is a discriminated polygon-fragment record with area in mm²; legacy authored interval fixtures retain length in mm. No area is encoded as a length. Plated-wall envelopes must belong wholly to one joint local source domain, with native bore/wall guards authoritative. One bounded immutable JS geometry snapshot is shared by DSL planning/refinements; native sessions still own internal preparations.
- Final response admission additionally compares cut placement fractions1/8 and1/6 through the unchanged2% wave/loss/delay gate. These fractions are not an accuracy guarantee without the response check. Existing mesh/domain gates, common band1–10kHz, loss/skin checks and P6 conservation/passivity/reciprocity checks remain.

## Agreed internal transport

`domainPolygonMm?: [number,number][]` is a simple absolute XY polygon in mm, with existing ROI as bounds. A cut retains the rectangular special case or `{originMm:[x,y], normalXY:[nx,ny], spanMm?:[min,max]}`. Tangent is `[-ny,nx]`, with span relative to origin. Electrodes retain exact source/layer identity. TS cache keys canonicalize sorted/deduplicated source sets, including a legacy singleton path; malformed dual representations are not repaired. Native core/band/geometry/port identity remains exact. No public DSL additions.

## STM32: specific unsupported interface, not a memory success

Reproduce the actual U5.7→J5.2 board/contacts/materials/branches planning with:

```
node --import tsx scripts/inspect-p7-run-corridors.ts --full-return-support
node --import tsx scripts/render-p7-run-corridors.ts
```

Saved `debugging/p7-channel-plan/corridor-refusal.json`, inspected `.png`, and `full-plane-corridor-audit.json` preserve the evidence. Sixteen authored signal segments form15 physical runs after one degree-two source merge. Coupled grouping may join overlapping parallel projections; every constituent still has a lateral GND obstruction. D5/JP2 branches and bottom-layer signal regions remain recorded.

With the original stack/environment, proposed half-width is3.327mm. BOTTOM source `copper.fixed.zones[0].filledPolygons[0].polygons[0]` continues through corridor sides. Track48 is3.2201mm long; GND crosses its two sides over3.2201mm and1.96191933mm. The present section model has endpoint terminals, so it cannot retain those distributed side currents. No insulated ribbon or perfect-ground substitute is admitted.

Enlarging each run to contain the complete captured finite return transverse support removes lateral contacts but yields **zero uniform intervals on all15 physical runs**, with119–2481 complete boundary events per run. This quantifies the obstruction for the present narrow/whole-return choices; it does not prove a more capable distributed-plane/interface model impossible. The planner raises `CHANNEL_CORRIDOR_UNSUPPORTED` before extraction instead of executing an eight-local fallback or dropping the plane.

There is no completed STM32 response, matching independent real-topology reference, or meaningful warm/restart model cache yet. A planning refusal cannot establish physical error or low-memory completion. The old real F64 baseline requested8,714,661,076B at numerical-stage admission; it was a **preallocation refusal**, not a successful measured8.715GB solve. No measured speedup or memory-saving ratio is claimed against that estimate.

## Completed latest-binary straight coupon

Packet: `debugging/p7-channel-plan/automatic-supervised-1793393382999381200` (latest frozen addon and current explicit fragment ownership). The earlier one-job packet `1793388741778925700` is retained separately.

| Measurement | Result |
| --- | --- |
| Internal cold / same-worker warm |142.079743s /1.494908s |
| Supervised cold+warm wall |144.847106s |
| Peak tree RSS / private commit |521,629,696B /460,304,384B |
| Native / tree commit / host reserve limits |2GiB /3GiB /1GiB |
| Per-call deadline / training jobs |600s /4 |
| Cold/warm matrices |bit-identical; warm local assembly/training zero |
| Mesh sensitivity max / domain / boundary placement |1.523770% /0.811532% /1.772425% |
| Final P6 residual / KCL / real-power error |8.711e-10 /5.039e-16A /1.202e-10 |

Windows commit supervision was explicitly enabled; no pagefile settings changed. The mode records commit headroom and private bytes, not proof that OS paging occurred. Cold exceeds the120s goal. This is the small ordinary no-scope DSL, with real local FEM, finite distinct return terminals, polygon FEM sections and P6 output; it is not STM32.

`audit-p7-segmented-reference.ts` compared against saved whole-volume packet `automatic-supervised-1793363430965924500` (same physical coupon/ports/materials;0.075mm electric-edge reference). Differences: complex S0.000574507%, phase delay4.994104%, copper loss0.000711407%. Reference0.15→0.075mm delay sensitivity was about0.32%; that is convergence evidence, not a rigorous absolute reference-error bound. The rectangular single-source reference path was unchanged by oriented-union work. This supports the low-band coupon comparison only; it does not certify STM32 or sum separate error allowances into an8% certificate.

Actual comparison/matrices and the inspected response figure are in `whole-reference-comparison.json` and `corridor-response.png` inside the final packet. Earlier packet `1793386624165283500` completed before the final precision/P6 changes (205.626s,535,162,880B RSS), and `1793386976893238900` restored it in a fresh process plus repeat (3.885s total); their identities remain separate from final-binary results.

## Memory lifecycle evidence

`P7_BLOCK_BEGIN/END` brackets each extraction; developer evidence includes native preparation/reduction timings, counts and before/after RSS/heap/external memory. `audit-p7-block-lifecycle.py` parses the final packet into `block-lifecycle.json`:30 completed scopes across cold/warm, maximum active blocks1, no incomplete scope. Cold performs10 local preparations across mesh/domain/boundary checks; all RHS within each preparation reuse its native factors. `trainingJobs.factorCopies=0`.

Across fine preparations, magnetic systems include44,622/44,653 unknowns; the selected domain2pair has44,180/44,306 unknowns; largest recorded panel is17,933,682F64 values (143,469,456B values alone). Factor numeric times are about0.4–0.45s, whereas basis training/reduction dominate many block walls. For the final fine pair, current/potential coordinates are693/68; preparation stage times are retained per block in response evidence. Do not sum distinct electric/magnetic unknown counts as one matrix.

Full factors have Rust preparation-scope ownership and do not cross the reduced DTO/cache boundary. DTOs can retain field/material/training data. Some post-block RSS snapshots fall far below the factor-stage process high-water mark, but allocator retention and paging prevent treating RSS as exact live-factor bytes. This instruments existing release behavior; it does not claim a new memory saving from redundant clears or a complete allocator audit.

## Rejected runs and targeted fixes

- Initial BSP produced eight all-local STM32 cells with up to48 differential contacts; diagnostic `route-plan.json/.png` remains. That automatic implementation was removed. One prematurely started segmented numerical test was stopped within about20s; no result from it is accepted.
- Rotated45 first DSL packet `1793386990645550300` failed exposed-area matching (~1.05e-8 relative). The native owner reproduced the exact request, refined the final dyadic arrangement grid while preserving original source points and the1e-8 guard, passed all four domain/mesh controls, and rebuilt normally.
- Changed rotated45 retry `1793387750698295400` cleared that defect and ran field models, then failed unchanged spatial convergence: delay2.174198%>2%, wave0.000997594%, loss0.0000133464%;87.148s,528,289,792B RSS /457,732,096B private. No tolerance was relaxed and no identical retry is justified. Rotated full-channel numerical admission remains open.
- Bend first packet `1793387106730929500` failed P6 gauge assembly (18.778s). A TS classification defect allowed a rectangular remainder to use another transverse corridor's proof; fixed by exact admitted rectangles and joint event extraction. Changed retry `1793387859548420500` still failed P6 (33.807s), and all four isolated block models nevertheless compiled/solved.
- The exact cached bend request reproduced that second failure in50ms without FEM. Native P6 assembly had derived nullspace intersection from a squared Gram spectrum. A localized direct thin-SVD intersection with original-constraint residual certification now passes the exact request and35 related tests; compiler/nullity/conservation guards remain unchanged. The normal release build and rebuilt-addon replay passed; details/fixture are recorded in the native report. This is a targeted interface-composition fix, not a P6 rewrite.

## Remaining acceptance

The frozen acceptance ledger below records completed small cases, numerical rejections and unsupported real extraction separately. No remaining run is planned for this candidate. Even successful small controls cannot supply a real-topology reference or distributed-plane model. Rotated numerical convergence, complete via/return/crosstalk and total physical error remain separate obligations where no successful completed case/reference is recorded. PI plane/source physics, RF loss/continuous-band causality, thermal/DC scope and EMI/antenna field requirements are not replaced by this partition. P7/P8/P11 and all-DSL readiness remain open.


## Final packet continuation

- Fresh-process straight restore `1793389562301636800`:1.713600s internal /3.017307s supervised,303,742,976B RSS /315,645,952B private. Matrices bit-identical to cold; assembly/training zero. It passes the10s restart goal for this coupon only.
- Actual unchanged STM32 full DSL was attempted under the approved diagnostic8GiB/600s and Windows1GiB commit reserve: `1793389572129921300`, failed `CHANNEL_CORRIDOR` before field extraction,2.195997s,207,290,368B RSS /136,486,912B private. Initial commit headroom17,480,908,800B, minimum17,417,035,776B. No resource supervisor refusal occurred. Captured-fill warnings/assumptions remain in its DSL report. No model was produced, so real warm/restart runs would not test cache reuse and were not repeated.
- Final corrected bend full DSL `1793388988025203100` completes with2sections/2joint locals,536.994099s internal /538.489025s supervised,791,420,928B RSS /723,169,280B private. Mesh1.846527%, domain0.622457%, boundary1.126669% pass; P6 residual1.225e-9, KCL5.244e-16A, power error3.219e-10. Twenty block scopes are sequential with no incomplete scope. Cold exceeds120s but stays below600s.
- Separate whole-bend references: `1793389600946011300` (electric edge0.15mm /current triangle area0.1mm²) completes88.107989s,1,268,989,952B RSS /1,280,458,752B private; `1793389727639853500` (0.075mm /0.05mm²) completes142.817145s,2,174,148,608B RSS /2,290,524,160B private. Both use4GiB native /5GiB tree /1GiB reserve /600s limits. The latter changes whole-response delay by0.423093%, copper loss by1.547e-7%, wave by0.000211841%; `reference-convergence.json` records the original drive/floor conventions. This is reference uncertainty evidence, not a mathematical absolute bound.
- Composed bend versus the finer whole reference: complex S0.769961%, phase delay0.514169%, copper loss3.995737%. These checked differences and the measured reference sensitivity are below the8% target without adding separate8% allowances. Full matrices, both comparison packets and inspected numerical plot are retained in the bend output directory. No result is transferred to STM32 or arbitrary bandwidth.
- The first finite-via full DSL control `1793389956529902300` fails before factors on source copper set2 area preservation (relative1.674e-8, topology/holes unchanged),1.441634s,213,229,568B RSS. Native owner reproduced all four requests and corrected polygon-domain barrel construction to use the existing common arrangement grid, keeping original points, integer bound and guards. Its changed retries are recorded below.

Geometry-only `corridor-controls.json/.png` contains straight0/45/90, bend, branch, finite via, coupled neighbour and meander source/corridor/cut plans. Its banner explicitly refuses a numerical certificate. Red/blue are actual TOP/BOTTOM source polygons; green/purple are section/local domains; dashed lines are physical cuts. All polygons/finite returns remain in the saved plans. Source-subdivision pipeline regression preserves one run across split source identity, and the long meander legs retain one joint multiconductor transverse extraction.


The via area-fix retry `1793390622446044700` completes its local field preparation/reduction but fails P6 constraint certification:98.933324s,560,164,864B RSS /494,190,592B private. All three isolated models pass, while its exact saved assembled request rejects in47ms. Direct decomposition leaked a coefficient about4.7e-18 into an independent constraint support, causing original-row residual/bound1.2155e-18 and a correct strict refusal. The targeted fix partitions **exact nonzero constraint support**, decomposes each component and embeds exact zeros outside that support, with nonsingular row equilibration. All original rows, their1e-12 certification, System/nullity/conservation checks and every R/L/C coefficient remain. Native saved bend/via replays and36 related tests pass before the normal build; no physical coupling was zeroed to obtain acceptance.

A separate bounded STM32 joint reference script, `verify-p7-real-reference.ts` (`--scenario real-reference` in the supervisor), captures the original DSL inputs and all board copper/bores into one enclosing polygonal domain, disables insulated copper cuts and retains original endpoints/materials/branches. It is independent of the rejected corridor decomposition. It is a reference attempt only, not a replacement all-local automatic planner or new solver. Its8GiB/600s supervised result is recorded in the frozen ledger below; inability to complete it is a real-reference acceptance gap, not proof of accuracy.


### Frozen acceptance ledger

Latest addon straight cold/warm packet `1793393382999381200` uses the existing four-RHS scheduling path (immutable factors shared, zero factor copies) and explicit source fragments in correct area units. Cold142.079743s and warm1.494908s are **measured**, not extrapolated from the one-job run. Fresh-process disk restore `1793393582119553800` completes1.599423s internally /2.765189s supervised,276,602,880B RSS /210,874,368B private. Cold/warm/restart matrices are bit-identical; restored local assembly/training are zero. It passes the warm/restart goals for this fixture, but cold remains above120s. Latest reference differences remain exactly4.994104% delay /0.000574507% complex S /0.000711407% loss. Latest `cache-comparison.json`, `block-lifecycle.json` and `corridor-response.png` retain actual data. Thirty scopes are sequential, max active1.

The via final changed-candidate packet `1793392116439826500` clears all repaired area/annulus/P6 defects and reaches the full coarse/fine response comparison. It fails unchanged spatial delay2.763860%>2% (wave0.001846141%, loss1.482e-7%),363.705700s,1,639,378,944B RSS /1,586,343,936B private, four shared-factor RHS jobs. This is not an admitted via channel. No identical full FEM retry follows. Rotated45 likewise remains unadmitted at2.174198% delay sensitivity. Native DC/area/continuity controls alone do not replace these failures.

Whole-source STM32 reference packet `1793392606244645600` fails **geometry admission**, not memory:7.488762s,272,592,896B RSS /274,313,216B private, initial commit headroom17,828,335,616B,8GiB tree/native cap,2GiB reserve,600s. Copper set2 area changes3.974439737e-8, above1e-8, with unchanged component/hole counts. Native exact-request reproduction tried **one** maximal-safe dyadic grid; set2 cleared but set15 still failed1.418278274e-8. The failed candidate was restored, production Rust/addon remain unchanged. `native-overlay-precision-evidence.json` records all trials, including a spoiled debug deadline, separately from release timing. Native refusal regression `stm32_joint_precision.rs` passes.

The full60.1×40.1mm domain needs2^24 grid units/mm at the finest permitted existing global grid. The next dyadic level2^25 maps the30.05mm half-span to1,008,310,681.6, exceeding the current absolute integer bound536,870,912. Meeting the unchanged1e-8 guard for all annuli requires a different geometry dynamic-range/conformity path; no broader kernel rewrite, changed source points, filtering, canonical-area substitution or tolerance relaxation was introduced. Thus a legitimate complete real reference is **not obtained**. Increasing RAM or calling the rejection a fast solve would not fix it.

| Obligation | Result |
| --- | --- |
| Local exact multi-source/oriented contacts and DC conservation |PASS native focused controls; physical source/layer/span preserved |
| Complete material/outline events, source subdivision, ownership and barrel envelope |PASS geometry controls; no area-as-length ledger |
| Uniform/star/coupled four-port P6 numerical controls |PASS; max absolute complex-S errors1.404441e-10 /8.357664e-7 for uniform/coupled comparisons |
| Straight full DSL + separate whole-coupon reference + warm/restart |COMPLETE low-band conditional fixture; cold time goal FAIL |
| Bend full DSL + two independent whole-bend mesh references |COMPLETE low-band conditional fixture; checked differences<8%; cold time goal FAIL |
| Rotated45 / via complete automatic physical admission |FAIL unchanged2% spatial-delay guards; no extrapolated success |
| Actual STM32 full DSL |REFUSED distributed lateral finite-plane interface; no output/cache model |
| Matching real whole-source independent reference |REFUSED strict source-area precision at current global integer range |
| STM324GiB/120s,5s warm,10s restart and<=8% total physical error |NOT ESTABLISHED; no successful model to time or compare |
| Full aggressor/victim board DSL, arbitrary shared planes, HF/all-DSL/P8/P11 readiness |OPEN; coupled primitive matrices are not a universal board certificate |

Final scope is working conditional run/interface code, concrete small physical results, source-preserving refusals and reproducible remaining obstructions. The requested complete STM32 outcome is **not achieved**. Its next work needs supported distributed finite-plane interfaces, a legitimate real reference with adequate geometry precision, and the existing numerical/band/resource guards retained. No commits or pushes were made; native/user debugging and original board/DSL fixtures are preserved.


Final validation: `npm run typecheck` and `npm run build` PASS;29 focused geometry/material/fragment/cache tests PASS;4 numerical uniform/star/coupled/ownership controls PASS;7 supervisor-policy tests PASS. Native normal builds passed for each production source correction, broad coupled/physical/frequency regressions passed, exact saved bend/via network regressions passed, and the final whole-board precision-refusal regression passed after restoring the failed candidate. The latest source-fragment datatype cleanup was verified to leave all physical block DTO inputs byte-identical to the saved via request; actual solver inputs were not repaired or rescaled. Generated geometry and numerical figures were opened for visual inspection. All implementation files are frozen for parent review.
