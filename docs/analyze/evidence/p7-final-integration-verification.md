# P7 final integration — work and verification

Base `6f639db`, branch `feat/analyze`, CPU/F64. P7 is **not closed**.
The accepted structural-gauge fix is a prerequisite, not a new investigation.
Historical evidence and the user's `native/analyze/debugging/` are untouched.

## Preflight, recorded before new numerical runs

The first implementation is automatic block-current Krylov congruence for the
existing finite-foil descriptor. It starts with all `R^-1 A^T` columns and extends
with `R^-1 L`; potential coordinates, C, physical ports and gauge/charge modes
are retained. R must be strictly positive diagonal for this implementation.
Two-pass orthogonalization deflates only numerically dependent coefficient
columns. No physical matrix eigenmode is clipped. Full P6 validation and its
guards precede reduction; this is explicitly not the scalable real-ROI path.

| Observable | Independent reference / normalization | Gate fixed before runs |
|---|---|---|
| DC impedance/current/local loss | analytic rectangle `length/(sigma*width*thickness)` and uniform J | 1e-9 relative to nonzero analytic scale |
| All 17 saved full/reduced AC points | frozen independent tensor/box coefficients and 80-digit descriptor from accepted structural-gauge audit; no field reference rerun | inherited 1% per-observable gate and weak-entry absolute floor |
| Automatic basis/order | full P6 complex state/ports, all port unit drives at declared training frequencies; independently evaluated holdout frequencies in tests | declared 1e-6 normalized state and individual loss errors; port state contains every potential and physical current |
| Energy, charge, passivity | exact block congruence plus unchanged full/reduced P6 checks | inherited production gates; no replacement tolerance |
| Initial currents | supplied physical vector versus Q Q^T reconstruction | existing 1e-10 representability gate |
| Basis limits / resources | invalid R, rank/order cap, nonfinite/empty frequency axis, memory/deadline admission | explicit refusal, no partial successful model |
| Physical scope | all original P7 obligations individually listed below | unimplemented obligations remain open |

All new numerical batches are capped at 120 s / 512 MiB, compilation separate;
`CARGO_BUILD_JOBS=1`. Training and holdout axes are disjoint. No expensive debug
board corpus or unchanged expensive reference is replayed. Mesh, quadrature,
copper-scope and air-domain convergence must not be inferred from reduction
agreement; their prior accepted scope is retained.

## Implemented scope

`local/reduction.rs::automatic_currents` selects current moment blocks and an
order from all declared unit-port drives, with separate port, full physical
state and per-coordinate Joule-loss checks. Every potential, raw C, source port,
physical terminal map and structural/conserved certificate is retained. All
physical current coordinates recover through Q; unrepresentable initial modes
still refuse. Positive diagonal R is an explicit supported class. Unsupported
mutual R, active L, false gauges, bad axes and resource/order failures refuse.
This path fully compiles the input before reducing it and therefore supplies no
scalable real-ROI admission.

`layered_capacitance.rs` now retains actual finite void rings, including their
allocation plan and serialized identity. Touching source pieces require both a
finite geometric contact surface and exactly identical explicit potential maps.
Names do not create a connection. All source identities survive shared surfaces;
each physical node reaction is assembled once. Current-space coverage uses the
complete polygon with its voids. A void denotes absent metal; its dielectric is
still the explicitly supplied z-layer profile. The vacuum void control does not
implement an air bore inside FR4. This adds actual finite foil contacts, but no
barrel current/trace or board padstack adapter.

The synthetic parallel-RLC control has three distinct relaxation times and six
physical currents; it selects three coordinates after testing orders 1/2/3.
Analytic complex branch currents, every local loss, both physical potentials,
stored energy and initial-state representability are checked on disjoint
holdouts. The accepted foil control requires more than its two static moment
directions when every unit-port drive and weak internal response is checked.
No gate is relaxed to retain the smaller basis.

### Geometry controls preflight

The next bounded control uses four adjacent finite foil source pieces with
identical explicitly authored interpolation at their shared physical surfaces.
Their DC R, J and loss retain the analytic rectangle 1e-9 gate; uniform normal
electric energy retains the independent tensor 1e-10 gate. Mismatched touching
traces and untraced contacts must refuse. A constrained square-void coupon checks
actual dielectric-cell ownership (288 additional local terms from two holes),
retained serialized void contours, reduction of capacitance, and invalid/missing
hole constraints. This hole control verifies ownership and energy consistency;
it supplies no general annular-via or continuum field accuracy claim. Neither
test changes prior caps, mesh tolerances, geometry or board fixtures.

