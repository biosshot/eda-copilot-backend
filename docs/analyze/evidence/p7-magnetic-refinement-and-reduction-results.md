# P7 corrected mixed precision and conforming far-air reduction

**Result: both implementations were exercised and rejected for final P7 use.** Corrected F32 passes the complete small channel, but the single real STM32 attempt fails numeric LLT. The conforming reduction passes independent energy controls, but fails the whole-reference delay allowance and removes no free unknowns on STM32. Both failed runtime branches were removed. Production remains F64; P7 is open.

## Parent review

Reviewed retained source diff, numerical logs and supervised result manifests. Independently reran all eight cache tests and seven supervisor tests successfully; inspected the final native build and frozen F64 control evidence. Retained changes are diagnostics, explicit unsupported-precision refusal and developer supervision, not an accepted memory optimization.

Two qualifications matter: the compact unreduced full-channel control already exceeds 8% (8.1078%), so reduction is not the sole cause of missing that threshold; it adds about 0.0437 percentage points to that comparison. The real F32 LLT failure occurs before correction and is not proof that the underlying physical F64 system is singular. No full successful real F64/F32 comparison exists. Timing observations on the small channel were obtained under varying host load and do not establish a general 1.73x slowdown. These limits do not change rejection of the tested candidates for production.

## Implementation and numerical contract

Stage A used F32 magnetic Cholesky values and triangular solves only. Original assembly/CSC, compensated residuals, reductions, exterior Schur/model matrices and transport stayed F64; capacitance stayed F64. Each preserved RHS column was reversibly normalized before F32 conversion. Up to eight corrections reused the same factor, with finite/stagnation/exhaustion/shared-deadline checks. Each RHS required componentwise backward error <=1e-8 and scaled backward norm <=1e-14, using `norm(residual)/norm(abs(A)*abs(x)+abs(b))`. The **unchanged** compensated full magnetic action retained its original row1e-8 and RHS-relative1e-14 guards. The nine exterior RHS were corrected before Schur construction; its1e-9 directed reciprocity guard was not symmetrized or relaxed. No second full F64 factor, regularization, Krylov solver or silent fallback was added.

Two intermediate small runs failed new checks at the wrong level: an extra per-row1e-14 threshold, then an RHS-relative1e-14 check on the rounded direct solution before compensated full-action correction. The latter stalled with backward error <8.6e-17 but RHS-relative residual2.6e-14. Reversible RHS scaling and proper per-RHS backward checks corrected these implementation defects. The failures are preserved in `dsl-A-supervisor.log` and `dsl-A-scaled-supervisor.log`; they are not admitted results. Unit tests cover real F32 factors, multiple RHS, triangular/full storage,1e-300/1e200 magnitudes, nonfinite values, deadline, stagnation and iteration exhaustion.

Stage B was documented before coding in `reduction-design.txt`. It removed a quadratic edge bubble from its entire incident-cell star only when no incident cell was protected. Protection included every original copper-vertex-star cell plus an inner shell of half the smallest clearance between copper and unchanged outer bounding boxes. Original vertex functions, near-cell traces, cell-private basis, source/ports, copper/bores, mesh and exterior coupling remained. Retained edge detail on both sides of the patch boundary made the subspace conforming. A regression checks shared-edge conformity and prevents minimum-degree propagation from lowering protected traces. This was one fixed approximation, without a parameter/method sweep.

## Independent frozen energy controls

The saved `tetra-space-reference.json` was reused without modification or regeneration. Its0.6780636440% empirical uncertainty is included below. All rows use the same physical three-cell source and air2/trace4/body4 configuration except for the indicated restriction/precision.

| Candidate | Unknowns | Factor bytes | Numeric-stage bound B | Energy error + reference uncertainty |
| --- | ---: | ---: | ---: | ---: |
| Original compact F64 | 3644 | 3656560 | 9592000 | 5.1667451161% |
| Reduced F64 | 3224 | 2986336 | 8539488 | 5.1670207656% |
| Reduced corrected F32 | 3224 | 1493168 | 7515080 | 5.1670207656% |

The restriction changes upper nnz60303→55372 and exact fill387209→324259; protected near-trace546/private61 coordinates remain, air3037→2617. Factor bytes fall18.33%, numeric bound10.97%; the full conservative operator plan changes only28514328→28387320B. Combined corrected F32 and reduced F64 coefficients differ at about1e-15 relative. The combined error is evaluated directly against the original reference, not obtained by adding separate allowances. Coarse and fine4/5-body reduction controls both pass. Original compact F32 exterior residuals needed two corrections, normalized Schur antisymmetry~2.4e-16. These controls alone do not admit a full channel.

