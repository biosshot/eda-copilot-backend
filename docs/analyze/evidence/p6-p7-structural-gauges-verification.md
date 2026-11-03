# P6/P7 structural gauges - verification

Base `7a1da96`, branch `feat/analyze`, CPU/F64. All 17 original saved frequencies pass on full/reduced loop and physical longitudinal two-port models. The original cross-foil source also passes all 17 AC points. No geometry, material, physical C/R/L/A/B, source, frequency or production residual/KCL/passivity/power/energy guard was substituted. Whole P7 remains open.

## Design and validation

The design was recorded before numerical experiments: additive internal `structuralGaugeModes` is separate from conserved physical-charge candidates. P7 derives a uniform shift from its connected insulated product field domain and partition-of-unity surface traces; InfinityBox supplies no certificate. It does not infer nullity from small eigenvalues. Old supplied models without the field remain compatible and retain their physical charge path.

P6 checks dimensions, finiteness and independence, normalizing each column by its maximum magnitude before reorthogonalization to avoid scaling underflow/overflow. For a normalized structural column q, every C*q and A^T*q row satisfies

```
abs(residual_i) <= 64 * eps_F64 * (nv+1) * sum_j abs(M_ij * q_j)
```

This dimensionless dot-product backward-error criterion was fixed before numerical experiments. The factor 64 accommodates F64 assembly/normalization and dot-product accumulation; nv<=1024 bounds the reduction length. It is zero when all terms are zero and scales with matrix units and basis scale. It is a consistency gate, not a continuum-accuracy bound or an absolute capacitance cutoff. Source compatibility applies the same bound to real and imaginary RHS separately. No source projection occurs. Invalid dimensions/NaN/zero/dependence, incompatible C/A and impossible excitation have specific failure checks.

The exact structural assertion comes from the field formulation. Rounded matrices alone cannot prove absence of a sub-roundoff physical exterior effect. A producer adding exterior C must withhold/recompute the certificate. No small eigenvalue is promoted to a certified gauge. Tested real exterior coefficients remain physical, including values twelve orders below internal C.

P6 uses existing bordered constraints q^T*v=0 and removes only the certified subspace from the conserved charge metric before its eigendecomposition. Original raw C is retained for storage, diagnostics and original-equation checks. There is no operator clipping/projection, artificial ground, leakage or gmin. The inherited coefficient-space rank-resolution check concerns basis dependence, not storage classification. Transient initialization diagonalizes storage on the certified complement and appends gauge coordinates as exact zeros in that rank calculation; raw charge/flux/energy and increment checks remain active.

A dormant port can couple structural components. Therefore actual B*u is checked at solve/initialization/advance after physical assembly. Adding a physical RC element splits the structural space into its annihilator plus the direction the element charges/anchors; that direction rejoins the physical candidate space. Known added capacitance is refused before any mutation if F64 accumulation loses more than the inherited 1% physical gate, or its nonzero scaled coefficients underflow. It is never silently erased as gauge.

Composition separately intersects embedded structural and conserved spaces with physical weld/ground constraints, then P6 revalidates the assembled model. Grounding or exterior C can remove gauges. Current-only reduction leaves potential coordinates unchanged, clones the certificate and revalidates full/reduced models through existing System::compile calls.

## Independent response and all 17 points

[Raw packet](../../../debugging/p6-p7-structural-gauges/coupon.json), [supervised run/hashes](../../../debugging/p6-p7-structural-gauges/run.json), [audit](../../../debugging/p6-p7-structural-gauges/audit.json). Inputs, foils, traces and full C/R/L/A/B are checked against immutable parent-coupon.json. Both fresh construction runs give identical responses. Exact original binary64 frequencies are frozen in fixture.rs and checked against history; table display rounding changes no calculation.

