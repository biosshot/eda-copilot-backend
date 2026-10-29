# P7 direct-FEM checkpoint — 2026-10-28

Active scope: [agreed route](p7-to-p8-p11-plan.md), one existing direct sparse FEM,
CPU/F64, shared300s cold user/2GiB, physical observables<=5%.
**P7 remains open; this delivery does not complete the agreed route.**

## Current result after phase separation and numerical correction

The actual STM32 opposite-drive1kHz regression now passes without weakening the
original1e-8 KCL/KVL guards. Previously global defect correction stopped at
6.31e-16 while the weak constitutive row still had KVL1.24828e-7. The same solver
now continues until global and local metrics resolve, retaining low current
corrections in compensated residual evaluation. Final KVL2.5472799779320795e-10,
KCL3.952773021587171e-21, complex-power error1.6398113985374103e-15,
Joule error1.6462750610268465e-15,5 iterations. No modes or drives were removed.
Corrected replay37.666s; final feature replay112.429s under different host load;
these are correctness checks, not a speedup claim.

`board_volume prepare` separates geometry/operators, four necessary training
solves and reduction from user P6 queries/full verification. It skips those last
two phases. `verify` preserves the original17-frequency response and51 full
holdout cases, both ports/opposite drive, reusing all four training solutions.
Cold preparation remains capped300s; the subsequent developer phase has its own
1800s clock within a2100s family supervisor. The full developer sweep was **not
completed** after the physical refusal below. This is still an internal example,
not the production worker factory. Stages include geometry, current assembly,
harmonic basis, electric/magnetic assembly and factor, initial basis, full-solve
preparation, training, reduction, user P6 and developer verification.

| Same binary/request, cold prepare only | Wall seconds | Peak family RSS bytes | Magnetic numeric factor seconds |
| --- | ---: | ---: | ---: |
| 1 thread |189.0486232|1026469888|6.058012|
| 4 threads |195.1336933|1030148096|2.144253|

Binary SHA256 `24094af9b68589a38d0edb537b3a02c083a781354b95e29867ef6f53a975e9a8`.
Serial stages: geometry1.3524s, magnetic assembly/factor20.5885s, initial basis
6.7205s, training125.9259s, reduction34.0136s. Four threads improve factorization
but not total preparation; default remains1. One pool is bounded by available
CPUs and4, with2MiB stacks and a conservative8MiB reservation even while idle.
Sparse workspaces follow the selected policy; implicit dense defaults stay
serial. Operators retain that policy: future cached parallel use must re-enter
the bounded execution scope. Existing direct factors/requested RHS are reused;
no new solver or persistent worker factor cache is introduced. Further CPU work
stopped at the physical refusal, not claimed complete.

An earlier pre-Rayon split prepare took159.878782s/1025777664bytes; it is not an
equal-binary comparison. All measurements above include training but **skip user
solve and developer verification**; they are not end-to-end user success. Older
packets contain microsecond bookkeeping durations for skipped phases; the final
source emits null for skipped user P6. Warm/restart times are N/A: no worker/disk
model cache is implemented.

## Physical rejection witness

Before changing the field grid, one refinement was declared: air margin2→8mm,
vertical maximum2→0.5mm,25µm interface step/growth2. All copper/current sources,
finite contacts, materials and the saved28-mode voltage subspace remain fixed;
selected electric RHS are re-evaluated by the existing FEM, without repeating
magnetic training. Refined field4218nodes/20997 dielectric tetrahedra; measured
diagnostic2.6054258s, without a supervised RSS measurement for this small check.

Maximum complex-port Z change across17 original frequencies/all four entries is
**88.816966%** (normalized to refined response with predeclared1µOhm floor),
exceeding5%. At1kHz Z00 changes from about0.000130-j727624210Ohm to
0.0000739-j6506501002Ohm. Component-charge projection reproduces this capacitive
pole sensitivity; it is not a near-zero tetra-current metric. Exterior charge
and the declared isolated finite Cu ROI affect this observable.

