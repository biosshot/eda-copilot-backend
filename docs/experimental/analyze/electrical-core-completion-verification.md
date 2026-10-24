# P6 completion verification and remaining scope

Worktree `analyze-p6-completion`, branch `codex/analyze-p6-completion`, base
`c8b6617`,2026-10-24. No commit is created by the executor. Numerical algorithms
are Rust CPU/F64. The shared addon retains the normal placement GPU features.
[Task/design](next-task-electrical-core-completion.md) and
[exact semantics](electrical-core-completion-semantics.md) define the supported
classes; neither this document nor a successful narrow test closes arbitrary SI.

## Current evidence

The bounded implementation is ready for parent review. Final native evidence is
37/37 focused tests (frequency15, coupled9, delay4, relaxation6, unit3), plus the
unchanged strict RL/RLC case rerun through the normal release addon,4/4. The
shared addon was built with default GPU placement features; analysis stays
CPU/F64. Earlier builds and failed runs remain as superseded evidence.

All cargo commands use `CARGO_BUILD_JOBS=1` and the isolated absolute
`debugging/p6-target`; no concurrent heavy compilation is used.

| Check | Actual result | Evidence under `debugging/p6-verification/` |
| --- | --- | --- |
| Original focused electrical debug tests |16/17; unchanged strict damped-RLC case hit its120 s debug deadline | `e1-e2-native-focused.log` |
| New explicit AC native target |2/2 pass | `e2-native-ac.log` |
| First coupled/relaxation target build |44.24 s; coupled5/5, relaxation2/2 | `e3-e4-first-focused.log` |
| Imported nonconstant RC→time; even/odd/different-reference fit |4/4 pass,3.63 s; separate model/holdout/time guards | `e3-normalization-and-time.log` |
| Full-C sphere/P7 oracle, direct frequency, grounded cascade |7/7 pass,0.85 s | `e4-direct-and-composition.log` |
| Matched/reflected/open delay, nonzero prehistory, attenuator cascade, negatives |3/3 pass,0.14 s final warm run | `e3-delayed-first.log` |
| TS exact-analysis/model/port/stimulus adapters |7/7 pass | `final-adapters.log` |
| Final coupled/delay/relaxation after deadline/authorization/band fixes |9/9 +3/3 +6/6 pass; includes mutual-LC time reference and hidden passive resonance | `final-focused-native.log` |
| Shared expired deadline and aggregate retained-source memory |2/2 pass | `final-budget-native.log` |
| Normal compiled package | pass, including declarations | `package-build.log` |
| Final native propagation/band/authorization/resource guards |37/37 pass | `propagation-final-native.log`, `propagation-final-units.log` |
| Strict RL/damped-RLC release retry |4/4 pass; original120 s deadline and tolerances | `release-rlc.log`, `release-rlc/manifest.json` |
| Final TS typecheck and compiled package after provenance fix |exit0 | `final-typecheck.log`, `final-package-build.log` |
| Frequency/propagation regressions after fix |10/10 pass | `provenance-regressions-final.log` |
| Synchronous native abort/timeout, process death and recovery |1/1 pass | `cancel-regression-final.log` |
| Documentation examples through production DSL |1/1 pass | `documentation-examples.log` |
| Three direct DSL saved runs |3/3 pass,6 cases/14 plots each | `direct-final.log`, `final-direct-dsl/*/` |
| Saved-result positive and negative audit |4 positives and56 corruptions rejected, including installed output | `saved-result-negative-audit-final.log` |
| Packed clean installed consumer |6 completed cases, expected failed check, F32 placement contract, timeout/cancel/recovery pass | `installed-package.log`, `installed/manifest.json` |
| Numeric JSON → static PNG |3 rendered and visually inspected | `static-render-final.log`, `final-direct-dsl/*/verification.png` |