The handwritten independent six-state complex descriptor uses an explicit common gauge row and frozen independent analytic tensor C / box L coefficients. It imports no production solver and reruns no expensive field/box references. Final reference arithmetic is 80-digit mpmath, with exact imported binary64 coefficients and high-precision omega. Every complex port entry, physical potential/current, local/total loss and full/reduced recovery retains the unchanged 1% gate. Error normalization is per entry: abs(actual-reference)/max(abs(reference),1e-12*max(abs(reference observable))). Zero/weak entries retain a declared absolute scale.

The table contains relative errors (threshold 1e-2). Full Z covers longitudinal, loop and original cross-foil; reduced Z covers longitudinal and loop. Current is the maximum independent full/recovered current error.

|Frequency Hz|Full/reduced|Independent full Z|Independent reduced Z|Physical current|Local losses|
|---:|:---:|---:|---:|---:|---:|
|1000|PASS/PASS|8.207e-13|8.207e-13|4.148e-13|3.081e-15|
|1333.52143216|PASS/PASS|1.094e-12|1.094e-12|5.531e-13|3.742e-15|
|1778.27941004|PASS/PASS|1.459e-12|1.459e-12|7.375e-13|4.182e-15|
|2371.37370566|PASS/PASS|1.946e-12|1.946e-12|9.835e-13|4.842e-15|
|3162.27766017|PASS/PASS|2.595e-12|2.595e-12|1.312e-12|7.923e-15|
|4216.96503429|PASS/PASS|3.461e-12|3.461e-12|1.749e-12|1.144e-14|
|5623.4132519|PASS/PASS|4.615e-12|4.615e-12|2.332e-12|1.849e-14|
|7498.94209332|PASS/PASS|6.154e-12|6.154e-12|3.110e-12|2.971e-14|
|10000|PASS/PASS|8.206e-12|8.206e-12|4.147e-12|5.062e-14|
|13335.2143216|PASS/PASS|1.094e-11|1.094e-11|5.531e-12|8.848e-14|
|17782.7941004|PASS/PASS|1.459e-11|1.459e-11|7.375e-12|1.554e-13|
|23713.7370566|PASS/PASS|1.945e-11|1.945e-11|9.834e-12|2.738e-13|
|31622.7766017|PASS/PASS|2.592e-11|2.592e-11|1.311e-11|4.849e-13|
|42169.6503429|PASS/PASS|3.454e-11|3.454e-11|1.748e-11|8.597e-13|
|56234.132519|PASS/PASS|4.598e-11|4.598e-11|2.331e-11|1.526e-12|
|74989.4209332|PASS/PASS|6.115e-11|6.115e-11|3.107e-11|2.709e-12|
|100000|PASS/PASS|8.115e-11|8.115e-11|4.141e-11|4.808e-12|

Maximum independent physical-potential error is 8.115e-11, full/projected potential recovery 3.521e-15 and total-loss error 1.981e-15. Full longitudinal relative residual is 1.949e-16, KCL 6.642e-17 A and real-power error 1.101e-16. These matches validate this bounded coupon and stated excitations, not arbitrary full-current-space continuum accuracy.

Cross-foil injection is admissible at nonzero frequency: it transfers charge between foils while conserving their total. Its large reactive Z (about -j7.19e10 Ohm at 1 kHz) is physical and independently expected; it differs from the historical unconstrained reference explosion. This same source has no DC steady state and is specifically refused with COUPLED_DC_INCOMPATIBLE. A source along a surviving structural zero-energy mode fails explicitly with COUPLED_GAUGE_INCOMPATIBLE.

## Physical, transient and negative controls

Nine structural tests pass: node permutation, certificate scaling 1e-200..1e100, rotated bases, two independent floating components, different initial references, invalid/dependent/NaN certificates, false C/A compatibility and incompatible excitation. Connected composition tests join floating models, attach a model with physical exterior C and weld to an authored ground. The existing disconnected-topology guard is preserved. Port RC assembly changes gauge/physical spaces correctly; tested 1e-6 and 1e-18 F additions retain physical initial island charge. Unresolved 1e-24 F accumulation and scaled-coefficient underflow are explicit failures before mutation.

