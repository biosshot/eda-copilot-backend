# Thermal verification

Review evidence for the thermal worktree; this document is not parent acceptance
or closure of all P9. Mathematical semantics and supported geometry are in
[thermal-semantics.md](thermal-semantics.md). All production thermal numerics run
CPU/F64 inside the existing disposable scenario process and shared addon.

## Transient performance checkpoint (2026-10-24)

The subsequent [single extended TPS convergence diagnostic](thermal-extended-transient-verification.md)
reuses the exact original6b8 runtime and completes in1018.936s native/1022.912s
public wall within the explicitly approved1800s deadline. Spatial1.546711% and
original physical guards pass, same final mesh/missing0; full-field temporal
0.116855% exceeds unchanged0.1%, so temporal acceptance still FAIL. One request,
zero retries/ordinary/Band/controller runs; no production budget/algorithm change.

The subsequent [bounded fundamental supernodal prototype](thermal-blocked-solve-verification.md)
is rejected: complete coupon process wall+42.70%, planned/observed peaks grow,
despite full temperature difference2.84e-14K and passing numerical/ownership
fixtures. Exactly one coupon per variant and three short replay repetitions per
variant ran; no full boards. Production remains simplicial; whole P9 stays blocked.

The approved bounded TPS tighter diagnostic finished rejected at native110.064s,
process110.207s<120s, with current-step counters and scalar phase timings retained.
Peak fresh Rust process RSS209932288B/private209354752B are distinct from native
384MiB admission and cannot be compared as a Node-worker memory optimisation.
The exact saved public physics/geometry/policy are retained in the diagnostic
request/preflight; no events, tolerances, meshes, resource caps or guards changed.
The110s diagnostic deadline is measurement only, not full-board acceptance.

New instrumentation is internal/test-scoped: fixed16 stage slots,13 timers,
8 counters. Normal production builds compile calls to no-ops; no new DSL/TS API,
fields, cache, electrical or placement changes. Errors remain rejected; no
partial field guarantee changed. One faithful existing coupon with timers off/on
has bitwise identical complete temperatures/flows/energy/quality/counters/memory.
0.764507/0.757059s is timing noise, not an optimisation candidate or claimed gain.

Measured diagnostic totals: triangular solve51.241s(46.55%), physical operator
20.128s, residual/tangent assembly16.056s, roundoff7.746s, numeric factors2.697s,
symbolic0.078s, apply workspace1.348s. Inclusive linear86.574s/preconditioner52.655s
must not be added to their children. Seed plus partial stage1 counters retain
26322 CG loop iterations,2005 accepted/18 rejected steps,40 factor builds,
6014 cache hits,4 same-dt+tangent selections and0 bypassed exact-dt matches.
Cache selection counts are not all repeated-matrix counts: the two half steps
share C/(h/2)+G for linear constant TPS, while nonlinear cooling can change G.

No safe measured production optimisation is established. A workspace elimination
ceiling1.23%, factor construction2.45% and0 bypassed exact cache matches do not
address the largest cost. The previously rejected single-slot policy is not
restored. A concrete [blocked/supernodal kernel proposal](thermal-transient-performance-proposal.md)
specifies identical factor/RHS replay, memory ownership/admission, matrix-key
invalidation, floating-order/ordinary-pair comparability and Amdahl limits.
Full performance acceptance requests used0/2: the measured-gain and<=600s preflight
conditions are not met. Controller/curve and the prior3600s/1536MiB estimate
remain unaccepted/unexecuted. Whole P9 temporal/curve acceptance stays blocked.

The first diagnostic harness attempt rejected DSL-only `validity` metadata in
native Material deserialisation before any solve. Its request/logs are retained
under `before-input-normalization-*`; removal of metadata preserves coefficients.
Exactly one physical110s TPS diagnostic ran after that input-only correction.
The native request, executable hash, OS observations, scalar profiles and native
source diff are under `debugging/thermal-verification/performance-*`. Local phase
and historical field-context PNGs were viewed; the latter is explicitly labelled
saved ordinary context, never a diagnostic/candidate field. No full field exists
for the rejected diagnostic. Final normal default-GPU addon PASS (jobs1,7m25s);
production integration analytical RC/time refinement, event/cooling switch and
invalid-input checks PASS (three selected tests). Source snapshots, runtime binary
and historical evidence hashes: `debugging/thermal-verification/performance-evidence.json`.
No algorithmic candidate was accepted; this measured blocker package is ready for review.

## Final-policy transient study (2026-10-24; four-request limit)

Parent task805f183 authorizes one ordinary/tighter pair per small board, with
explicit600s shared request budget and unchanged384MiB/50k nodes/2M numeric/
128MiB output caps. Existing default-GPU addon6b86525c… is reused; no native,
cache, API, model, material or numerical acceptance threshold changed. Direct
`thermal-final-time{,-refined}.js` preserve original authored physics, ambient
initial state, source5s/ambient20s/radiator30s events and15s observations. Native
refinement publishes axis[0,5,15,20,30,45,60]; internal steps remain adaptive.
Each request independently checks the accepted steady policy rather than
substituting its steady result. Only time tolerances change from1e-4/1e-5 to
2.5e-5/2.5e-6. Reports remain conditional authored hypotheses.

| Request | Result | Native / public wall | Final nodes / numeric numbers | Planned peak / result bytes |
| --- | --- | --- | --- | --- |
| TPS ordinary | PASS independent spatial1.546716%,missing0,7 solves |566.752 /571.782s|42660 /1899440|338097846 /50454326B|
| TPS tighter | FAIL THERMAL_DEADLINE; no full field |unavailable /600.643s|unavailable|unavailable /4810B error result|
| Band ordinary | PASS independent spatial1.324310%,missing0,7 solves |427.424 /432.055s|42877 /1844070|356627204 /50845037B|
| Band tighter | FAIL THERMAL_DEADLINE; no full field |unavailable /600.661s|unavailable|unavailable /5306B error result|

TPS original nodal4.77615e-12, energy5.53273e-13 and local error0.907865 pass;
Band6.83416e-12,6.46561e-13 and0.993465 pass. Every successful intermediate
comparison has missing0 and published absolute/relative signed witnesses. The
original residual, energy, independent spatial2% and local time-error guards
remain mandatory. Saved actual-field substitutions26/26(TPS),27/27(Band)
reject; restored originals pass, including exact preserved authored inputs.

Measured TPS geometry/assembly/integration phases are42.930,71.057,87.896,
92.862,83.553,80.538,106.812s; comparison/loop overhead1.103s. Band phases
44.543,57.962,56.919,58.621,59.274,59.086,88.471s; overhead2.547s. Factor versus
CG wall times are not separately instrumented, so no subphase estimate is
presented as measurement. Full counters/witnesses remain in per-run checkpoints.
Preflights retain geometry identities, source/script/addon hashes and sizing;
native actual physical/report/symbolic admission remains authoritative before
numeric allocation. The ordinary TPS launcher was600s and completed below it;
subsequent launchers900s leave transport/error-save headroom without extending
the600s physical request. Production defaults stay unchanged.