The debug RLC failure is retained. Its deadline/tolerances are unchanged; only
that case was repeated with the normal release addon.
`electrical-core-release-retry.mjs` transcribes exactly the existing
`rl_step_and_damped_rlc_converge_to_independent_analytic_response` case: both RL
and damped RLC,1e-7/1e-8 relative tolerances,1e-10 absolute tolerance,1e6 iteration
cap, the original120 s default and independent200-midpoint finite-ramp reference.
It invokes `AnalysisElectricalSession.solve`, the same native `Session.solve`
path in the normal release addon. Every actual request/report and original-test/
addon hash is retained. No separate release test harness or full unchanged
native/corpus suite is built merely to obtain a green total.

The first broad TS regression run was57/61. Four failures were diagnosed:
one missing test-output directory, a CRLF-only fixture comparison, the obsolete
SI refusal code, and exponential ancestor validity/provenance duplication.
The two test portability defects and expected code were corrected. The concrete
production fix hashes generated ancestor history while retaining applicability,
conditional status/reason, port/reference mapping and permissions. Original
authored validity is retained; every direct parent still has a full resource
SHA256 link. The10 frequency/propagation tests and focused cancellation test
then pass; the other57 passing cases were not needlessly repeated. No Rust or
native binary changed after the frozen native build.

## Independent mathematics

- Explicit AC: independently evaluated loaded RC complex formulas, source phase,
  current orientation and total absorbed complex power; independent impedance RHS
  remains separate. Finite source R and explicit RC receiver both participate.
- Sampled model: independently sampled nonconstant two-series-RC Y; constrained
  positive weights/DC/asymptote, training/holdout/order gates; one-RC finite-ramp
  closed form checked separately from BE tolerance refinement. Two-port even/odd
  Y and two distinct S reference pairs25/120 and200/35 Ohm are converted with
  independent two-by-two complex algebra; power identity is checked for V/I U.
- Full coupled model: P7's three concentric infinity-reference capacitor shells
  give the analytic two-port Z; nonzero common charge retains outer potential
  and Q²/(2C) energy. Wrong-metal grounding changes transfer. Assembled finite R
  changes the nullspace; exact zero-energy gauges differ from stored charge.
  Full mutual RLC is checked against frozen independent complex entries and a
  nonsingular branch-basis rescaling.
- Delay: finite-front matched and independently summed bounce series with source
  mismatch, finite/open receivers and several round trips; initial steady wave
  history is nonzero when required. The lossless line has exactly zero internal
  dissipative term,1e-11 J final stored energy and <1e-20 J work/loss/storage error
  on the matched reference. An attenuator's positive loss and delay energy are
  reported separately. Frequency cascade matches `0.5 exp(−jωτ)` in all entries.

Primary references and predeclared tolerances are in task§6.4–6.9. No external
solver is a runtime dependency. A hidden high-Q passive resonance between sampled
nodes is a required conditionality test; resolving its peak must reject the
relaxation class. A regular grid cannot certify the original continuous response.

## Direct scenarios, audit and visual protocol

The resource propagation regression covers coupled/delay/sampled sources crossed
with a single network, a reordered single-block connection and a cascade.
Coupled reordered/connected cases reject; complete ordered analytic cases retain
their DTO and permissions, other cases produce bounded frequency-only samples.
It checks renamed unequal reference dictionaries, factory/reuse round trips,
port order, conditional validity, every source's permission/validity provenance
and producer-band refusal. Native tests additionally enforce the declared band
for direct and cascaded delay/table inputs, and DC/front guards for delay time.

| Source | Ordered single network | Reordered single-block connect | Cascade |
| --- | --- | --- | --- |
| Coupled RLC | Original A/B/R/L/C and reference topology, original permissions/band | Explicit refusal | Explicit refusal |
| Real-K delay | Original causal DTO, renamed reference keys, original permissions/band | Frequency-only sampled DTO in actual output order | Frequency-only sampled DTO in actual output order |
| Sampled S/Z/Y | Frequency-only sampled DTO | Frequency-only sampled DTO in actual output order | Frequency-only sampled DTO in actual output order |