Standalone exterior C per conductor of 1e-6, 1e-12 and 1e-18 F retains the common 3 V seed. Independent common energies are 9e-6, 9e-12 and 9e-18 J; native values are 9e-6, 8.999999999613159e-12 and 8.998030998682433e-18 J (worst error 0.02188%, within 1%). Frequency responses match independent admittance 1/100+j*omega*(1e-6+Cext/2). False structural certificates on these exterior models are rejected with C incompatibility.

Independent DC loop R is 0.9852216749 mOhm, density +/-57.14285714 MA/m^2, and four local losses total 0.9852216749 mW at the preserved opposing 1 A drive. Existing congruence/energy/recovery and representable/unrepresentable initial-current tests remain active.

Two independent transient controls pass: RC exponential decay preserves differential charge without common drift; RLC two-root decay preserves nonzero initial inductor current 0.01 A and initial energy 2.05e-6 J. Voltage, final current and energy bounds against analytic solutions are 5e-4 V, 5e-6 A and 1e-9 J. Initial energy and charge checks are tighter. Original finite-front/rank/original-equation checks and both energy guards remain unchanged.

Six saved-value corruption cases (missing point, changed C, absent certificate, false admission, damaged current/recovery) are refused: [diagnostics](../../../debugging/p6-p7-structural-gauges/audit-negative-tests.json).

## Reference arithmetic and preserved failures

The first independent NumPy/F64 descriptor solve contaminated a weak physical charge coordinate, giving 2.1288% potential error at 2371.373705661655 Hz despite accurate port differences. [The refusal remains](../../../debugging/p6-p7-structural-gauges/audit.log). Evaluating the same handwritten equations, source and frozen independent coefficients in 80 decimal digits resolves it. Production tolerances and normalization were not loosened. Final audit.json records its own source/input/reference hashes. Native run.json retains the earlier audit-source hash recorded before that reference correction; [the final manifest](../../../debugging/p6-p7-structural-gauges/evidence-manifest.json) binds final audit/DSL scripts, compiled workers and addon separately.

Development failures remain in first-tests.log, regressions.log and checked-regressions.log: stale expected AC refusal, invalid test topology wiring, and the capacitance-underflow negative that exposed a missing refusal. Corrected wiring retains original topology guards; final production now rejects the underflow. No failed frequency was deleted or interpolated.

## Performance and resources

Executable wall 0.438760 s; observed peak Windows working set 5,959,680 bytes. Supervised numerics remain below 120 s / 512 MiB; compilation is separate. Current simultaneous plan 434,561 bytes; full/reduced projection plan 288,512 bytes. Heap plans and observed OS memory are distinct.

|Fresh construction|Complete path s|Full loop 17 points ms|Reduced loop ms|Full physical two-port ms|Reduced physical two-port ms|
|---|---:|---:|---:|---:|---:|
|cold|0.0976535|1.1438|0.4178|1.8404|0.7024|
|same-process-repeat|0.0974044|0.9440|0.5993|1.7194|0.8617|

Both runs rebuild field, currents, model and projection. No cache/ROI acceleration is claimed. All eight board hashes and every historical p7-completion file hash remain unchanged. User native/analyze/debugging/ was not used or modified.

## Checks and integration

Final optimized Rust: 57 passed, 4 existing ignored, zero failures (final-checked-regressions.log). Targets: structural_gauges, current_space, coupled, physical_terminals, mixed, delayed, relaxation and layered_capacitance. No expensive debug corpus was run. Typecheck/build and the 30-test DSL suite pass. Normal shared-addon build completed with existing placement/link warnings (addon-build.log). Actual compiled-worker/shared-addon direct DSL completes in 411.117 ms; [result](../../../debugging/p6-p7-structural-gauges/dsl-result.json), [log](../../../debugging/p6-p7-structural-gauges/final-dsl.log). Both full/reduced physical longitudinal models return 17-point graph series on the exact saved axis. Three checks pass and the intentional full-P7-completion check fails; execution continues and emits its final log. Graph series are present; interactive browser rendering remains unclaimed.