Both tighter refusals preserve the public total-deadline error. The adapter
checks its deadline immediately after native execution before publishing a
report, so no partial native phase history, final fields or error witness are
available in that result. Native completion/stage cannot be inferred. The
ordinary/tighter temporal pairs correctly FAIL missing complete fields; no
temporal acceptance, invented witness or retry/cap raise is claimed.
Each refusal also rejects attempted promotion of its failed result to complete;
this checks fail-closed audit behavior, not numerical acceptance. Successful
ordinary reports pass26/27 actual substitutions; tighter numerical-substitution
audits remain blocked without complete fields. All four requests, original
failed images and exact raw errors remain saved. No fifth request is authorized.

All four final PNGs were viewed. Successful maps/energy/source/event curves use
only saved numerical arrays; the source-ID caption now shows complete entries
without clipping and points to the full JSON list. Refusal PNGs have readable
explicit error cards; a transparent black-text placeholder found during QA was
preserved before correction. Per-run `render-qa.json` records the separate
renderer hash; numerical/error bytes remain unchanged. Final typecheck PASS.
No unchanged native/addon/package/green steady tests were repeated in this task.
The complete current package is `debugging/thermal-verification/final-mesh-transient-evidence.json`;
new source snapshots, per-request checkpoints/preflights and pair/refusal audits
are alongside it. Heavy slot was returned to P7/P6; this bounded task is stopped.

The curve comparison plan is [thermal-curve-comparison-plan.md](thermal-curve-comparison-plan.md):
intersection field comparison and symmetric-difference area/material/source/
coverage are separate diagnostics; changed area cannot silently disappear from
a full-domain acceptance claim. No controller .01 solve is run.

`controller-final-transient-resource-estimate.json` retains one review-only
estimate:7×measured controller372.160s≈2605s, with a3600s/1536MiB measurement
candidate under the existing140k/4M/128MiB caps. At140004 complete nodes the
measured-ratio estimate is3.781M numbers and allocation1.098GB, not symbolic
admission or a bound on nonlinear fill. Only~8750 nodes remain at the old48.23%
spatial discrepancy. This candidate requires separate review and may still
fail node/memory/output/spatial gates; no full7 controller execution or coupon
speedup assumption is authorized or performed. Whole P9 remains blocked.

## Native boundary checkpoint (2026-10-24)

The native comparator now implements the proven whole-ring CAD correspondence
described below, with unchanged strict P1 interpolation and physical tolerances.
Source identity must match. Hole/interior/removed-cell/wrong-ID/over-bound
negatives remain missing. Absolute and normalized scalar witnesses separately
retain signed coarse/fine temperatures, ambient, time/layer/location and physical
rise denominator. Provenance and witness storage enter memory admission.

Focused native comparator tests2/2, adaptive small-board regressions5/5,
TS comparator/audit13/13 and typecheck PASS. The same frozen24point Band fixture
is strict-missing and proven-localized in both implementations, also checked at
scale2/offset[1000,-1000]. A syntax-only failed compile is retained separately.
Normal jobs1/default-GPU addon build PASS7m21s; JS build and installed package
normal/GPU-disabled/drivers-unavailable modes PASS with the fresh shared addon.
Exact bytes are retained in `native-boundary-source-manifest.json` and its source
snapshot under `debugging/thermal-verification/`.

Exactly one authorized Band policy rerun PASS:
`debugging/pcb-analyze/BandAmpV5/runs/2026-10-24T09-38-41-238Z-thermal-refinement-KRrrAm`.
Six physical solves use one total116.280s/120s budget (outer119.726s), unchanged
50k/384MiB caps, final36526 nodes, planned291133044B. Independent nested control
is0.541754%, every comparison missing0 with48 proven mesh/sample localizations.
Native original nodal5.87646e-13 and energy3.58838e-13 pass; independently
reconstructed signed-flow nodal balance agrees. Full result23542235B<128MiB,
SHA256 `5a7b5893a9d3b97512430cfd895ebe1e80183a4b42f3e7b0a354997718674e5f`.
Worker-reported process peak RSS395780096B is distinct from allocation planning;
the retained external sample is an observation, not a continuously monitored peak.
Saved actual-result negative substitutions25/25 reject, restored original PASS.
Numerical PNG viewed: independent-control/converged labels, convergence/guards,
three layer fields and conditional authored hypotheses agree with saved arrays.
The earlier Band coverage blocker below is historical and superseded by this run.

### One authorized preconditioner coupon A/B: no speed improvement

The preflight declares one SPD slot, unchanged cooling tangent and refresh before
the next CG solve after >32 iterations or an8iteration checkpoint without2x
residual reduction. Admission remains explicit; no mid-CG replacement. The same
10mm layered Cu/resin/via/contact/cooling coupon has source/ambient events5/20s
and output[0,5,20,60], at5000nodes/128MiB/30s per variant. It ran exactly once
through the explicit ignored Rust test; all experimental code is `cfg(test)`.

| Variant | Wall | Factor builds | CG iterations | Planned peak |
| --- | --- | --- | --- | --- |
| Existing production policy A |0.740342s|23|12514|36830775B|
| Single-slot test B |1.148985s|3|23924|36518519B|

Both accept902/reject9 steps. Maximum field difference6.47105e-10K and normalized
2.21352e-10 preserve the unchanged time/physical gates. Local errors are
0.950481763/0.950488865; original nodal4.61666e-12/4.09443e-12 and energy
1.44529e-13/3.24666e-13 pass. B refreshes twice. Factor memory468384→156128B
reduces the total plan only0.85%, while runtime increases55.2% and CG91.2%.
This is a negative performance result, not controller feasibility evidence.
Production cache is unchanged. Preflight, exact executable/hash, full request,
both numeric reports and summary remain in `debugging/thermal-verification/`
(`single-slot-coupon-ab*`). No further A/B tuning/run is authorized here.

TPS/controller accepted steady results were not repeated. Whole P9 remains
blocked on complete final-mesh temporal/curve/three-board acceptance. Controller
full7pass transient and curve.01 remain prohibited/deferred pending new review.
No production commit, merge or broader acceptance is claimed.

## Spatial follow-up (2026-10-24): bounded boundary localization and saved witness

The BandAmp boundary transport defect is corrected in the test comparator only;
native numerics and interior interpolation are unchanged. A fallback requires a
unique cyclic (either orientation) correspondence of the entire transported
physical outer ring to the captured original CAD outline, within the reported
Boolean coordinate bound. Hole boundaries are never mapped to the outer ring.
The sample must lie on the transported edge to binary64 roundoff; it is mapped
to the identical fraction of that same source edge and then uses the original
strict P1 lookup. A real missing cell remains missing. No nearest-triangle
interpolation or enlarged barycentric tolerance is introduced.