This is a **same-method, fixed-subspace h/domain rejection witness**, not an
independent continuum oracle or an88.8% estimate of true physical error.
Huge common-mode Z entries cannot establish opposite-drive accuracy by
subtraction. Only the direct1kHz opposite numerical control above passed.
No independent physical admission, integrated-current/regional-loss error table
or combined<=5% accuracy is claimed. Neither grid is certified. The contract
limits this study to one baseline/one refinement and says to stop on non-admission
with concrete numbers; no new grid/method search or board campaign followed.

Committed small fixtures under `tests/fixtures/analysis-local-extraction/board-integration/`:
`prepared-stage-profile-v1.json`, `opposite-residual-regression-v1.json`,
`field-port-refinement-refusal-v1.json` (predeclared plan,68 Z comparisons and
component diagnostic), `field-refinement-request-v1.json`, and
`saved-real-field-basis-v1.json`. The latter retains the actual unaccepted model,
voltage basis and producer hash for cheap replay, omitting bulky current recovery;
it is not a trusted production cache. Raw packets remain in ignored debugging.

## Current completion/readiness matrix

| Obligation | Actual state |
| --- | --- |
| P7a/P7b sections/RLGC and accepted P6 classes | Preserved; regressions pass |
| One existing local direct sparse FEM | Alternatives retired previously; no new solver |
| Phase timings / prepare vs full developer verification | Internal example implemented; full verification not completed |
| Cold300s/2GiB | Supervised prepare passes; successful worker user flow absent |
| Opposite1kHz KVL | PASS with unchanged guards |
| Physical observables<=5% | NOT ADMITTED:88.8% port h/domain sensitivity |
| Full17freq/both/opposite physical acceptance | Missing; reduced sensitivity sweep and1kHz numerical control only |
| Independent continuum / integrated currents / regional loss errors | Missing |
| Real board→fresh compiled worker→direct DSL | Refuses SECTION_UNSUPPORTED in0.5585104s |
| Worker geometry/operator/factor cache | Not implemented |
| Atomic/checksummed reduced-P6 disk model/recovery/invalidation | Not implemented; no accepted entry |
| End-to-end user cold/warm/restart | No successful cold user result; warm/restart N/A |
| STM32 / TPS63060DSCR / BandAmp accepted extraction DSL | 0/3; no additional board runs after refusal |
| Geometry/ports and frequency diagnostic visuals | Produced/viewed; explicitly unaccepted |
| Accepted current/loss maps | Missing |
| P8 extracted PDN dependency | Not ready: no admitted multiport/range/time model |
| P11 actual coupled-channel dependency | Not ready: no admitted delays/losses/returns/coverage |

Real DSL refusal comes from actual execution, not a substitute circuit. Contact
selectors pass worker tests, but cross-section only supports explicit `a.line`;
the real layered/channel extraction factory remains absent. Existing accepted
line/RLGC/P6 behavior does not establish these extracted-board gates. No cache
behavior or warm/restart benchmark is invented from the refusal. P8/P11 remain
unimplemented. Remaining agreed work is explicitly unfinished.

Checks passed:11 local unit tests,18 current/field integration tests,4 P6 unit
tests,26 P6 integration tests, bounded-pool regression and explicit ignored actual
opposite-drive regression. Typecheck passed. Final ordinary addon build passed
in17m39s after all library/addon edits;31 fresh compiled-worker DSL tests passed.
Later example-only replay/budget edits were checked separately. No full physical
PASS follows from these checks. Python supervisor AST and fixture JSON validation
are part of the evidence checks.

Reproduction (build separately, CARGO_BUILD_JOBS=1):

```powershell
cargo build --release --manifest-path native/analyze/Cargo.toml --example board_volume --example field_observable_check
python tests/fixtures/analysis-local-extraction/board-integration/single-method-run.py unique-packet prepare
cargo test --release --manifest-path native/analyze/Cargo.toml --test real_via_residual -- --ignored --nocapture
```

Cheap `field_observable_check` arguments: STM32 board JSON, committed
`single-method-via-request.json`, `saved-real-field-basis-v1.json`, and
`field-refinement-request-v1.json`. The supervisor never overwrites packets;
prepare caps300s/2GiB, verify2100s/2GiB including the separate developer phase.
Viewed visuals: `debugging/p7-single-method-audit/real-via-geometry-ports.png`
and `field-port-sensitivity.png`. No accepted current/loss maps exist.
User `native/analyze/debugging/` is untouched; no ignore rule masks it.