## Numerical evidence and performance

New evidence is confined to [p7-final-integration](../../../debugging/p7-final-integration/).
[automatic.json](../../../debugging/p7-final-integration/checked/automatic.json) contains
two fresh complete constructions and every full/recovered state on all 17 saved
frequencies for both unit-port drives and the opposing drive. The nine training
frequencies and eight disjoint holdouts retain the original binary64 values.

Both runs select current orders **2 then 4**, reducing 8 physical coordinates to
4. At order 2 the maximum internal-state training error is 0.014527781973343685
(1.45278%), while port and loss errors are 4.395e-16 and 4.834e-12. Thus a port
or total-loss-only acceptance would miss a weak internal response. The fixed
1e-6 gate refuses that order. At order 4 the Krylov space spans precisely the
four nonzero physical-coordinate rows. Independence and exact zero support
permit its exact coordinate basis; no physical mode cutoff is used. Full and
recovered states, ports and individual losses then match bit for bit here.

The [independent audit](../../../debugging/p7-final-integration/checked/audit.json) keeps
the accepted frozen analytic tensor/box coefficients and independently assembles
the opposing-drive descriptor in 80 decimal digits. Maximum physical error on
all original 17 points is 8.115e-11, below the inherited 1% reference gate. This
reference does not independently establish general foil unit-drive continuum
accuracy; those drives are checked against the full discrete model. The
synthetic three-relaxation control separately checks all of its branch responses
against closed-form parallel RL/C arithmetic. Six [saved-value corruptions](../../../debugging/p7-final-integration/checked/negative-audits.json)
are refused: missing point, false gauge, source mutation, damaged recovered
current, damaged basis and false via admission.

| Fresh construction | Complete extraction/reduction/response path | Automatic reduction |
|---|---:|---:|
| cold | 0.0885246 s | 0.0018633 s |
| same-process repeat, rebuilt geometry/field/model | 0.0949010 s | 0.0022622 s |

Supervised executable wall is **0.4670545 s**, observed peak Windows working set
**6,447,104 bytes**. Field/current plans are 74,581 / 436,053 bytes; automatic
admission separately includes simultaneous full/reduced solves and saved
responses. Function-local admission does not account for arbitrary external
retained field objects; a real extractor would need shared remaining-budget
accounting before integrating it. The bounded executable is additionally
supervised for its whole actual process RSS. Plans are not OS RSS. The diagnostic
ran while the normal addon was compiling; no isolated throughput claim is made.
No real-board cold/warm result, cache gain or
speedup is inferred. [run.json](../../../debugging/p7-final-integration/checked/run.json)
records command, source/binary/output hashes and before/after equality of all
eight captured boards and historical P7/structural-gauge evidence.

The [spectrum/recovery figure](../../../debugging/p7-final-integration/checked/automatic-spectrum-recovery.png)
and [loss coverage figure](../../../debugging/p7-final-integration/checked/loss-band-gap.png)
were opened and visually inspected. Axis labels, units, all 17 samples and
overlays are readable. Exactly zero reduction errors use a **display-only
1e-18 floor** on the logarithmic plot. The coverage packet evaluates the existing
guards for sigma=58 MS/m, t=35um: volume skin screening reaches 396126.3 Hz but
the actual path is capped at 100000 Hz; surface-loss admission begins at
356513665.2 Hz. The red interval is unimplemented coverage, not a new model.
Interactive controls and real-board visualizations are not claimed.

Development refusals were retained rather than relaxing the gate. Before the
exact coordinate-basis representation was used for the already complete
four-row support, rotated four-current coordinates reached 1.90748e-6 weak-state
error and correctly refused the fixed 1e-6 gate. This was representation error
in the bounded reduction comparison, not a reopening of structural-gauge
physics or a reason to change C, the source or any production guard.

A final review also added explicit rejection of nonfinite/overflowing reduction
observables before maximum-error folding: infinity-minus-infinity must never
turn into a zero comparison error. The separate range test includes NaN, infinite
losses, overflowing complex magnitudes and subtraction overflow. This strengthens
the new reduction admission only. The final packet/build is regenerated from
that source under `debugging/p7-final-integration/final/`; the earlier successful
finite-observable packet is retained separately at the parent directory.

The final contour-allocation review reserves the borrowed input, retained metal
metadata and both ring buffers that can coexist while geo closes an unclosed
outer/void contour. This is admission accounting, not a larger cap. A valid
16,384-vertex void under a 1,000,000-byte request must refuse before allocating
prepared polygons. The checked packet and final shared-addon build include this
correction under `debugging/p7-final-integration/checked/`. The intervening addon
rebuild was deliberately interrupted to incorporate it; that log is preserved.