The geometry module documents Boolean axis bound
`max(1e-12, spanMm*64/2^29)`; the ring correspondence uses its Euclidean bound
`sqrt(2)*booleanErrorBoundMm + roundoffMm`. Here
`roundoffMm=8*Number.EPSILON*max(1,abs(source coordinates))`. Each individual
displacement is bounded more tightly by
`max(norm(domainA-sourceA),norm(domainB-sourceB))+2*roundoffMm`, because the same
edge fraction is preserved. The saved BandAmp re-audit localizes48 mesh/sample
pairs (the24 original points in both meshes), maximum displacement1.148376e-8 mm,
with every original/source edge, fraction, formula and actual displacement saved.
Physical/boundary missing counts are now both zero; spatial10.1468% still FAILS.

Focused comparator/audit11/11 and typecheck pass. Positives cover verified
transported edges; negatives cover removed physical cells, cutouts, another
boundary, coordinate error beyond the representation bound, and interior points.
Independent actual saved-result negatives remove cells or replace the boundary;
both preserve missing coverage. Commands/logs and full per-point records are in
`debugging/thermal-verification/{steady-pair-focused.log,steady-pair-typecheck.log,bandamp-boundary-localization-reaudit.json}`.

### Executed small-board spatial acceptance and remaining blocker

The authorized ordinary1/.5 mm direct steady pairs both fit the original limits
but fail the unchanged full-field2% gate: TPS7.324923% and BandAmp10.059790%.
TPS runs13200/26871 nodes, outer7.160 s,23962467-byte result, planned peak223033847
bytes; BandAmp19156/31531 nodes, outer35.741 s,31737714-byte result, planned
peak257196215 bytes. Both have zero missing physical/boundary samples after the
verified localization. Original residual/energy guards pass. Full numerical PNGs
were inspected, including separate boundary-displacement metrics.

Preflight recorded unchanged50k node,384 MiB /120 s per solve limits, one128 MiB
scenario cap, bounded numeric sizing and measured conservative output projection.
There is no public symbolic-only API: native physical/report allocation and
sparse symbolic factor admission remain authoritative before numeric factor
allocation; no hidden cap increase was added. Evidence/preflight/commands are
`small-steady-pair-{preflight.json,public.log}` and
`small-next-steady-pair-diagnostics.json` in `debugging/thermal-verification/`.

Exactly one justified existing-policy study per failing board followed:1 mm seed,
maxPasses5,2%, independent.5 mm control, unchanged source/curve/physics and one
total120 s /384 MiB budget,50k nodes. Graded healing-length support addresses the
source/contact/copper-gradient errors seen in the ordinary pairs. No subsequent
policy chain or temporal solve was started.

| Direct policy report | Result | Native / outer time | Planned peak | Full public bytes |
| --- | --- | --- | --- | --- |
| TPS63060DSCR | PASS,6 solves, independent control0.881207%,missing0 |13.703 /16.402 s|255136222 B|20218070|
| BandAmpV5 | FAIL,6 solves, candidate1.302268%,missing24,no independent control |99.409 /101.751 s|235846252 B|16893017|

TPS final32433 nodes /129322 paths has independent signed-flow original residual
8.788756e-11 and normalized energy9.766707e-15. Its process RSS metric351653888 B
is separate from planned allocation. Result SHA-256 is
`59872e868f5670fe7c357436bd40e33d62e9a115afdfe6b2b43ef8f564a96ca1`.

BandAmp final25237 nodes /95710 paths retains complete finite arrays; independent
signed-flow original-equation reconstruction and energy guards pass (reported
original residual1.154814e-13,normalized energy1.916666e-14). Every candidate pair
reports24 missing boundary points, so native policy never admits independent
control despite the last two relative changes0.543775% /1.302268%. It remains
`unconverged-candidate`, not accepted. Result SHA-256 is
`f0b8debc15f6d372480577c1222defa54ad06b577fcd9e6b231c142aaf246f4e`.
The observed OS process working peak469090304 B / sampled private486215680 B are
separate from the235846252 B planned allocation and native RSS sampling.

The remaining blocker is now localized to the native policy comparator:
`thermal/refinement.rs::Interpolator::at` still uses the original dimensionless
1e-9 cutoff without the proven CAD-outline correspondence. Saved final-candidate
self-coverage reproduces exactly24 missing points with the old comparator and
zero with48 verified localizations using the corrected comparator. This is
self-coverage evidence only: previous candidate arrays are absent and no
independent control has been computed. It cannot replace policy acceptance.
The exact native correspondence fix, focused native gap/identity negatives,
normal addon rebuild and one newly reviewed bounded run are the next required
review decision. Native sources were not edited and the allowed one policy
attempt is exhausted; no retry or tolerance relaxation was performed.

New direct source files are `thermal-spatial.js` / `thermal-refinement.js` with
thin named launchers and separate expectation files in each fixture. Original
failed `thermal.js` histories stay unchanged. Report folders are:

- TPS pair:`debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-24T07-49-58-171Z-thermal-spatial-xA5FSy`
- Band pair:`debugging/pcb-analyze/BandAmpV5/runs/2026-10-24T07-50-10-225Z-thermal-spatial-mTHw8T`
- TPS policy:`debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-24T07-52-27-211Z-thermal-refinement-oGodXE`
- Band policy:`debugging/pcb-analyze/BandAmpV5/runs/2026-10-24T07-52-47-292Z-thermal-refinement-vBeVf6`

Each saves complete arrays, DSL/source identities, implementation/addon hashes,
metrics, audit and visually inspected PNG. The renderer now shows actual final
role/converged state; a hardcoded independent-control label discovered on the
unconverged candidate was corrected, and both policy images were regenerated
from unchanged saved numbers, with old images and `render-qa.json` preserved.
Additional checkpoints are `small-one-policy-results-checkpoint.json`,
`bandamp-policy-native-coverage-blocker.json` and
`bandamp-unconverged-field-audit.json`. Commands:
`npm run test:pcb-analyze -- TPS63060DSCR BandAmpV5 --name thermal-spatial`
(exit1,two expected acceptance failures) and the same command with
`--name thermal-refinement` (exit1,TPS pass/Band fail).
No native build, green controller steady, package gate, controller.01 or full
controller transient was repeated. P9 overall remains FAIL; review is pending.

The saved controller transient explains why48.2304% differs from the absolute
5.250010 K witness at60 s. The native comparison normalizes separately at each
output/event time by maximum physical fine-field rise relative to the current
ambient. Fine P1 physical sampling achieves the all-node upper bounds: zero rise
at0/5 s,4.999999213 K at20 s after the ambient step,22.73547041 K at60 s.
Thus the48.2304% gate is at20 s, implied maximum difference2.411520511 K versus
allowed0.099999985 K. This is a real multi-kelvin discrepancy with a5 K norm,
not division by nearly zero. Initial fields are exactly25 C; input watts/energy
and event units remain correct. Coverage is87277 points, four times, zero missing.