## Completed ordinary small DSL

All cases below use measured addon SHA256 `5866F92EEEEAF3F583BDD1D1C99E27ABFB303B9093004260ADF03133C6CBC18F`, four existing outer RHS jobs,2048MiB native/tree bounds and600s/call. Windows commit-aware supervision was opt-in. RSS/private commit are50ms process-tree samples, separately from conservative native allocation bounds. Host load varied; these are observations, not a controlled speed benchmark.

| Case | Cold call s | Warm call s | Tree peak RSS B | Tree peak private commit B | Max sampled delay error |
| --- | ---: | ---: | ---: | ---: | ---: |
| Original baseline F64 | 180.9069903 | 1.0595943 | 529321984 | 469336064 | 7.79017605% |
| Corrected F32, same baseline | 312.6263797 | 1.1355974 | 487215104 | 432734208 | 7.73829011% |
| Original compact F64 control | 31.2368435 | — | 316911616 | 259268608 | **8.10782443% fail** |
| Reduced compact F64 | 31.1621556 | — | 308002816 | 253816832 | **8.15148490% fail** |

Corrected F32 saves36601856B/7.79865% peak private commit on this completed comparison, with a1.73x observed cold-time cost. Magnetic assembly/factor stage53.798349→35.547836s; training58.073391→137.128373s; reduction55.168467→115.543178s. The120s cold goal fails for both original baseline cases. F32's fresh-process restart call is1.3656198s (whole supervised process2.5200086s). Cold/warm/restart response arrays are bit-identical; every final cached block reports assembly0/training0 on warm and restart. Warm5s/restart10s goals pass **only on this small coupon**.

The reference is the already-saved independently refined whole geometry `automatic-supervised-1793363430965924500`. At1/3.162/10kHz, corrected F32 maximum errors are complex S0.0006325087%, Cu loss0.0030117294%, delay7.73829011%. Floors are S0.01/delay1ps/loss1e-18W. The same-coefficient F32-versus-F64 response difference is0.06530556% delay,0.00000262353% complex S and1.07e-9% loss. Thus geometry/composition error and precision propagation are reported separately; no continuous-band or independent uncertainty certificate was regenerated. The original F64 comparison reproduces the earlier~7.7902% composition error.

The reduction gives only2.10% extra whole-process private-commit savings versus its compact control. It is rejected because its actual whole-reference delay exceeds8%, even before adding any further reference uncertainty. The existing compact profile's prior independent energy/whole-example evidence must not be mistaken for admission of this ordinary mixed-channel composition. No combined full DSL or real reduced run was started after this failure.

Successful calculation packets/measurements are in `debugging/p7-channel-plan/automatic-supervised-` directories: F32 `1793378482649340600`, matching F64 `1793378838211254000`, F32 restart `1793378873749215700`, compact control `1793379042181359700`, reduced F64 `1793379245528755700`. Raw arrays, stage totals and bit/cache assertions are in `debugging/p7-magnetic-refinement/small-dsl-summary.json`. The viewed result plot is [completed-small-dsl.png](../../../debugging/p7-magnetic-refinement/completed-small-dsl.png).

## Actual STM32 coordinates and bounded attempt

The read-only audit reused one physical mesh and stopped before sparse graph/ordering/factorization. Compact original/reduced coordinates are both1097762 =2336 remaining air trace +987232 near-Cu trace +108194 cell-private, on22614 nodes/112980 tetrahedra.112931 cells are protected by the fixed1mm shell; no removable free outer edge-star remains. This construction therefore gives **zero real-system reduction**, despite11.53% unknown reduction on the tiny fixture. Audit wall113.5478403s, RSS374550528B/private commit400822272B, initial available commit11826257920B, no refusal. Saved local request SHA256 `0b0d3f64e0c85970625c8b9ef89a94242af8be977eb990fd172d844b53bbc359`.

After small F32 passed, exactly one real ordinary U5.7→J5.2 attempt used the original baseline profile, F32 and **one** existing outer RHS job to minimize simultaneous scratch. This is scheduling, not changed physics or a job-count sweep. The supervisor used8GiB tree private commit,512MiB reserve and600s/cold call; OS pagefile settings were unchanged. Cold LLT failed with `LOCAL_MAGNETIC_SINGULAR: potential Laplacian unresolved; no regularization`. No model, ports response, L, delay, warm result or restart was admitted. No heavy retry was made.