## Whole-P7 closure matrix

| Original obligation / final task item | Positive evidence and supported class | Negative/boundary evidence | Status |
|---|---|---|---|
| P7 RLGC(f), materials, layers and returns | Previously accepted P7a/P7b four uniform section classes | Prior geometry/weak-loss/frequency refusals retained | Partial; complete real paths absent |
| Actual foil grid with holes and multiple copper sources (A) | New constrained-hole field ownership and four-source finite-current assembly; analytic DC/J/normal-field energy | Invalid/missing void constraints, conflicting/untraced contacts, zero-area contact refusal | Bounded positive; native real-ROI mesh adapter absent |
| Source/net/layer provenance, CAD fills and original copper | Prior shared Board coordinate/context packet; new source identities retained at contacts | Eight saved boards remain immutable; count-only meshes not promoted | Real board extraction mapping open |
| Finite ordinary barrel, span, pad/antipad and return (A) | Prior DC barrel/conduction behavior is preserved only | All-foil AC assembly still withholds barrel admission | Open; zero successful AC signal/return via controls |
| Joint foil/barrel current conservation and no duplicated volume/R (A/B) | Finite foil source-piece reaction/DC/energy conservation | No substitute ideal via or scalar appended L/C | Open for barrels |
| Physical positive/return electrodes (A) | Prior P6 physical maps and new explicit finite foil maps | Different names, mismatched traces and zero-area contact do not authorize a weld | Partial; board pad/layer electrode adapter absent |
| Real ROI cuts and retained neighbouring/floating copper (A) | Prior native union/scope packet unchanged | Original ROI count refusals preserved | Open; no new physical ROI solve |
| Joint self/mutual R/A/L/C (B) | Accepted finite-foil kernels and new source-piece assembly use raw physical forms | Full P6 PSD/gauge/original-equation guards retained | Partial; barrel field/current extension absent |
| Scalable assembly without 64-support/potential dependence (B) | No new claim | Existing field/current limits and full dense P6 precompile remain | Open; no dense cap increase |
| Automatic passive basis/order (B) | New current moment blocks, congruence and response/order admission | Order cap, exhausted space, unknown mutual R and resources refuse | Partial; current-only and full bounded compile first |
| Port, charge/gauge, initial state, internal currents/loss recovery (B/C) | New all-drive training/holdout and analytic parallel-RLC controls; prior structural/transient controls retained | Unrepresented initial current is rejected | Bounded positive; general PCB continuum recovery open |
| Complete trace/pair selection and inhomogeneous channel (C) | Prior uniform-line source-bound examples remain | No uniform-section substitute for a heterogeneous route | Open |
| Confirmed losses and the intermediate frequency region (C) | Prior thin-volume and surface-loss scopes unchanged | Guard-derived 35um coverage gap recorded separately | Open; no arbitrary interpolation |
| Distributed finite multiport PDN with actual vias (C) | Supplied P6 descriptors remain supported | A bounded foil coupon is not advertised as a PCB PDN | Open; zero admitted board PDN multiports |
| Causal frequency/time inputs (C) | Mathematical passive congruence retains a descriptor; synthetic independent energy/control checks | Finite-band samples are not advertised as a temporal model | Partial; extracted-board time validation absent |
| Direct agreed board extraction DSL, schema and worker (D) | Existing model/network builders can execute the new bounded descriptor through P6 | Intentional false P7 check must fail while execution continues | Board extraction operation absent; interchange only |
| Three successful STM/TPS/Band physical ROI scenarios (D) | None added | Original counts/early refusals remain | Open; 0/3 successes |
| Strip/pair/different-layer/via/PDN independent coupons (D) | Prior P7a/P7b and bounded foil references retained; new analytic current reduction and source-piece controls | No continuum/via accuracy inferred from convergence or algebra alone | Via/return/PDN, asymmetric/different-layer pairs and general spatial-trace controls open |
| Separate mesh, quadrature, copper, air/domain, frequency convergence | Prior accepted isolated controls retained | New ownership/reduction tests do not replace real-ROI refinements | Open for requested board/via class |
| Negative physical inputs through DSL/worker | Existing accepted resource/port/validity guards retained | New internal negatives tested explicitly; worker only model interchange | Missing board-extractor negatives remain open |
| CPU/F64, ABI, bounded resources, regressions and evidence | New focused tests and packet listed below | Four historical ignored tests remain ignored; corpus filters explicit | Limited implementation verification only |