The separate global absolute witness is TOP at[-.5234500109959406,10.6828850410187]
mm at60 s, fine34.62978520 C, difference5.250009808 K (23.0917% of the60 s norm).
Its signed coarse value and the20 s relative-witness location were not published:
scalar history intentionally omits seed arrays and currently stores only an
absolute-difference witness. The two possible coarse temperatures are29.37977539 /
39.87979501 C; choosing one would invent evidence. This telemetry limitation is
explicit in `controller-transient-saved-witness-diagnosis.json` and must be fixed
before another accepted transient study. No solve was repeated for this diagnosis.

One concrete design-only performance path is a single retained SPD preconditioner
per mesh with reuse at actual backward-Euler dt, replacing it only on demonstrably
stalled convergence. Keep the actual operator, original residual and local time
error tests unchanged; admit one cache slot before factorization. Current19/21
builds and three slots require215231016 bytes of cached directional numeric
factors; a single slot can remove two thirds of that cache storage. Iterations
and total runtime may grow, so bounded small accuracy/cost experiments and a new
design review are required. This is not an implemented cache or a certificate
that seven passes fit900 s. Full controller transient and curve.01 remain deferred.

## Current checkpoint (2026-10-24): production-policy native steady gate passes

### Fresh public DSL/shared-addon checkpoint

The direct controller `thermal.js` now also passes against the normal freshly
built GPU-default shared addon (thermal remains CPU/F64). The saved run is
`debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-24T06-48-10-805Z-thermal-bgPfdi`.
Its full public result is81623350 bytes under the unchanged128 MiB cap, SHA-256
`04d9b99c65e386d7c572b0652740638f656f6be2bb14e80e653129bcdf1f4357`.
Seven physical solves take355.757 s native and367.701 s outer. Final independent
field change is1.562019%,85441 samples, zero missing physical/boundary samples.
Independent signed-flow reconstruction gives original nodal residual8.368495e-9;
normalized energy balance is8.381802e-12. Maximum planned simultaneous allocation
is860599825 bytes. Observed worker OS working/private peaks are1330847744 /
1363927040 bytes; these are separate from the allocation plan. Complete arrays,
exact DSL/hashes, scalar history, audit and static numerical PNG are saved; PNG
was visually inspected. This steady result does not establish transient or curve
convergence or complete three-board acceptance.

Fresh JS build, named-runner4/4, production thermal TypeScript10/10 (including
observed native abort/recovery), and installed package normal/GPU-disabled/both
drivers unavailable all pass. Logs are `final-production-js-build.log`,
`final-runner-tests.log`, `final-production-ts-thermal.log` and
`final-production-package.log` in `debugging/thermal-verification/`. The final
native build was reused once; unchanged expensive native gates were not rerun.

### Bounded public transient feasibility: not accepted

The separately authored maxPasses1 transient preserves the original source,
ambient/cooling history and ambient initial condition; sampleStep60 reports the
complete event/output axis `[0,5,20,60]` s. Its only two physical solves take
205.229 s (seed) and372.160 s (directional), total native578.312 s / outer591.520 s.
The final mesh has131253 nodes /558000 paths. Actual numeric observation/state
count3544558 stays below4 million; complete public JSON107827766 bytes stays
below128 MiB. Maximum planned simultaneous allocation1047711521 bytes stays
below the unchanged1024 MiB budget but leaves only26.0 MB. Observed worker OS
working/private peaks are1571389440 /1781641216 bytes, separately reported.

Original nodal residual2.579507e-10 and normalized energy3.397550e-13 pass.
The directional physical-field change is48.2304%, with87277 samples at all four
times and zero missing physical/boundary coverage: refinement is unconverged,
there is no independent control, and the acceptance audit correctly fails.
The full seven-pass transient is not started: measured time/memory feasibility
must be discussed without silently raising900 s /1024 MiB /128 MiB /4m limits.
This is no temporal-refinement, continuous-extrema or completed transient claim.

Evidence is `debugging/thermal-verification/controller-public-time-feasibility-20261024-095625/`
(full arrays, authored DSL, implementation hashes, expected failed audit and
visually inspected numerical PNG), `controller-public-time-feasibility-checkpoint.json`
(complete scalar pass history/counters), and the observed-RSS/audit files beside
it. The public steady saved result also rejects18 actual in-memory negative
substitutions and passes again after restoration (`public-steady-negative-audit.json`);
no physical solve or original saved file was changed by that audit.

### Fresh smaller-board public reports

The unchanged TPS63060DSCR direct DSL completes all four steady/time cases in
54.847 s, full JSON28642691 bytes, process peak435830784 bytes. Its full physical
4/2 mm field comparison is7.04927%, so the unchanged2% spatial acceptance audit
correctly fails even though the solves complete. Preserve this failure before
choosing a finer explicit mesh pair; junction-only agreement does not replace
the physical-field gate. Full arrays/DSL/hashes and the viewed numerical PNG are
in `debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-24T07-08-41-155Z-thermal-pvz5IP`.

The unchanged BandAmpV5 DSL also completes all four cases (119.269 s,
52118076-byte public JSON, process peak746545152 bytes), but strict interpolation
reports24 missing boundary samples. Saved-field diagnosis also finds10.1468%
physical-field change, independently failing2%. The two existing complete-field
temporal refinements are0.095339% (TPS) and0.055284% (BandAmp), both within0.1%.
Their successful temporal gates do not establish spatial convergence.

The original BandAmp missing points lie on rounded outer corners. Both meshes retain every
layer value on the nearest triangles; their measured geometric separations are
3.182188e-10..5.261859e-9 mm. The current dimensionless barycentric cutoff rejects
these transported boundary coordinates. The bounded correspondence correction
and independent re-audit are recorded above; the original failed artifact stays
unchanged, and physical spatial acceptance remains failed.
Evidence: `debugging/pcb-analyze/BandAmpV5/runs/2026-10-24T07-09-39-082Z-thermal-VLJ8Rk`
(full source/arrays/hash manifest/viewed PNG), and
`small-boards-fresh-field-diagnostics.json` / `bandamp-boundary-nearest-triangles.json`
under `debugging/thermal-verification/`. The diagnostic comparator is a retained
copy with missing-boundary locations added; production comparison is unchanged.

Parent accepted the single controller production-policy measurement. The exact
opt-in policy is seed1 mm / curves.005 mm, maxPasses5, relativeTolerance.02,
independent validation.5 mm, explicit140000 nodes /600000 links /1024 MiB and one
shared900 s deadline. Defaults and physical guards are unchanged. Seven physical
solves are reported: seed76713 nodes, then89991 /96120 /98151 /98406 /98460,
then125799 independent-control nodes. The candidate change is1.269281%; the
independent-control change is1.562019%, with85441 sampled points and no missing
physical/boundary coverage. Earlier failed meshes below remain failures.