Every sampled derived DTO is bounded by its output axis intersected with all
producer bands; it does not acquire temporal closure. Every derived DTO retains
conditional validity and per-source validity/permissions in provenance. Its
references describe the stored coefficients, independently of physical returns.

`tests/fixtures/analysis-electrical-core/scenario.js` is executed unchanged as
standalone and in the saved STM32G0_CAN_RS485/ThunderF722 contexts. R1/U2 contacts
are checked/recorded and explicit authored values take precedence; these are
electrical hypotheses, not automatically extracted copper/PCB channels.

Each run retains exact DSL, source/native/board hashes, implementation manifest,
complete complex AC V/I, both delay port traces, sampled source/fitted Y,
fit/order/KKT/time-refinement histories, source resource identities, quality and
RSS. Expected failed check followed by pass verifies continuation of reporting.
The saved-field audit requires every case, axis, port, unit, validity and plot;
negative mutations cannot repair missing arrays from another run. The installed
consumer uses the packed addon/compiled workers without TS source dependencies.

Static PNGs are generated solely from those numerical JSON arrays and independent
references. All three were visually inspected: titles/axes/units and legends are
readable, AC amplitude/phase are smooth, RC reference and refined traces overlap,
errors remain below the declared line, both delayed ports show the expected
matched/reflected fronts, and all four cascade magnitudes are displayed. Identical
traces across board contexts are expected for the identical authored network.
Browser/HTML automation is blocked;
interactive HTML controls are **not** reported as verified.

## Reproduction and identity

All paths below are relative to this worktree. `final-review-manifest.json`
under `debugging/p6-verification/` records the current full source snapshot,
the frozen native-build snapshot, unchanged addon SHA256, compiled/installed
package files, every retained result/plot/log SHA256 and byte count. Per-run
`implementation.json` and metadata bind the exact DSL, source, addon and board.
Native-build source snapshot:
`0ef9570783b68048364ef190a618427cb763410cc9bb64782c89586bf4e4b938`.
Addon SHA256:
`c9ab7e130319ef167c33942ebf4ce563d10c577387d2c553d0be0abf6a0a2f6e`.
Only `frequency-executor.ts` differs in production source after that snapshot;
the final package build and new direct runs include its provenance fix.
Current production snapshot:
`d871647d4edbba79a9e98916e43cd3b34f99fb2a4dc2df369a21b325ad77e038`.
Packed archive SHA256:
`c9e4ca15e56a1f78416b74ff4c3b72851a49f7f2df2288c04fbb9504c027331c`
(11,517,111 bytes;34,095,304 unpacked). The49 installed files and38 compiled
files are individually hashed. Final rendered PNGs are byte-identical to the
three inspected PNGs before the metadata-only provenance correction.

Final commands (all exit0):

```
npm run typecheck
npm run build
node --import tsx --test --test-concurrency=1 tests/pcb-analysis-frequency.test.ts tests/electrical-resource-propagation.test.ts
node --import tsx --test --test-name-pattern="abort/timeout kill" tests/pcb-analysis-electrical.test.ts
node --import tsx tests/helpers/electrical-core-verification.ts debugging/p6-verification/final-direct-dsl
node tests/helpers/electrical-core-package.mjs
node --import tsx --test tests/electrical-core-audit.test.ts
python tests/helpers/electrical-core-render.py <three final-direct-dsl/*/result.json paths>
```

For the saved audit, `P6_SAVED_RESULTS` is a semicolon-separated list of the three
final JSON paths and `installed/consumer/result.json`. This mode reads saved
arrays only, uses the same independent audit and14 negative mutations per run,
and never reruns native math or substitutes missing output. Installation uses
`npm pack --ignore-scripts` followed by `npm install --ignore-scripts --no-audit
--no-fund` in a separate consumer. The archive contains exactly one `.node`.
No source TS runtime is used by that consumer. Per-solution `resources` contains
OS process high-water RSS; it includes Node and previous solves, not isolated
per-case allocation. Native conservative allocation plans and budget guards
are separately retained in complete numerical solutions.