| Real baseline F32 quantity | Result |
| --- | ---: |
| Unknowns | 1411442 |
| Air / near-Cu trace / cell-private | 11633 / 994048 / 405761 |
| Original upper CSC nnz | 50854985 |
| Exact fill / stored panel values | 692719663 / 784635174 |
| Original F64 CSC / converted CSC B | 824971304 / 621551364 |
| F32 factor / queried workspace B | 3138540696 / 632913983 |
| Numeric-stage bound / admitted budget B | 5994251776 / 7796674500 |
| Measured peak tree RSS / private commit B | 5992804352 / 6433353728 |
| Supervised wall s | 33.7068556 |
| Initial / minimum available commit B | 19643121664 / 13294997504 |
| Supervisor refusal | none |

The failed process reached5.99GiB private commit, already exceeding the4GiB goal.33.7s is an early-failure time, not a successful cold latency. Factor storage would be6277081392B at eight-byte width, but **no same-matrix real F64 numeric completion was measured**, and no full-memory speedup is inferred. The F32 direct factor failed before residual correction could be applied. This rejects the tested real F32 candidate; it does not prove that every alternative scaling/representation would fail, and none was tried.

Column-owned original upper nnz by category are191215/35539879/15123891, giving F64 index/value payload3059440/568638064/241982256B, plus shared11291544B pointers. Factor fill cannot be additively attributed to physical categories after coupled elimination. “Near-Cu trace” means a non-private coordinate belonging to a cell touching an original copper vertex; “air” is the remaining trace ownership category, not a claim of an independent uncoupled material block. Private coordinates vanish on every face.

Real packet `automatic-supervised-1793379352717185500`; board SHA256 `0159c43d43b59df4f59fb943d59f0d841d17ca38d6d5a7d5576920a4aee3cce3`; exact bounded DSL code SHA256 `d8a14170a9402caccbc636d829905785001b133b5ccc562e383c66c88b10fd4b`; original F64 matrix FNV1a64 audit `895cf1763bd7346b` (non-cryptographic). Full identity and script are saved in `real-identity.json`. These baseline coordinates differ from the compact read-only audit; they are not silently equated.

## Cleanup, retained work and verification

Both numeric runtime branches and the far-profile selector were removed. Exact candidate sources are archived under `debugging/p7-magnetic-refinement/rejected-far-source/` and `rejected-corrected-f32-source/`; failed/earlier runs remain labeled. F64 sparse factorization/solves and original numerical guards are restored. Final F64 energy control reproduces the original matrix **bit-for-bit**, error0.04488681472101003. `P7_MAGNETIC_PRECISION=f32` is explicitly refused in TS/native entry points, including before a cache hit; F64 provenance/cache identity remains explicit. Existing compact-v1 defaults were not rewritten because this experiment found a mixed-channel limit.

Retained code supplies `Operator::audit_patch_coordinates` and the `magnetic_coordinates` developer example, sharing exactly the production coordinate map without a sparse factor. The supervisor adds optional remote cold/warm measurement and bounded1..4 RHS scheduling; its default remains four. User `native/analyze/debugging/`, physical fixtures and references were preserved.

Final addon SHA256 `D5A2541E3A038FF1477F7885131C96DCF77E2255C90F11FE493212CDDA9C8F8C`; normal `npm run native:build` passes6m27s. This is the cleaned F64-only binary, not the rejected measurement binary above. Final checks: coordinate audit1, sparse layout1, board response7, coupled projection1 and explicit frozen energy1 pass;8 TS cache/retired-selector tests and7 supervisor tests pass; TS typecheck/build and final native default/F64/rejected-F32 selector smoke pass. `git diff --check` passes. No unchanged heavy rerun was performed after cleanup of rejected branches.

Reproduction of the measured experiments requires the archived source, not just setting a retired environment flag. Source baseline commit `fc5edf6bc41055d7c4cd49fa0b2739f62a8571d5`. Commands, runner scripts and raw logs are preserved in the evidence folder. Final production memory is **not claimed to be reduced** by rejected experiments.

## Acceptance ledger

- Corrected small F32 numerical/independent-energy and sampled full DSL: pass; full real F32 factor: **fail/rejected**.
- Conforming restriction/protected traces/independent energy: pass; real unknown benefit: **zero**; complete small delay: **fail/rejected**.
- Real complete channel, <=4GiB / <=120s: **not achieved**. Real warm/restart5/10s: unproven because cold failed.
- Original whole-channel geometry classes, bandwidth/common-band, RF losses/coupling, DC/PDN and causal temporal/TPS obligations remain open. Small sampled comparisons and cached responses do not close P7.

No commit or push was created.