Independent reconstruction from the complete signed flows gives original nodal
residual5.198659776e-9. Input/output are.8 /.7999999999904466 W; energy imbalance
is9.55347e-12 W (normalized5.96719e-12). Tjunction119.8211271 C and
Tcase117.4211271 C are conditional authored-model results. The final map, full
coordinate frame and scalar history are saved; sampled convergence is not a
continuous or absolute-error certificate.

Maximum simultaneous planned allocation is862151557 bytes, including retained
previous field and coordinates; no automatic cap increase occurred. Native solve
time441.870 s, outer443.108 s. Observed OS working/private peaks are1074384896 /
1211645952 bytes; the allocation plan is explicitly not an OS memory cap.
The raw native result is73279986 bytes, SHA-256
`332de3205c6cda8cb0a6f75d9eb842f5e8fee24f035918fef872a7f0f74f0858`.
Evidence is `controller-production-policy-steady-{result,audit,checkpoint,observed-rss}.json`
under ignored `debugging/thermal-verification/`, with implementation manifest and
the exact retained executable. Final small board regressions5/5 and comparison
unit1/1 pass in `adaptive-small-final-native-rerun.log` and
`adaptive-final-lib-comparison.log`.

The native evidence below is complemented by the fresh public steady checkpoint
above. Three-board reports, curve pair and controller transient acceptance remain
pending. The controller direct steady DSL now has a separate
bounded report; complete final arrays are never omitted to fit multiple cases.
Actual native final counts125799 nodes /526470 paths make five transient samples
require4141973 numeric values, exceeding the unchanged4 million cap. That
five-sample plan is rejected before a run. Parent instead authorized explicit
reportStep60 with `[0,5,20,60]` s, all physical events and internal integration
preserved, contingent on measured output/memory/time admission. A bounded staged
feasibility measurement must precede the complete transient sequence.

## Earlier checkpoint (2026-10-23): nested spatial experiment

This checkpoint supersedes pending-run statements in the development history
below. No accepted production refinement, final shared-addon verification or completed
three-board acceptance is claimed. The approved bounded opt-in production boundary is
in [thermal-mesh-production-proposal.md](thermal-mesh-production-proposal.md).

### Small production-policy gate

The implemented policy passes one native comparison unit test and five focused
board regressions. The analytical 1 mm² coupon uses an independently known
uniform through-thickness solution, with 509 seed, 1877 directional and 1901
independent-control nodes. The final top-slice mean matches
`25 + .001*(1+50+500+100/3)` C within 1e-7 K; original nodal residual is
1.704e-17 and normalized energy imbalance 2.082e-14. Its three analyses take
97 ms with maximum planned peak 106863663 bytes. The published map belongs to
the independent control; child containment and diameter checks cover all fine
cells, and intermediate history contains scalar summaries without full maps.

The transient coupon compares every output and power/ambient/cooling event at
`[0,.07,.1,.11,.13,.2]` s. Zero heating at an absolute ambient of 1000025 C,
common-origin translation, deliberately exhausted convergence, incompatible
continuation, invalid policy, shared timeout and memory refusals also pass.
Before board waveform/table fanout, admission accounts for all cloned signal
payloads; valid large PWL and cooling inputs refuse before cloning into every
mesh node, while the ordinary small constant-signal baseline fits 1 MiB.

Evidence: `adaptive-small-native-corrected.log` (one lib and four successful
board cases; a fifth initially used malformed PWL input), followed by
`adaptive-small-fanout-valid-table.log` (corrected valid-input memory refusal).
Intermediate failed test-input/admission-size attempts remain saved. TypeScript
typecheck passes in `adaptive-typecheck-final.log`; seven physical/refinement
audit regressions and two report deadline/publication regressions pass in
`adaptive-audit-tests.log` and `adaptive-total-deadline-ts-tests.log`.
The additional `adaptive-whole-run-deadline-tests.log` has two passing tests:
delayed preparation progress prevents executor entry; an overdue executor cannot
publish a completed result; overdue report serialization cannot mutate the
store. `adaptive-whole-run-typecheck.log` passes after these additions.
This admits one explicitly approved controller steady measurement only; it
does not validate the stale shared addon, the controller policy or transient
corpus. Its native direct-request ordinary-node limit has an additional guard
whose newly added regression awaits the next focused build.

The bounded directional experiment continued only as authorized and stopped at
pass 5. Every row retains the original nodal guard; the maximum below is the
bidirectional physical-field sampling difference from its predecessor, normalized
by the finer board maximum temperature rise above 30 C, not junction change.

| Level | Replicated vertices | Junction C | Field maximum change | Original nodal residual | Planned peak bytes |
| --- | --- | --- | --- | --- | --- |
| Directional 3 | 103587 | 119.7325563 | 6.4592% | 7.147e-9 | 674175335 |
| Directional 4 | 103848 | 119.7364252 | 3.4004% | 8.145e-9 | 676866271 |
| Directional 5 | 103902 | 119.7371014 | 1.2713% | 9.741e-9 | 675531463 |
| Fresh global .5 CDT | 131706 | 119.9147694 | **5.4061%** | 9.158e-9 | 871463479 |
| Nested global .5 | 131706 | 119.8213016 | **1.6042%** | 9.563e-9 | 851721583 |

The last two rows independently compare against directional 5, not against each
other. Pass 6 was not run. A common-marker last-pair pass alone was insufficient:
93 originally missed regions required additional coverage. Fresh CDT preserved
all candidate vertices but dropped numerical triangle edges. At its worst
existing vertex, support increased 76%, and directional widths increased from
.0804--.1322 to .1001--.2159 mm. Its 4.5663 C discrepancy is retained as
nonmonotone mesh-sensitivity evidence, not proof of monotone refinement error.

The approved nested control retained **all 103686** candidate triangle edges,
split 5416 newly collinear edges and added 9268 genuinely new global .5 vertices.
No candidate vertex moved or merged. All 87266 accepted fine cells lie inside
coarse cells (maximum containment deviation 3.553e-15 mm); summed child areas
recover every parent within 1.776e-15 mm2, and the maximum fine/parent diameter
ratio is 1. Convex containment prevents coarsening in any fixed direction and
makes the coarse P1 space a subspace, up to the reported F64 segment roundoff.
These are computed checks, not certified exact-arithmetic claims.

The nested field maximum is 1.353515 C / 1.604212%, RMS .0460205 C; junction
change is .0937419%. Every one of the 93 historically missed regions was sampled
explicitly (maximum 1.318289 C), with zero missing physical/boundary samples.
Both meshes, boundary vertices/midpoints and inward void traces are covered.
This is a sampled spatial convergence gate, not a certified continuous L-infinity
bound or absolute accuracy guarantee. Independent steady nodal reconstruction
agrees with native residual 9.56337e-9; normalized energy imbalance is 1.46542e-11.
Geometry identity, integrated material area, capacity and conservative contact /
barrel weights are retained in the comparison artifact.