## Historical checkpoint before the correction above

The following original220s refusal,120s packets, cleanup record and checks are
retained as history. Their claims of an unresolved KVL refusal and deferred
optimization are superseded by the current sections above.

Current contract: [single-method v1](next-task-p7-single-method-v1.md).
One existing direct sparse FEM local candidate, CPU/F64, shared **300 s / 2 GiB**,
physical observable error at most5%. Speed optimization is currently deferred.
P7 is **open**; extracted-board P8/P11 dependencies are **not ready**.

## Actual 300 s run

The unchanged STM32G0_CAN_RS485 via/actual-return ROI used
`entity:6:8:16:3 train`. A single monotonic start feeds remaining time to builders,
full solves and P6; the former per-solve30s clamp was removed. No method, grid,
polynomial configuration or physical guard was changed. A separate supervisor
bounds the complete process family to300s/2GiB, including uninterruptible factor
calls. This is a diagnostic example, not a worker factory.

**Exit1 at220.2373207s; peak family RSS1025572864 bytes (978.06 MiB).**
The supervisor did not terminate it: the original numerical guard rejected the
opposite drive on the first1kHz holdout:
`LOCAL_VOLUME_RESIDUAL: full KCL=1.1087457960519757e-16 KVL=1.2482831421062513e-7`.
KVL limit remains1e-8; it was not replaced by the physical5% tolerance.

All four10k/100k training solves completed (7,7,12,12 iterations). P6 built28
voltage/35 current modes and returned its original17-frequency impedance call;
two individual1kHz full holdouts passed before the opposite drive refused. The
example emits its final report after all holdouts, so no complete model/response
artifact was returned. No successful cold result, warm benchmark or physical
observable error table is claimed. No spatial refinement or configuration search
followed this refusal. Worker integration remains gated on physical acceptance.

Evidence: `debugging/p7-board-integration/single-method-300s-v1/direct-fem/`.
Small exact run metadata and the unchanged request are committed under
`tests/fixtures/analysis-local-extraction/board-integration/single-method-300s-run.json`,
`single-method-300s-plan.json` and `single-method-via-request.json`. The300s plan
records the new policy; observable floors/configuration are inherited unchanged
from the earlier predeclared baseline plan. Executable SHA256:
`f19c61bf1789fe19b13b94a8230d9054975637ae744b78caf52b1140b03287e4`.

## Remaining completion matrix

| Obligation | Actual state |
| --- | --- |
| P7a/P7b sections/RLGC, accepted P6 classes | Preserved |
| Direct local FEM only | Alternative production modules/dispatch removed |
| Shared lifetime budget | Diagnostic300s start/remaining budgets plus family supervisor; worker path absent |
| Real ROI physical observable error <=5% | Not demonstrated; hard KVL refusal precedes admission |
| Both ports/opposite drive, original17 frequencies | P6 sweep computed internally; full opposite-drive check refuses at first frequency |
| Regional/integrated-current/power error tables | Not delivered |
| Geometric/domain independent convergence | Not admitted; same-grid agreement is insufficient |
| Actual board -> worker -> direct DSL | Real compiled-worker scenario refuses SECTION_UNSUPPORTED; extraction not delivered |
| Lines/transitions/PDN/reduction/loss completion matrix | Incomplete |
| STM32G0_CAN_RS485 / TPS63060DSCR / BandAmp accepted extraction DSL | 0/3 |
| Final accepted geometry/current/loss/frequency visualizations | Not delivered; historical diagnostics retained |
| P8/P11 extracted-board readiness | Not ready; no implementation of those stages |

## Retained and retired code

`volume_magnetic` uses direct sparse factorization only. `board_volume` accepts
only direct entity FEM and geometry/field/full/train modes. The existing sparse
`Field`/`ElectricApply` solves requested original-space RHS; full C is no longer a
prerequisite. Complete-C checkpoint loading is a diagnostic option. P6 receives
remaining memory after conservative concurrent reservations, not a fixed512MiB.