## Original P6 matrix and downstream implications

These rows mirror every original P6 checkbox in `implementation-plan.md`.
Acceptance remains subject to parent review; the broad original P6 checkbox is
not marked universally complete.

| Original P6 item | Implemented bounded behavior | Remaining limit and implication |
| --- | --- | --- |
| MNA, DC/AC/transient, initial/events/error/sample independence | Existing sparse RLC MNA retained; explicit spectra and finite source/load response added; full descriptor and exact-delay paths use existing worker/addon | No general dependent-source/SPICE family or nonlinear operating point. New temporal paths require finite continuous fronts; existing MNA event semantics retained. Feedback regulators require a further justified model before using them in P8 |
| Simple models, supply/load/driver/receiver/R/C/L and optional PartUuid resolution | Finite linear driver, passive RC input, ideal source/R/current load; explicit component pins; authored→exact board nominal→provided exact PartUuid catalog, strict parasitic semantics | No automatic ESR/ESL/DCR/band/datasheet discovery, active nonlinear load, IBIS/AMI or remote model fetch. These are model-class limits, not a requirement for a full BOM |
| Multiport/S/Z/Y/connect/Touchstone/references/complex series and applicable checks | Accepted P6b frequency behavior retained; exact real-K analytic frequency model and direct full coupled descriptor added; grounded RLC and all-delay time composition separately tested | Infinity/noncanonical coupled joins need explicit physical terminal mapping. Mixed distributed-delay/lumped/relaxation composition is unsupported. These matter for general P7 channels/PDN and full P8/P11 integration; a narrow single-block or homogeneous cascade does not close them |
| Universal a.model NetworkResource, direct/catalog equivalence without separate attenuator API | Existing immutable factory retained, additive temporal/coupled/delay data classes; direct source identities/hashes preserved | No new attenuator builder or analyzer. Producer analyses/validity/band are never expanded merely because evaluation succeeded |
| FFT/convolution/normalization/windows/time response/fitting as needed; appropriate passivity | Exact delayed-delta convolution and constrained positive-real RC fitting provide positive imported/supplied-to-time paths. Model, holdout, time and power/charge errors are separate; ordinary frequency use has no temporal requirement | No general FFT/window/IFFT reconstruction or resonant/active sampled fitting. Unknown DC/high-frequency behavior is not invented. Lossy P7b finite-band tables do not gain broadband time applicability. A fitting method supporting those cases is needed before corresponding full P11 RF/TDR claims |

The current P7 physical extractor still owns geometry, field applicability,
convergence and full-to-reduced recovery. A mathematically accepted coupled
coupon does not establish that any real PCB extraction has been accepted.
Future eye/timing can process an already justified voltage trace; those P11
operations are outside this P6 implementation. No broad P6/P7/P8/P11 checkbox is
silently marked complete from these restricted positive cases.

| Remaining capability | Classification / downstream gate |
| --- | --- |
| Physical terminal maps and infinity/noncanonical descriptor joins | Required when future P8/P11 composes those extracted models with sources/loads; single-block success is insufficient |
| Mixed delay + RC/RLC/relaxation composition | Required for corresponding general P11 channels or mixed distributed/lumped P8 models; homogeneous supported cases remain usable |
| Broadband/DC and constitutive applicability of real extracted models | P7 producer evidence required before P8/P11 time claims; a solver residual or finite-front screen cannot supply it |
| Passive resonant sampled fit, active stable sampled fit, arbitrary inferred delays | Model-class extensions; required only if a future P8/P11 case actually supplies such a model, not prerequisites for every explicit descriptor/delay case |
| FFT/windows/IFFT, arbitrary incoming wave history, full nonlinear SPICE/IBIS-AMI | Scope extensions, not implemented by this bounded completion |
| Board SI, TDR/eye/timing and full PCB PI | Future P11/P8 products; not implemented or claimed here |