Native time was 69.20 s. The 1 GiB request admitted physical 283314080 + report
reserve 268435456 + sparse peak 299972047 = 851721583 bytes. Observed Windows
high-water RSS was 1050927104 bytes; sampled private bytes reached 1200996352,
**above 1 GiB**. The allocation estimate is not a strict process-memory cap.
No further native build or solve followed this probe; the shared slot was released.

Exact results/plans, actual executable/source hashes and independent audits are
`controller-nested-global-half-{result,plan,comparison}.json`,
`controller-nested-implementation-manifest.json`,
`controller-nested-global-half-observed-rss.json`, and
`nested-coverage-native-test.exe` under ignored `debugging/thermal-verification/`.
Viewed static `controller-nested-local-patches.png` is rendered from those saved
arrays and hashes. Fresh-CDT artifacts and executable remain separately saved.

The stationary reaction diagnostic also passes two bounded analytical tests in
`tests/thermal-stationary-reaction.test.ts`. On a homogeneous thin-sheet sinh
patch, both lumped and consistent stationary interlayer coupling converge below
2% once h/ell=.347; consistent coupling can undershoot the prescribed core on
the coarse mesh. Thus this test does not establish a fundamental consistency
defect or justify replacing production coupling. The diagnostic PNG was viewed;
all original nodal and power checks remain below 1e-11. Strengthened T1 passes
11/11, physical sampler/audit 6/6 (including 38 report mutations), TypeScript
typecheck passes. Final current-addon spatial references and reports are pending.

## Earlier diagnostic checkpoint: fixed halo and first directional passes

Original nodal equilibrium is now a separate immutable `1e-8` guard. The former
uncapped Newton max-diagonal floor admitted a mixed stiff/weak network with
zero global imbalance and a 100 C local error. Persistent common-origin
coordinates now solve that counterexample correctly: c=225 C, stiff-link flow
0.9999999999999973 W, normalized original residual 1.48e-15. The steady saved-flow
audit independently reconstructs all FEM/contact/barrel/compact/surface nodal
balances. Earlier probes predating this guard remain diagnostics, not acceptance.

After fixing local constraint-edge sizing in a **cfg(test)-only** physical-halo
experiment, the latest admitted controller cases are:

| Global / local spacing, mm | Replicated mesh nodes | Junction C | Original nodal residual | Planned peak bytes |
| --- | --- | --- | --- | --- |
| 1 / .08274585 | 82176 | 118.0425857 | 6.98e-9 | 583766807 |
| 1 / .0625 | 102399 | 118.2530554 | 7.90e-9 | 664763823 |
| .5 / .08274585 | 109704 | 119.0550296 | 5.21e-9 | 700813167 |

The physical halo remains four explicit background healing lengths (ell=.16549
mm) and does not shrink during global refinement. Added collinear constraint
vertices preserve boundary area, materials and connectivity; the independent
reaction/diffusion sinh coupon confirms that resolving this layer is necessary.
Nevertheless the last global pair still fails full-field sampling: maximum
8.06609 C / 9.6432%, despite junction change 1.1369%. Worst TOP [3.5,7.5] is a
source-free resin triangle beside warm copper **outside** the source halo, with
matching exact fractions [0,1,1] and harmonic coupling density. Geometry SHA is
identical, integrated area differs 1.32e-7 mm2 and mass 3.95e-10 J/K. No unresolved
material/topology discrepancy explains the field error.

Evidence: `controller-constraint-halo-factor-separation.json`,
`controller-constraint-halo-physical-comparisons.json`,
`controller-constraint-halo-global-local-patch-audit.json`, viewed static
`controller-constraint-halo-global-local-patches.png`. Physical sampling includes
both supported nodal sets, every domain boundary vertex/midpoint and inward
physical traces; missing physical and boundary samples are zero. It is explicitly
not a certified continuous L-infinity maximum.

Parent authorized one/two **test-only** directional midpoint passes selected by
source-independent reaction/gradient curvature. The one-pass plan is 95349
replicated nodes under the same explicit probe caps. It is a heuristic, not an
error guarantee: 93 of 262 sampled bad triangles are missed (worst missed 2.876 C),
retained in `controller-directional-pass1-missed-regions.json` and the viewed
`controller-directional-error-marker.png`. No production adaptive algorithm or
resource increase is authorized or implemented. Two admitted directional passes
finish in 58.54/60.03 s with junction 119.277955/119.660986 C, but their last pair
still fails full-field sampling: 7.920619 C / 9.4051%, RMS .19410 C, no missing
physical/boundary samples. Original saved-flow nodal residual is 9.87594e-9 and
energy imbalance 2.27e-13. Exact plan for pass 2 is 667862383 bytes; observed
high-water RSS 827006976 and sampled private bytes 854077440. Pass-1 RSS sampling
was incomplete and is not presented as its peak.

The final worst is a new midpoint [-.7201750,10.5713175]. It was not missed by
its ancestor markers: base h_parallel/ell=3.76..5.16, eta32.7..61.8 K; pass1
2.11..2.63, eta12.05..18.70 K; pass2 neighboring cells still .85..1.38 ell.
See `controller-directional-worst-history.json` and viewed static
`controller-directional-local-patches.png`. A third pass was subsequently
approved only after candidate admission: 506 marked triangles, 685 added planar
vertices, 34529 planar / 103587 replicated. This was a pending-run checkpoint;
the subsequently authorized results and independent nested gate are recorded above.

The requested all-interface geometric planner estimates 187648 planar / 562944
replicated candidates at ell/2 and 372207 / 1116621 at ell/3, with edge sizing and
fixed graded 4ell halo. It allocates no CDT/physical matrix/factor. A subset using
midpoint-classified physical Cu/resin interfaces hardly reduces those counts;
conservative all-contour cases retain every relevant interface plus Cu/void
edges. Empirical cost extrapolation is not symbolic admission; this expensive
full-interface solve path was declined. `controller-all-interface-mesh-plan.json`
retains the complete candidate/work ledger and explicit scope.
 Acceptance still requires actual
physical-field convergence, original guards and conservative coefficient/weight
rebuilds. Production source refinement remains unchanged while this experiment
is evaluated. The final shared-addon/package and three direct board reports
remain pending.

Strengthened T1 tests pass 11/11, including mandatory correct mixed stiff/weak
solution, 1e6 K translation, varying ambient, nonlinear cooling, transient energy
and verified continuation. The physical sampler/audit tests subsequently pass 6/6, including explicit ambient normalization,
absolute-temperature shift invariance and 38 report-negative mutations.

### Earlier global-refinement diagnostic