Tensor/tensor_potential/sparse_iterative candidates were removed. Independent
integral/surface/cache/matrix references live only in `native/analyze/tests/reference/local`.
Shared geometry/current/field/arithmetic helpers remain. Initial inventory and
pre-cleanup source copies remain in ignored `debugging/p7-single-method-audit/`.

Dependency review found73 unused untracked preparation/reference experiment
sources (including the unused factor-server example); exact bytes/hashes were
archived under `retired-preparation/` before removal from the working tree.
36 generated pyc files were removed after verifying workspace paths. The only
name matches in tracked callers were unrelated same-name current-space and
structural-gauge scripts, not dependencies on the retired directory. Committed
frozen-fixture generators and the actual-board via/return DSL input remain; its resource limits and frequency count follow the current contract. Historical ledger paths are historical;
producer snapshots/results were not rewritten. User `native/analyze/debugging/`
was not changed and no ignore rule was added to hide it.

## Checks and historical measurements

Previous focused validation:56 Rust tests, typecheck, examples/reference cargo
check. Direct frozen source3 energy error0.309465% plus inherited empirical
reference uncertainty0.678064%=0.987529%; this is a primitive check only.
Final ordinary `npm run native:build` passed after all Rust/reference moves in
9m37s. `npm run build` rebuilt the compiled workers and documentation assets;
`npm run typecheck` passed. All31 fresh DSL tests passed, including via contact
creation and missing-via refusal through the actual compiled process worker.

P6 validation passed4 unit tests and26 integration tests. The conditioning test
uses every exact original17 frequency, both ports individually, opposite drive,
and repeated load changes; corrupt cached coordinates refuse against original
physical residuals. Structural gauges and existing energy negatives also pass.
These regressions do not turn the failed real-board calculation into admission.

The preserved real-board `real-via-return-scenario.js` was actually submitted to
the freshly rebuilt worker: it refused in0.2869661s with `SECTION_UNSUPPORTED`
(cross-section supports explicit a.line; layered channel extraction absent).
`single-method-real-worker-refusal.json` binds its board/scenario/worker hashes.
No authored equivalent circuit or dummy extraction output was used.

| Historical cold run (not current deadline) | Seconds | Peak family RSS bytes | Outcome |
| --- | ---: | ---: | --- |
| Direct FEM/full C,120s policy |120.0787639|1025654784|Wall refusal |
| Tensor candidate,120s policy |111.4839997|94924800|Internal deadline; removed candidate |
| Direct requested RHS,120s policy |120.0989665|1025327104|Wall refusal |

These records remain immutable. The viewed historical timing comparison is
`debugging/p7-single-method-audit/cold-run-comparison.png`; it labels both direct
120s timeouts and is not a physical accuracy plot.