Common production files changed: electrical/coupled/mod.rs (Model/System), coupled/time.rs (initialization), coupled/composition.rs (separate intersections), electrical/relaxation.rs and resonant.rs (empty-field initializers), extraction/local/layered_capacitance.rs and current_space.rs (field provenance/transfer), src/analyze/analysis-dsl/frequency-model.ts and spec-doc.d.ts (additive model-data property). Adapter/executor/worker implementations need no change: existing JSON transport preserves it. No new user builder/manual board gauge is introduced. Reduction source stays unchanged because existing cloning and compile checks already preserve/revalidate the field.

Commands, CARGO_BUILD_JOBS=1:

```powershell
cargo test --release --manifest-path native/analyze/Cargo.toml --test structural_gauges --test current_space --test coupled --test physical_terminals --test mixed --test delayed --test relaxation --test layered_capacitance --no-fail-fast
cargo build --release --manifest-path native/analyze/Cargo.toml --example current_space
python tests/fixtures/analysis-local-extraction/current-space/run.py
python tests/fixtures/analysis-local-extraction/structural-gauges/audit.py
python tests/fixtures/analysis-local-extraction/structural-gauges/test-audit.py
python tests/fixtures/analysis-local-extraction/structural-gauges/plot.py
npm run native:build
npm run typecheck
npm run build
node --import tsx --test --test-concurrency=1 tests/analysis-dsl.test.ts
node --import tsx tests/fixtures/analysis-local-extraction/current-space/dsl.ts
```

Four PNGs were opened and visually inspected: [spectrum/reference/error](../../../debugging/p6-p7-structural-gauges/spectrum-reference-error.png), [full/projected](../../../debugging/p6-p7-structural-gauges/full-projected-recovery.png), [currents/losses](../../../debugging/p6-p7-structural-gauges/physical-currents-losses.png), [real exterior C](../../../debugging/p6-p7-structural-gauges/real-exterior-capacitance.png). All 17 samples, overlays, units and labels are readable. Interactive controls are not claimed.

The bounded structural-reference interface is repaired. Whole P7 stays open: foil/barrel junctions, actual vias/returns, real ROI scaling and independent general full-current-space/continuum accuracy remain outside scope. PCB PI/SI and P8/P11 gates are unchanged. Placement F32/GPU and Thermal sources are untouched. No commit or new task. STOP for parent review.


## Parent acceptance — 2026-10-25

Accepted the bounded P6/P7 structural-gauge repair. Reviewed certificate validation, charge-space separation, physical port additions, composition, initialization and field provenance. Independently verified 25 source, 4 binary, 26 compiled-JS and 35 artifact hashes before review annotations: zero mismatches. Viewed spectrum-reference-error.png.

Parent reran the eight optimized targets: 57 passed, 4 existing ignored; DSL suite 30/30 passed. Normal native addon build passed. Independently audited both the saved packet and a fresh execution of the matching native example: all original 17 frequencies pass for full/reduced loop and longitudinal models; original cross-foil AC also passes. Fresh outputs: debugging/p6-p7-structural-gauges/parent-coupon.json and parent-fresh-audit/. Historical packets remain unchanged. Typecheck/package-build and direct-worker scenario evidence were source/binary/hash-verified; no full-board corpus was replayed.

This closes the diagnosed structural-null execution blocker for the tested class. It does not close P7: physical foil/barrel junctions and actual return vias, real-board extraction/mapping, scalable operators and automatic compact reduction, wider frequency-loss applicability, independent continuum/full-current-space validation and successful real-board DSL reports remain. P8 extracted-board PDN and arbitrary-channel P11 readiness are not granted. No next tasks issued.