The explicit controller .5 mm resource probe completed but **failed** the unchanged
2% spatial target. Junction 1 mm / .5 mm is 116.31017995 / 118.24734560 degC,
relative rise change 2.19515%. Bidirectional P1 nodal sampling finds 13.02472 degC
maximum difference (15.6029% of fine board maximum rise), RMS .68005 degC and
p99 2.25949 degC. RMS does not replace the maximum criterion; this sampling is
not a certified continuous maximum. The initial 4/2 and subsequent 2/1 failed
levels remain evidence. No three-board completion or final addon acceptance is claimed.

Raw exact requests/results and hashes are under `debugging/thermal-verification/`:
`controller-mesh1-resource-result.json`, `controller-mesh0.5-resource-result.json`,
`controller-mesh1-05-field-comparison.json`, `controller-local-patch-audit.json`,
`controller-local-paths-and-missing.json`, and viewed static
`controller-local-patches.png`. This image is generated from numerical/geometry
arrays, not a browser screenshot. Both geometry reports have SHA-256
`62fe677a0fd65c441a3827b4cc7f79ac1e0d40e41efc4467fcc3206a75287df8`.

| Probe | Physical nodes / planar triangles | Native time | Total allocation plan | Observed Windows high-water RSS |
| --- | --- | --- | --- | --- |
| 1 mm, explicit 100k / 384 MiB | 56232 / 40782 | 43.076 s | 390390439 B | 490934272 B |
| .5 mm, explicit 110k / 768 MiB | 100575 / 70446 | 51.208 s | 667372439 B | 832598016 B |

The .5 plan partitions physical 221320480 + report reserve 201326592 + sparse M
244725367 bytes; 407083 links, 969403 admitted input pairs, fill 6080451 and 3 CG
iterations. OS measurements are observations, not a strict RSS cap. Limits are a
separate authorized measurement probe, not an approved final fixture resource change.

Worst locations TOP [-3.5,-1.5] and [2.4897585,6.79475] are resin, with exact
stored copper/core/bottom fractions [0,1,1], matching vertical conductance density
and no local barrel path. Large background supports bridge a steep transition
from hot pads. Local h and halo shrink .125 -> .0625 mm along with global h;
these factors are not separated in the failed comparison. The explicit TOP resin
sheet k/t and harmonic core coupling imply healing length .1655 mm. A test-only
mesh-cost experiment now separates global/local spacings and studies a fixed
4-healing-length halo with graded bands; production meshing is unchanged pending review.

Sixty missing interpolation entries are twenty planar basis vertices across three
layers: nineteen lie inside explicit bore/barrel voids; one lies 3.02e-8 mm from
an NPTH boundary. Their coarse basis has nonzero support in adjacent physical
material while the fine containing cells have zero clipped physical area. They
are retained with classifications, not replaced by zero or discarded to pass.
Comparison on common physical material and hole traces is required before acceptance.
Integrated area differs 1.31e-7 mm2, TOP/bottom copper areas differ 1.61e-7 / 9.91e-7
mm2 and capacity differs 3.89e-10 J/K; .8 W is conserved to about 1e-12 W.

After direct-index barrel mass accumulation, complete temperatures, flows,
ledgers, node/link manifests and final field capacities match legacy 1 mm arrays
exactly (`controller-index-optimization-invariance.json`); native time decreases
75.000 -> 43.076 s. Streamed SHA-256 reduces state identity 38621524 -> 88 bytes
and native result 74324686 -> 30569274 bytes. A canonical small-reference unit,
T1 8/8, mesh 3/3 and finite-barrel/continuation 1/1 pass after these changes.

The older successful addon/package results below predate the final Rust solver,
meshing and identity changes. They are historical integration evidence only;
the corrected standard shared-addon build and named reports remain pending.

## Native mathematical gates

`CARGO_BUILD_JOBS=1 CARGO_TARGET_DIR=debugging/thermal-target cargo test
--manifest-path native/analyze/Cargo.toml --test thermal --test thermal_board --
--test-threads=1 --nocapture`: exit 0, T1 7/7 (0.53 s), T2 6/6 (171.66 s).
Saved summary: `debugging/thermal-verification/native-focused-summary.txt`.

T1 covers one-node exponential and two-node eigenmodes, temporal refinement,
finite power pulse, ambient/cooling events, tabulated cooling steady reference,
range rejection, continuation identity, floating boundaries, missing capacity,
invalid tables, budgets/recovery and near-zero balance/physical signal guards.
The relative analytic temperature-rise guard is 0.1%, separate near-zero absolute
allowances; linear residual is <=1e-8 and normalized energy imbalance <=1e-6.

T2 covers exact homogeneous and dissimilar-layer harmonic slab resistance,
source watts/capacity, independent Fourier spreading, required missing-property /
geometry / contact rejection, finite via kA/L, bore-excluded mass and finite barrel
power conservation. Independent test-only layered FV uses harmonic face
resistances and the same layer-center positions, source-area average and Robin
boundary as the FEM; it is not another production engine.

High contrast case: 400 versus 0.3 W/(m K), 0.08 mm strip, source rectangle
x=0.4..0.8 and y=0.85..1.15 mm on a 2 x 2 mm board; layer thicknesses
0.02 / 0.1 / 0.02 mm, total exterior resistance 200 K/W. Nominal FEM spacings
0.0625 / 0.03125 / 0.02 mm. FV refinement 192 -> 256 cells per direction:

| Strip center, mm | FV coarse / fine, K/W | FEM coarse / middle / fine, K/W | Fine reference error | Final change |
| --- | --- | --- | --- | --- |
| 1 | 1605.00596 / 1602.29972 | 1493.10627 / 1554.89758 / 1575.69645 | 1.6603% | 1.3200% |
| 1.0125 | 1609.34534 / 1606.85977 | 1485.28456 / 1558.08902 / 1584.33923 | 1.4015% | 1.6569% |

Both alignments meet the original 2% error and refinement gates; reference
self-change remains below 0.5%. The earlier mesh without copper interface
constraints failed this gate. Copper contours now constrain P1 triangles;
optional points within 1e-9 mm of an existing contour segment are omitted without
moving physical contours. Remaining barrel cut coefficients and 128-point outer
rim interpolation are approximations; these results do not establish convergence
for arbitrary finer traces or 3D package/barrel fields.

## Production and saved boards

Standard `npm run native:build` with CARGO_BUILD_JOBS=1 passed (exit 0,
21m11s), preserving the existing GPU placement host. `npm run build` and
`npm run test:package` passed. The installed consumer runs thermal DSL in all
three normal / GPU-disabled / unavailable-driver package modes. Logs are under
`debugging/thermal-verification/{native-build,build,package}.log`.

Production T1 JSON is `debugging/thermal-verification/t1-production.json`.
Production abort instrumentation observed PID 2828 inside `solveAnalysisThermal`,
confirmed process death before solve-end and recovered with the full T1 scenario
(`cancellation.log`). Native deadline and report budget rejection recover too.
Versioned audit negative tests include 26 mutations, including missing-to-zero
published graph/map substitutions.