Reproduction (compilation separately; run inside one300s/2GiB family supervisor):

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo build --release --manifest-path native/analyze/Cargo.toml --example board_volume
python tests/fixtures/analysis-local-extraction/board-integration/single-method-run.py unique-packet-name
```

The retained supervisor is single-method only and never overwrites an existing
packet. It enforces300s/2GiB for the whole child family. AST validation passed;
the recorded300s run used its same fixed configuration and cap.
The viewed diagnostic `debugging/p7-single-method-audit/direct-300s-refusal.png`
shows actual wall/RSS and the unchanged KVL guard, explicitly without physical
acceptance. Its plot does not substitute for missing current/loss/frequency maps.

## Bounded electric convergence follow-up � 2026-10-28

**The 8 mm candidate is stable under the two declared checks.** The earlier
88.816966% change does not establish that the refined candidate is wrong; the
coarse result is much less stable than the candidate in these tested directions.
This is not a continuum error certificate or whole-P7 admission. No ports,
contacts, copper, ROI, materials, solver family or guards changed. Magnetic
training was reused, never repeated; the saved 28-mode voltage subspace limits
all STM32 Z conclusions. P7 and P8/P11 readiness remain open.

`electric-convergence-plan-v1.json` declares exactly two additional comparisons
before execution: vertical max step 0.5 to 0.25 mm and interface step 25 to
12.5 um at 8 mm; boundary 8 to 16 mm at candidate resolution, growth unchanged.
The coarse and candidate runs below only replay existing results with added
component/geometry diagnostics. All runs used the existing direct sparse FEM,
CPU/F64, serial, supervised process family 300 s / 2 GiB, with builds separate.
All 17 original frequencies and all four complex Z elements are retained in
`electric-convergence-result-v1.json`. Normalization for every new comparison
is |Z-Zcandidate|/max(|Zcandidate|,1e-6 Ohm), including historical coarse replay.

| Case | C diagonal (fF) | Im Z00 at 1 kHz (GOhm) | max Z difference vs candidate | wall s / peak RSS MiB |
| --- | --- | --- | --- | --- |
| coarse | 250.368928, 234.535645, 387.159827 | -0.727624210 | 88.816966141% | 2.387896 / 24.160 |
| candidate | 24.770100, 24.076928, 108.630909 | -6.506501002 | 0.000000000% | 2.488034 / 43.105 |
| grid | 24.381680, 23.818474, 108.169484 | -6.609833073 | 1.588135789% | 2.675259 / 47.566 |
| boundary | 24.762459, 24.069302, 107.096107 | -6.506518216 | 0.000264572% | 2.682167 / 45.750 |

Component-matrix max-entry norm changes are 0.424763% (grid) and 1.412859%
(boundary). Independent unit-conductor excitation energies change at most
1.568102% and 1.412859%. For all conductors at 1 V, energy is respectively
39.845896, 39.828567 and 38.848045 fJ for candidate/grid/boundary; common charge
capacitance is twice that energy at this unit voltage. Common excitation energy
changes 0.043491% and 2.504275%, also below 5%. Full matrices and energies are
retained, including coarse 69.093966 fJ. This common capacitive response dominates
the huge Z entries; subtracting them does not establish accurate through-mode or
opposite-drive impedance. No such subtraction is presented as a measurement.

Boundary near-region audit uses local [-1,-1,-1] to [1.726,1.352,2.6] mm:
all 2842 vertices and 12981 tetrahedra match exactly by coordinates/connectivity,
with no added/removed near cells. This comparison preserves the checked near
region, rather than silently assigning retriangulation changes to the boundary.
Outer mesh necessarily changes. Vertical refinement only probes z resolution;
no independent XY convergence or complete-model subspace error is established.

Analytic reference: the production `CapacitanceOperator` assembly, direct sparse
factor, charge reactions and integrated field energy solve a coax annulus with
inner radius 1 mm, outer radius 4 mm, length 2 mm, epsilon_r=2.5. Inner trace is
1 V, physical outer conductor trace is 0 V; end faces have natural zero normal
flux, exactly matching the z-invariant analytic problem. Straight polygons with
32/64 sides and 8/16 logarithmic radial intervals approximate the circular walls;
one axial interval is sufficient for this invariant problem. Nonzero radial
field varies with radius. Analytic C=200.651839655 fF. Charge/2-energy results
201.794800018/201.794800018 fF (0.569624%) and
200.937682334/200.937682334 fF (0.142457%) pass 5%. Charge balances are
1.25e-16/2.51e-16; reaction-energy errors 1.00e-15/6.28e-16. No guard was
relaxed. This is a homogeneous reference, not an independent multilayer or
laboratory STM32 validation. `electric-analytic-result-v1.json` retains numbers,
boundaries and the supervised 8-test result (0.145146 s, 11.274 MiB).

A short primary-source search used one query plus the two assigned URLs.
[NIST SP 250-76](https://nvlpubs.nist.gov/nistpubs/Legacy/SP/nistspecialpublication250-76.pdf)
provides 3-terminal calibration procedures, reference values, frequency response
and uncertainty budgets; its four-rod calculable standard transfers the unit to
the Farad Bank at approximately 1592 Hz. It does not supply a complete simple
CAD/material/contact dataset for reproducing a particular measured capacitor.
[NIST geometrical-imperfection model tests](https://www.nist.gov/publications/model-tests-investigate-effects-geometrical-imperfections-nist-calculable-capacitor)
report relative standard uncertainty 2e-8 and sensitivity to blocking electrodes
and bar taper: the real geometry/guards matter. The assigned
[historical NIST overview](https://pmc.ncbi.nlm.nih.gov/articles/PMC4865284/)
was browser-challenged; the search excerpt confirms the four-bar geometry but
is not used as a measured numerical reference here. No full geometry, epsilon,
frequency, contact/guard model, measured value and uncertainty tuple suitable for
a simple reproducible laboratory comparison was found in this bounded search.
Analytic verification passes; laboratory validation remains separate.

Visuals (generated and viewed) under ignored
`debugging/p7-board-integration/electric-convergence-v1-visuals/`:
`geometry-boundary.png`, `capacitance-z.png`. Labels explicitly describe
convergence in the fixed subspace, not a true 88.8% physical error estimate.
Raw geometry, stdout, stderr and process/hash records remain in
`electric-convergence-v1-{coarse,candidate,grid,boundary,tests}/direct-fem/`.
User `native/analyze/debugging/` was untouched.

Checks: release focused test build 3m22s, example build 3m09s including lock wait;
8 field integration tests pass in supervised replay (initial single-test run was
replayed to add process RSS supervision). `npm run native:build` passes (8.58 s
Cargo phase); no library or addon behavior changed. Python AST/JSON validation
and report generation pass. Existing compiler warnings remain. No expensive
magnetic/full-board sweep, worker/cache, P8 or P11 run was started.

Reproduce after the separate release builds:

```powershell
python tests/fixtures/analysis-local-extraction/board-integration/single-method-run.py unique-coarse field-coarse
python tests/fixtures/analysis-local-extraction/board-integration/single-method-run.py unique-candidate field-candidate
python tests/fixtures/analysis-local-extraction/board-integration/single-method-run.py unique-grid field-grid
python tests/fixtures/analysis-local-extraction/board-integration/single-method-run.py unique-boundary field-boundary
python tests/fixtures/analysis-local-extraction/board-integration/single-method-run.py unique-tests electric-tests
```

Actual packet prefix was `electric-convergence-v1-`; the solver-free report script
uses that prefix. Binary SHA256 is recorded per run; electric example:
`632375f85bde90b650c50cf97ee48b8d86879e8052a15a59134670a4be72fd36`.
The remaining gap is independent real-ROI/full-model accuracy: these limited
vertical/domain checks plus homogeneous analytic validation cannot certify all
spatial directions, magnetic reduction, multilayer physical inputs or laboratory
agreement. Stop here; full P7 is not closed.


## Accepted electric v1 and library integration - 2026-10-28

The user accepted the refined electric v1 configuration after `2366190`; earlier coarse 88.8% wording above is historical and does not prohibit integration. [The active assignment](next-task-p6-p7-integration-cache.md) requires complete library/worker/direct DSL/cache delivery and three real boards. [The integration ledger](p6-p7-integration-acceptance.md) distinguishes the library orchestration checkpoint from the outstanding integration/physical/recovery gates. Original current geometry and ports are preserved. No method/port change or renewed electric-convergence campaign is introduced.


## Final conditional integration supply — 2026-10-29

The [current obligation matrix](p6-p7-integration-acceptance.md#final-obligation-matrix) replaces the earlier0/3/library-only status. Original STM32's17 frequencies/51 direct excitations and independent full/reduced voltage/current/cell/region-loss comparisons now PASS5%; numerical guards remain unchanged. Final production model and both bases exactly match the developer packet. No renewed electric convergence or alternative solver search was used.

Existing channel DSL, addon/session and compiled workers now perform finite board extraction and P6 recovery; bounded memory/disk cache and fresh-process restore are measured on real boards. Three actual reports were run and viewed (two successful local volumes plus one supported BandAmp RF section with an explicitly refused local class). Final successful cold families147.806/198.620/8.544s/<2GiB; the same-binary300.055s STM32 refusal remains visible. Schema2 developer persistence round-trips every IEEE bit and actual restore skips all51 completed points/training, with checksum/identity/migration lineage preserved.

Whole P7 remains open for larger/arbitrary PDN, the actual BandAmp ground-via factor-plan refusal and common-band full line/discontinuity channels. Compatible DC endpoints and numerical dynamic RLC compatibility do not silently extend production1kHz..100kHz to a physically validated transient band. P8/P11 scope/gaps are explicit in the final matrix; their implementation is not started.
