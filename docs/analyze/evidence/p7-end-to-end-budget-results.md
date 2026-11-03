# P7 end-to-end budget: bounded ordering experiment (2026-10-30)

Status: the simple BFS-layer ND candidate is rejected; whole P7 remains open. CPU/F64 direct sparse FEM; no matrix coefficient, copper/contact geometry, physical guard or production default is changed by the ordering experiment.

## Experiment contract

The saved remote request is `debugging/p7-memory-ordering/request.json`, copied byte-for-byte into `debugging/p7-end-to-end-nd/request.json`. Use the current compact-v1 airP2 / retained copper trace4 matrix. Historical private-first AMD evidence is retained for context, but its matrix dimensions differ (see below); it is not presented as a same-matrix ratio. No fullAMD or profile search was repeated. The diagnostic example temporarily uses air degree2, config `entity:4:4:16:1`.

The candidate is deterministic graph nested dissection: construct symmetric adjacency from actual CSC edges; three BFS sweeps choose a peripheral root; a median BFS layer is the separator, ordered last. Every actual edge is checked for an illegal left/right crossing. Disconnected components are discovered iteratively; the implementation releases parent BFS arrays before balanced recursion. Small leaves and separator vertices use deterministic degree/index ordering. The output is checked to be a complete bijection. Cell-private elimination remains unchanged. This is a graph separator experiment, not an approximation to the magnetic matrix or a new linear solver.

The temporary ordering admission conservatively accounts an additional40 bytes per input nonzero and160 bytes per unknown, alongside the original AMD scratch bound. This covers trace CSC, symmetric CSR, permutation and recursive balanced vertex lists. Numeric-stage admission and all solver residual/KCL/KVL/energy guards remain unchanged. A symbolic-only stop prevents magnetic numeric allocation regardless of the predicted fit. Supervision is finite600s,4608MiB process-tree private commit plus512MiB system commit reserve; Windows paging is diagnostic only, not proof of a normal resident working set.

## Independent numerical controls

Release focused tests PASS: connected3D grid SPD (64 unknowns), disconnected3D grid SPD (343 unknowns), isolated1024 vertices and1024-vertex chain. ND is deterministic and bijective and checks separators from every actual edge. The two SPD solves use exactly the same coefficients and RHS with AMD and ND, and agree within1e-12 absolute at every unknown. No residual tolerance was relaxed. Final focused build reported2 tests PASS,40 filtered out,0.21s test execution; build wall4m03s. Existing unused-import/dead-code warnings remain.

## Baseline, not rerun

Historical intermediate airP3/bubble4 private-first AMD (not final compact-v1):1113875 unknowns,38952305 input nonzeros,108194 private coordinates,680816983 exact simplicial fill. Actual unrelaxed supernodal factor6169957272B; symbolic bound221742160B; workspace641129983B; numeric stage8136190172B including retained base194971877B, CSC623236880B and node reserve285152000B. These numbers are from `p7-memory-results.md` and the saved log, not a new measurement. Historical dual-order symbolic diagnostic116.811419s, peak RSS2192052224B/private2484711424B is not comparable timing for a single current ordering. The host is busy; even read-only shell invocations sometimes yield after10s, and build wall is unstable.

## Acceptance ledger

| Requirement | Status |
| --- | --- |
| Remote positive complete STM32 | not accepted; tested ND rejected and production numeric memory refusal remains |
| Cold120s / peak4GiB (goal2GiB) | unproven for remote |
| Warm5s / restart10s | unproven for remote |
| Entire assembled physical response within8% | unproven; old7.7902% composition and5.63% profile figures do not establish combined accuracy |
| RF loss/coupling/common-band transition | open |
| DC/PDN and causal temporal/TPS finite-front dependencies | open |
| Geometry classes / independent whole-channel validation | open; retain original ledger |

The parent change reuses immutable native geometry within one `solveBoardChannel` call across domain/level plans. It adds no global cache and changes no physics. Five focused planning/convergence tests passed; no speedup is inferred without a measurement.

## Remote result and selected path

The ND run exited with the original memory refusal before symbolic-pattern allocation, before numeric factor allocation. Current magnetic matrix:1097762 unknowns,38421979 upper CSC entries,108194 private coordinates. Exact simplicial fill **20459023747**; therefore factor values alone require at least **163672189976 bytes** (152.43GiB), already far outside the4GiB target. The symbolic-pattern construction bound was8524376556B versus7796674500B remaining. No factor value/scratch allocation or response solve was attempted, so no actual supernodal values/scratch estimate or physical response is claimed. This absolute failure rejects this simple deterministic BFS-layer ordering, not all possible ND implementations.

Supervised remote wall192.3584684s, tree peak RSS2102964224B, peak private commit3049496576B, initial Windows commit headroom21025939456B. No supervisor refusal, timeout or OS kill. Geometry63.309317s, current assembly1.754069s, harmonic basis1.159681s, electric assembly/factor10.596745s. Historical geometry23.590356s confirms that elapsed performance is not comparable as a speedup. The finite experiment includes geometry/current/electric preparation and magnetic ordering/prefill; it is not a complete DSL result.