## Concrete remaining blockers and P8/P11 readiness

The accepted structural-gauge repair is not a blocker and has not been reopened.
The missing integration is physical and architectural:

1. The field/current representation has no implemented axial barrel potential
   trace and complete foil/barrel junction. A DC barrel conductance cannot serve
   as its self/mutual magnetic/electric model. Actual padstack/antipad/return
   mapping and independent annular-via current/field validation are still absent.
   Moreover, the field material contract currently assigns epsilon by z alone;
   an air bore inside a dielectric layer needs XY-dependent material ownership.
   Simply declaring a hole in the metal would fill that bore with the layer's
   dielectric, so it cannot establish the requested actual via field model.
2. The original retained STM/TPS/Band ROIs have 726/1319/1804 potential and
   1902/4110/5584 candidate current coordinates. Their existing P6 envelope alone
   is 8.23/35.14/65.07 GiB. New current-only congruence still retains all those
   potentials and compiles the full model before projection. It cannot make these
   ROIs fit 512 MiB or bypass the field's 64-potential restriction. An implemented
   sparse/operator field/current assembly and potential reduction before P6 is
   still required; changing caps or handing it a smaller manufactured model
   would not resolve this.
3. The existing volume and surface-loss admissions have no overlapping frequency
   region for ordinary 35um copper. The new coverage packet evaluates their
   actual guards; no diffusion/proximity-loss method filling that gap has been
   implemented or independently validated. Source-code integration cannot
   justify interpolation between the two asymptotic models.
4. The existing DSL can execute supplied descriptors but has no implemented
   extraction operation accepting these physical board/ROI inputs. The new
   worker interchange intentionally remains a bounded coupon verification.

These are outstanding implementation/method-validation obligations, not a new
request for authorization or a claim that the whole assignment was completed.
There are no new successful real-board extraction reports to review.

**P8:** no admitted extracted PCB PDN input for AC/transient is delivered.
Previously accepted authored P6 models remain usable only within their original
classes. **P11:** previously accepted uniform P7a/P7b lines remain available;
there is no newly admitted real channel with a signal/return transition or
validated time input. No P8/P11 implementation was started. **P7 is not closed.**

## Checks, compatibility and changed common files

| Check | Actual result |
|---|---|
| Broad optimized P6/current/field/magnetic regression batch | 93 passed, 4 existing ignored; [regressions.log](../../../debugging/p7-final-integration/regressions.log) |
| Geometry/conduction/DC/P7a/P7b and contact/current controls | 79 passed, 4 existing ignored, 2 explicit corpus filters; [final-geometry-regressions.log](../../../debugging/p7-final-integration/final-geometry-regressions.log) |
| Final reduction nonfinite/overflow control | 1 passed, 24 unrelated unit tests filtered; [final-range-test.log](../../../debugging/p7-final-integration/final-range-test.log) |
| Reduction/current regression after range guard | 7 passed; [final-current-tests.log](../../../debugging/p7-final-integration/final-current-tests.log) |
| Final contour/contact/current/field controls | 15 passed, 4 existing ignored; [final-contour-tests.log](../../../debugging/p7-final-integration/final-contour-tests.log) |
| Fresh checked native example | Complete under whole-process 120 s / 512 MiB; independent audit passes all 17 saved opposing-drive points and all-drive full/reduced checks |
| Negative saved-data audit | 6 corruptions refused |
| TypeScript typecheck/package build | Both exit 0; [typecheck.log](../../../debugging/p7-final-integration/typecheck.log), [package-build.log](../../../debugging/p7-final-integration/package-build.log) |
| Normal shared addon from final sources | Exit 0, 8m23s, existing placement/link warnings; [checked-addon-build.log](../../../debugging/p7-final-integration/checked-addon-build.log) |
| DSL regression suite | 30 passed, 0 skipped; [dsl-tests.log](../../../debugging/p7-final-integration/dsl-tests.log) |
| Direct compiled-worker/shared-addon interchange | Complete in 406.833 ms, two full 17-point original-axis graphs, 3 checks pass and 1 intentional P7-completion check fails; execution continues; [result](../../../debugging/p7-final-integration/checked/dsl-result.json), [scenario](../../../debugging/p7-final-integration/checked/scenario.js) |

The four existing ignored field requests are `frozen_accepted_d64_domain`,
`frozen_accepted_d64_mesh`, `frozen_failed_d32` and
`native_cdt_independent_accuracy`. No unchanged expensive field reference was
recomputed. `saved_boards_conduction_corpus` and
`separated_object_growth_candidates_and_corpus` were explicitly filtered; they
are not reported as passes. Later tests repeat only code affected by the late
range/allocation changes. No expensive debug corpus was executed.