The same-context DC transfer test uses two finite vias and the reused canonical
meshReference: 328 triangles carry 0.015800211044329465 W and two barrel segments
carry 0.005592797575812816 W. Thermal input is 0.02139300862014228 W; no contribution
is dropped. Source ID, board identity and original meshReference are retained in
`loss-transfer-production.json`. The first attempt correctly rejected an omitted
explicit dielectric material entry; the corrected input declares it.

Pending: corrected host build, final production/regression passes, three direct
board runs and static visual inspection. The first TPS report correctly rejected
CG residual stagnation; it is a saved failed report, not T3 evidence. Physical
through-bore support is now clipped once per triangle and reused per slice and
interface, avoiding redundant whole-board self-booleans. Source/contact clipping
is done once per target with an RTree candidate query. No tolerance was changed.
Production-lock Rust release gates pass T1 7/7 and T2 7/7, including captured TPS
coarse geometry and all independent references (`final-native-release.log`).
The captured TPS native transient then exposed a 120 s Jacobi-PCG deadline
(`tps-time-release.log`). The corrected bounded sparse Cholesky preconditioner
keeps the original matrix-free physical operator and all error/energy guards.
Its independently checked tridiagonal exact solve, allocation-admission and
nonpositive-factor rejection pass (`sparse-preconditioner-unit.log`). The final
captured TPS time gate passes (`sparse-tps-time-release.log`): 734/1476 accepted
steps, 12/11 rejected, 28261/50556 CG iterations, 27 factors and zero fallbacks,
16,610,791 planned factor bytes. Wall times including preparation were 37.77/54.83 s.
Junction time-refinement change is 0.0223113%, below the original 0.1% gate.
IC(0) was rejected during development because its shifted factors fell back on
this stencil; it is not the production preconditioner.

The controller's original .001 mm curve preparation exceeds the unchanged
50,000-node budget (`controller-steady-release.log`). The explicit authored
`mesh.geometryDeviationMm` addition preserves that default and the DC range
.000001.. .01 mm; it never changes tolerances automatically. Controller acceptance
requires separate .01/.005 mm curve and fixed-.005 mm 4/2 mm mesh gates, both <=2%.
The first .01 mm probe exposed a third-party CDT incremental crossed-constraint
assertion, including with exact endpoint insertion. Explicit robust planarization
of all projected contours then passed its isolated controller test at both curve
tolerances (12196/14501 planar points, 13751/16596 constraints), without moving
physical coefficient polygons. Full thermal curve/mesh gates and the final
host/three-board runs remain pending.
No completed T3 or complete P9 claim is made while the remaining gates are pending.
The authored scenarios retain geometry/source identity and label all thermal
properties and power as conditional hypotheses. HTML controls are unverified;
static PNG/SVG are generated directly from saved numerical arrays, not browser
screenshots. Browser local-file rejection is not bypassed.

## Limits

No CFD, electrical-temperature feedback, material laws depending on temperature,
3D package solver or datasheet-derived defaults. First spatial class supports
alternating copper/dielectric slices and round through bores; slots, blind/buried
spans and unconfirmed drill support reject. Loss transfer is same-context only,
from authoritative completed DC solutions with full computed coverage and checked
meshReference; missing fields cannot become zero. A successful conditional hot
solution can fail a check and continue; numeric failure rejects the calculation.

Early development Node runs that required dist failed because the worker asset
had not yet been built. They are prerequisite failures, not accepted numeric
evidence; final production/regression runs must replace them below.

## Earlier corrected-gate development history

The full bounded run `thermal-bounded-final-release.log` passes lib 8/8 and T1
8/8; spatial 8 pass, two controller cases reject the 50000 unpruned mesh-plan
limit, and one explicit exporter is ignored. Controller curve .01/.005 mesh4
solves in 3/4 CG iterations with Tj 112.1349123 / 112.1328818 degC. All physical
residual/energy guards remain unchanged. A fixed h/4 local-aid redundancy probe
did not resolve fine-mesh admission and was reverted to preserve mesh quality.
The mesh-only point plan records 14790 mandatory contour vertices, 15341 with
global samples, then an upper bound 17742 planar / 53226 replicated slice nodes
before support pruning, using the original 48x48 contact lattice. A separate
explicit maxNodes=60000 native resource probe is in progress under the same
384 MiB request; proposed public maximum 100000 is pending parent review of its
actual counts, memory plan, solve cost and observed RSS. Default remains 50000.

`thermal-final-focused-release.log`: production-lock release lib 5/5 and T1 8/8;
T2 manufactured/FV/finite-via and TPS transient 7/7 pass, while captured controller
and TPS steady fail their unchanged internal numerical/deadline checks. The
corrected high-contrast FEM sequences are 1492.85375 / 1554.88169 / 1575.06408
(error 1.6998%, change 1.2814%) and 1485.12039 / 1558.09982 / 1584.34179
(error 1.4014%, change 1.6563%). TPS transient whole-source junction change is
0.0222286%, with 26.23/43.58 s wall time. This is partial development evidence,
not a successful full gate.

`full-barrel-preconditioner-release.log` shows the provisional 250000-entry
preconditioner heuristic falling back for controller and TPS time; it is not
successful full-M evidence. The next gate uses actual pair/symbolic-fill memory
admission under the same quarter budget and explicit F64 backward-error allowance
capped by the original physical normalized residual 1e-8. Full details and formula
are in semantics; no temperature, energy or convergence guards changed.

The controller A/M audit (`controller-A-M-audit.log`) passes four fixed vectors:
rowwise backward discrepancies 7.4e-16..1.53e-15; barrel-only residual 1.13e-17;
an injected second barrel diagonal produces 1.63e-3 and is rejected. LLT backward
errors are 2.4e-15..6.4e-15; forward error up to 5.27e-4 on an 80+sin vector
confirms conditioning rather than an assembly mismatch. The exact affine steady
correction clears mesh4/curve .01 in 3 CG iterations, Tj 112.1349123 degC, 50.41 s
including preparation (`controller-affine-release.log`). The next .005 geometry
requires 749039 pairs / 2230083 fill / 135403631 bytes for one factor; its rejection
by the original arbitrary quarter allowance is saved in that same failed gate.
Parent approved repartition within the unchanged 384 MiB request: explicit physical
plan + M plan + retained quarter report reserve <= request budget. The corrected
full gate and original three-board acceptance remain pending.

Current TypeScript typecheck and the prior 25-substitution audit contract pass;
the newly expanded 26-substitution contract is awaiting its final rerun; authored
geometryDeviation bounds and loss authority/finite-barrel transfer pass in the
focused Node run. Its native binary predates the final Rust correction, so this
is not final corrected-host evidence. Final production tests will follow rebuild.