A comparison trap was found: saved historical ordering runner does not set `P7_FEM_AIR_DEGREE`; its archived patch defaults to3. Its1113875-unknown matrix therefore cannot be silently equated to the current explicitly air2 matrix1097762. Both use the identical request. A current private-first AMD diagnostic was briefly started to repair this comparison, then cancelled on parent instruction before any symbolic result, because the absolute163.7GB lower bound already rejects ND. No percentage fill/memory speedup is claimed.

Selected runtime remains private-first AMD. All temporary ND source/hooks and example air2 modification were removed from production. The ignored `debugging/p7-end-to-end-nd` contains the exact source, patch, request, logs, finite supervisor and evidence; `debugging/p7-end-to-end-amd/cancelled.json` records the cancelled comparison. No external dependency was added. The final addon remains unchanged; consequently no addon rebuild was required for a rejected diagnostic-only candidate. The diagnostic example build passed4m45s; focused controls passed as recorded above.

Request SHA256: `0b0d3f64e0c85970625c8b9ef89a94242af8be977eb990fd172d844b53bbc359` (identical to saved request).

## Positive ordinary DSL and cache evidence after TS geometry reuse

A real small physical mixed section/local coupon completed through geometry, direct FEM field/training/reduction and P6 assembly using the unchanged final baseline addon and the parent TS snapshot reuse. It is not a remote STM32 success and does not use the rejected ND candidate. Physical-RAM supervision:2048MiB native/tree limit,256MiB host reserve,600s/call. First2560MiB tree+512MiB preflight was refused (2556813312B available); it performed no calculation. The smaller finite cap then admitted3413520384B available.

Evidence folders under `debugging/p7-channel-plan/`: cold+warm `automatic-supervised-1793372258834536100`; fresh-process restart `automatic-supervised-1793372400037005200`. Empty-cache cold **125.1850832s**, warm **1.1300416s**; supervised cold+warm wall **129.3751489s**, peak process-tree RSS **543465472B**, largest child471445504B. Restart call **1.2367391s**, complete supervised process wall **2.3978480s**, peak tree **261464064B**, largest child179150848B. No paging mode, timeout or memory refusal during either admitted computation. Cold exceeds120s; warm and restart meet5/10s. Old small-coupon cold116.8539482s/tree530907136B is historical context only: this busy-host observation does not establish a speedup from TS reuse.

Matrices and assembled models are bit-identical cold/warm/restart. Every final block cache reports assembly0 and training0 on warm/restart; timing metadata naturally differs. Exact comparisons and cache states are in `cache-comparison.json`. The7 focused cache tests PASS, including board/contact/material/band/core/schema invalidation, corruption handling, binary/profile separation and same-executor profile-change refusal. Parent reports TS typecheck/build and5 focused planning/convergence tests PASS. `git diff --check` is required at handoff; production native sources have no diff and original CRLF bytes were restored.

Independent already-saved field-refined whole geometry `automatic-supervised-1793363430965924500` is reused, not recalculated. Actual1�10kHz sampled comparison: maximum complex-S relative error0.0006325085%, Cu-loss relative error0.0030117283%, phase-delay relative error **7.79017605%**. This baseline coupon sampled comparison is below the current8% target; it is the same historical composition error, not evidence for combined compact-profile accuracy, missing coupling classes or a complete continuous-band certificate. The existing audit script intentionally retains its historical5% failure status; raw errors are reported against the new8% target without weakening numerical guards. A coarse historical reference was first audited and retained separately (16.5894% delay); final comparison uses the documented refined reference above.

Runnable measurement: `python scripts/verify-p7-automatic-supervised.py --scenario segmented-coupon --warm --native-memory-mib 2048 --tree-memory-mib 2048 --reserve-mib 256 --seconds 600`. Restart adds `--cache-dir <cold-folder>/cache` and omits `--warm`. Existing DSL source is `tests/helpers/pcb-analysis-segmented-coupon.ts`. The rendered actual numerical plot is `<cold-folder>/channel-evidence.png`; its local generated renderer labels the8% sampled target and preserves the conditional scope.

No overall P7 closure: remote complete electrical response remains blocked; cold120s is missed even on the small coupon in this observation; RF/DC/PDN/time/TPS and geometry-class obligations remain open. No further ordering/profile search or heavy retry was performed. No commit was created.

Cold native stage totals (all local refinement/domain blocks; seconds): `{'geometry': 0.18464099999999997, 'currentAssembly': 0.00476, 'harmonicBasis': 0.004326, 'electricAssemblyAndFactor': 0.10464000000000001, 'magneticAssemblyAndFactor': 36.414845, 'initialMagneticBasis': 6.597613000000001, 'fullSolvePreparation': 0.134929, 'basisTraining': 43.961609, 'reduction': 34.78934699999999, 'developerVerification': 6.9e-05}`. Basis training and reduction dominate the remaining computation; these totals exclude TS/transport overhead.