All Cargo commands use `CARGO_BUILD_JOBS=1`. The broad batch names are recorded
in its log; final focused commands are:

```powershell
cargo test --release --manifest-path native/analyze/Cargo.toml --lib automatic_range_tests
cargo test --release --manifest-path native/analyze/Cargo.toml --test automatic_reduction --test finite_contacts --test current_space --test layered_capacitance
cargo build --release --manifest-path native/analyze/Cargo.toml --example automatic_reduction
python tests/fixtures/analysis-local-extraction/final-integration/run.py debugging/p7-final-integration/checked
python tests/fixtures/analysis-local-extraction/final-integration/audit.py debugging/p7-final-integration/checked/automatic.json debugging/p7-final-integration/checked
python tests/fixtures/analysis-local-extraction/final-integration/test-audit.py debugging/p7-final-integration/checked
python tests/fixtures/analysis-local-extraction/final-integration/plot.py debugging/p7-final-integration/checked
npm run native:build
npm run typecheck
npm run build
node --import tsx --test --test-concurrency=1 tests/analysis-dsl.test.ts
node --import tsx tests/fixtures/analysis-local-extraction/final-integration/dsl.ts debugging/p7-final-integration/checked
python tests/fixtures/analysis-local-extraction/final-integration/manifest.py
```

Changed common production sources:

- [reduction.rs](../../../native/analyze/src/extraction/local/reduction.rs): bounded automatic moment blocks, order/observables admission, exact coordinate representation of a complete coordinate subspace, nonfinite guards.
- [layered_capacitance.rs](../../../native/analyze/src/extraction/local/layered_capacitance.rs): optional void contours, full contour-copy allocation admission, finite shared-source surfaces and source-preserving traces.
- [current_space.rs](../../../native/analyze/src/extraction/local/current_space.rs): exact hole-aware coverage validation.

The internal Rust `Metal` constructor gains `holes_m`; its two pre-existing Rust
fixture initializers supply an empty vector. Serialized old inputs remain
compatible because the property defaults empty and is omitted when empty; the
independent audit proves equality of prior physical inputs and raw R/L/C/A/B.
No TypeScript DTO/schema/worker implementation or addon ABI is changed. The new
TS file is a bounded interchange test only. Captured boards, placement F32/GPU,
Thermal and historical evidence remain unchanged. Parent-authored task/changelog/
plan text is preserved; only the new partial-result entry was added. No commits,
subagents, P8/P11 implementation or new assignments.

[Final evidence manifest](../../../debugging/p7-final-integration/evidence-manifest.json)
binds the checked source files, native example/shared addon, compiled JavaScript,
documents and artifacts. Before writing it, the manifest tool verifies every
checked-run source, captured-board and preserved-history hash against current
files. The checked spectrum PNG is byte-identical to the visually inspected
final-generation image. Whole-P7 closure and P8/P11 dependency acceptance are
explicitly withheld. STOP for parent review.


## Parent review — 2026-10-25

Acceptable as a bounded partial implementation, not completion of the final-integration assignment. Reviewed automatic current moment/order selection, holdout separation, physical observable recovery, finite source-contact ownership, void coverage and allocation guards. Parent verified 41 source files, 8 captured boards, 85 preserved historical files, 2 binaries, 26 compiled JavaScript files, 4 documents and 60 artifacts: zero hash mismatches before review annotations.

Parent reran the 15 focused automatic/contact/current/field tests (4 existing ignored), normal addon build and independent audit of both the saved packet and a fresh matching native-example run. All 17 opposing-drive independent comparisons and all-drive full/recovered checks passed. The automatic spectrum/recovery visualization was inspected. Evidence is under debugging/p7-final-integration/parent-*. Historical packets were not overwritten.

The final assignment remains unfulfilled: no real AC via/return, scalable ROI assembly/potential reduction, validated intermediate loss coverage, complete path/PDN extraction or successful real-board DSL scenarios were delivered (0/3). These are remaining implementation obligations, not external blockers or newly required user authorization. P7 is open; extracted-board P8 and arbitrary-channel P11 are not admitted. The bounded improvements are accepted independently; no next task is issued by this review.

Additional parent checks passed: 26 coupled/physical-terminal/structural-gauge regressions and the nonfinite reduction range unit test (24 unrelated unit tests filtered). Parent total: 42 passed, 4 existing ignored. No new implementation assignments were issued.
