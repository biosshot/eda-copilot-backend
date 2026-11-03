# P7 completion verification — 2026-10-24

Accepted base: `a9d5c5c`, branch `feat/analyze`. **Partial implementation;
verified contract blocker. P7 is not closed; PCB P8/P11 remain unready.**

## Preflight (written before new numerical experiments)

The accepted magnetic operator is bounded to 64 prism supports/16 current
modes; the accepted electrostatic kernel prescribes equipotential metal
bodies. A foil P1 current mesh instead needs a *spatial potential trace* on
each finite metal surface. Identifying all its nodes with a metal potential
would destroy longitudinal voltage gradients. The first implementation step
therefore extends the existing electrostatic kernel with explicit conservative
surface traces, reusing its assembly, factorization and field recovery.

| Observable | Independent reference | Error normalization / gate | Refinement | Resource cap |
|---|---|---|---|---|
| Generalized nodal C, all entries | Exact P1 triangle mass/stiffness tensor product on a rectangular insulated gap | max absolute entry error / max reference diagonal <= 1e-10; zero terms use this absolute scale | two planar triangulations; added dielectric plane separately | 120 s / 512 MiB per batch |
| Constant-trace C | Existing analytic layered series capacitance and unchanged equipotential kernel | full entry error / max reference diagonal <= 1e-10 | same geometry, independent trace representation | same |
| Trace KCL/work/field recovery | Integrated dielectric field energy and boundary reactions | existing 1e-9 guards, unchanged | nonconstant, common and zero voltage vectors | same |
| Sheet DC port R / local Joule power | sigma*t triangle stiffness and rectangular rho*l/(w*t) | 1e-10 scaled absolute; zero uses maximum nonzero self scale | shared conforming triangles / electrode quotient | same |
| Finite-volume L | Frozen independent square-box convolution and smooth box mutual references | inherited 0.2% quadrature/self and 5% full physical energy gates; all signed mutual diagnostics retained | quadrature separately from mesh | same |
| Full / projected model | Full descriptor response in existing P6 solver; independent analytic symmetric modes | <=1% complex port response; <=1% physical current/loss norm, exact invariant modes <=1e-9; zero uses nonzero excitation scale | projection order and holdout frequencies separately | same |
| Passive congruence | Full/recovered electric/magnetic energy and R loss | 1e-10 scaled absolute; PSD uses existing P6 validator | independent nonzero and zero states | same |

These exact polynomial patch tolerances test implementation consistency, not
accuracy of arbitrary PCB fields. Existing finite-domain 5%/2% accuracy gates
are unchanged. No PCB success will be reported without compatible traces,
foil/barrel junctions, independent physical references, and copper/domain/mesh
controls. No artificial capacitance is attached to a DC graph.

2026-10-24 preflight addendum, before the new coupon/reference run: the finite
two-foil coupon uses 0.5 mm squares, 35 um metal, a 1 mm vacuum gap, and
explicit insulated exterior walls. An independent smooth **vertical box mutual**
reference uses the difference-convolution integral with weight
`(l-|dx|)(w-|dy|)(t-|dz|)` and separation `h+t+dz`, product Gauss orders16/32.
The last observed change must be <=1e-6; no singular native kernel is imported.
Self L reuses the frozen independent 512-order box convolution result unchanged.
Independent C uses rectangle linear-x mass `lw/6 [[2,1],[1,2]]` and stiffness
`w/l [[1,-1],[-1,1]]`, with analytic z blocks h/3,h/6,+/-1/h. A handwritten
six-state complex descriptor plus its single common gauge supplies the independent
full two-port and opposing-port current/loss reference at17 log-spaced frequencies
1..100 kHz. Gates remain1% physical current/loss/complex response; this validates
the bounded discretized coupon, not finite-air fringing or PCB accuracy.


## First native checkpoint — retained refusal

The first release run passed the trace/tensor/real-ROI refusal tests and the
DC/current/Joule/self-L assertions, then refused the cross-foil unit-port AC
calculation with `COUPLED_POWER`. The low capacitance makes that source store
large common conductor energy; its dissipative response is unresolved by the
existing P6 power gate. No P6 tolerance, C, physical source or return was repaired.
`debugging/p7-completion/failed-cross-foil-ports.json` preserves this case.
A separate explicit longitudinal port pair on the two foils is now tested;
opposing port currents give the same closed-loop RHS. This is a distinct
supported excitation, not acceptance of the original charge-injecting ports.
The original remains a negative regression and an additional P6/P7 limitation.


The separate longitudinal unit-port test also exposed the same gauge issue at
10 kHz (potentials ~5.77e44 V); its unchanged real-power gate refused it. Merely
changing two-contact ports does not resolve the structural-null limitation.
The remaining positive experiment uses the explicitly prescribed neutral
four-electrode loop mode `B=[1,-1,-1,1]^T`, already representable by general P6
incidence. It has no two-contact `portTerminals` map and cannot be advertised
as a composable physical via/PDN port. All physical quantities and rejection
checks remain; no frequency point or source is silently discarded.

## Implemented boundary

- `local/layered_capacitance.rs`: `extract_traced` reuses accepted geometry,
  copper exclusion, sparse factorization and field/charge/work recovery.
  Exact-coordinate, source-bound partition-of-unity traces connect finite metal
  surfaces to current potentials. Missing/duplicate/unmatched nodes, unknown
  sources and unused coordinates refuse. Legacy `extract` stays equipotential.
  The new diagnostic trace cap is64 potential coordinates; the16-metal limit
  and explicit field budgets are unchanged. `energy_capacitance_f` evaluates
  `basis^T K basis` and checks against retained raw reaction C, without averaging
  or repairing C.
- `local/current_space.rs`: conforming foil P1 potentials, accepted two current
  bases per triangle, finite electrode quotient, complete source/surface/coverage
  and interpolation checks, R/A assembly, finite-prism self/mutual L, current
  density/Joule recovery and existing P6 descriptor validation. Both directed
  integrals and12/8 versus24/16 refinement remain visible. Gates are0.2%
  refinement and1e-9 raw reciprocity normalized to largest physical self entry.
  `band_hz` is necessary skin/geometric screening, **not frequency admission**.
  Early64-support/64-potential admission precedes dense allocation. No barrel,
  board adapter, dielectric-loss model or public extraction operation exists.
- `local/reduction.rs`: supplied orthonormal current congruence `diag(I,Q)`
  retains full potential C, contacts and charge. It recovers currents and refuses
  unrepresented initial currents. The coupon reduces8 currents to2 (12 total
  states to6). DC, stored energy and R loss pass. This is **not automatic PRIMA,
  scalable pre-solve reduction or full-band recovery**.

No P6/DSL/schema/worker/addon ABI implementation, runtime dependency, placement
F32/GPU, Thermal, CAD export or captured board changed. Existing shared numeric
changes are confined to internal layered capacitance; local exports are additive.
No physical source/return/material was invented.

## Numerical outcome

Independent triangle/rectangle tensor references pass; four-coordinate C error
is2.1382e-16 relative to largest reference diagonal. Constant-trace projection
preserves old analytic series C; separate z refinement preserves uniform normal
field. These are exact discretization checks, not arbitrary spatial-trace/PCB
continuum accuracy. Copper-scope, nonconstant field mesh and finite-air domain
controls remain open. Magnetic refinement3.2669211e-7 and raw reciprocity
3.5544233e-10 pass unchanged gates. Self L reuses the frozen box convolution;
vertical mutual uses the predeclared independent smooth box16/32 control.
DC loop R is0.9852216749mOhm, J is+/-57.14285714MA/m²; four local losses total
0.9852216749mW for opposing1A. Congruence preserves DC/energy/loss with
zero/nonzero/tiny seeds and refuses an unrepresented initial loop mode.
The independent box references cover the uniform physical-current subspace;
they do not establish the inherited5% full quadratic accuracy gate for every
arbitrary coefficient in this new eight-current space. PSD and small integration
changes do not substitute for that still-open independent full-space check.

**Whole AC band fails:**8 of17 fixed1..100kHz points are refused, including
full/projected power, residual and singularity failures. Neither longitudinal
two-contact ports nor neutral four-electrode mode resolves the issue. Nine
diagnostic points have maximum independent loop-Z/current/local-loss errors
0.201861%/0.0035724%/0.0003132%, projected current0.0035690%. These points
establish no usable band. No reusable S/Z/Y or transient model is exported;
failed points are never deleted, interpolated over or replaced by zero.

The structural-null packet records full C and both accumulation orders.
Its common mode is analytically zero-energy, but native sequential `C*1`
leaves3.9443e-31F roundoff while BLAS returns zero. The rotated conserved-charge
metric obtains a tiny positive eigenvalue. P6 distinguishes exact zero energy
from physical charge; this representation can misclassify the mathematical
gauge. At10kHz one diagnostic produced common potentials~5.77e44V and failed
power admission. Projected residual/singularity failures remain too. Good DC
or passivity never overrides these guards.

## Real ROI and performance boundary

Reused accepted shared-context source/count evidence, without replaying
unchanged expensive references or geometry corpus:

| ROI | Foils | Axial supports | Candidate currents | Potentials | Existing P6 dense envelope alone |
|---|---:|---:|---:|---:|---:|
|STM32G0_CAN_RS485|903|192|1902|726|8.23GiB|
|TPS63060DSCR|1911|576|4110|1319|35.14GiB|
|BandAmpV5|2600|768|5584|1804|65.07GiB|

The last column applies existing `(nv+ni)^2*1280` admission without allocation.
Saved GEOS meshes remain counts-only, not valid current/contact spaces. Tests
send all three packets through new early refusal. These are **ROI refusal
regressions, not successful extraction**. No inadequate smaller ROI or ideal
via replaces the originals. Real ROI full-path cold/warm/RSS is absent.

Optimized coupon cold/repeat complete paths0.0784975/0.0764606s; cold field
0.0005146s, magnetic/current0.0755125s, projection0.0000508s. Supervised process
0.331509s; observed peak Windows working set5,935,104bytes. Simultaneous
field/current/P6 plan434,295bytes; full/projected plan288,512bytes. Heap plans
exclude runtime/image memory; OS RSS is separate. Both runs rebuild everything;
no cache or board acceleration is claimed. Numerical work stayed under
120s/512MiB; compilation is separate. All eight captured board hashes are
unchanged before/after in `run.json`.

## Whole-P7 matrix

| Original obligation | Accepted earlier | Implemented here | Not closed |
|---|---|---|---|
|Coupled line sections, RLGC(f), materials/layers/returns/losses|P7a/P7b four uniform sections, conditional weak losses|Prior physics preserved|Automatic real path/pair selection; finite returns/inhomogeneities and low/high-frequency loss transition|
|Local transitions/PEEC/BEM, pad/antipad/span/return/G3|Finite magnetic/layered-C coupons; copper context/classification|Spatial foil C/current compatibility and bounded matrices|Ordinary barrel traces/junction; independent signal/return via; real padstack/contact admission|
|Finite PDN, vias/mutuals/frequency losses/distribution|P6 supplied physical descriptors; P7b uniform line losses|Foil DC/current/loss; extracted candidate R/L/C; AC refusal|Any admitted PCB PDN multiport/band, losses, actual ROI extraction/refinement/performance|
|Passive compact models, port/internal recovery, fitting/interpolation|P6 supplied full-model execution|Current congruence, DC/energy recovery and initial-current checks|Automatic PRIMA/order/basis, reduction before large compile, full-band recovery, runtime recovery/continuation metadata|
|Strip/pair/different-layer/via/PDN coupons|Prior P7a/P7b/kernel references|Spatial-C tensor and two-foil DC/energy|Via/return/PDN, admitted RLC band and spatial-trace continuum field accuracy|
|Separate mesh/integration/copper/domain/frequency controls|Prior bounded kernel/field controls|Separate z patch, magnetic quadrature, fixed-axis refusals|Real current/field mesh, copper/domain and AC representation convergence|
|Three PCB direct DSL reports/visuals|Prior board-bound uniform sections|Three ROI refusal checks; direct DSL refusal audit; inspected PNGs|Successful STM/TPS/Band RLC extraction, real via/return and extraction DSL operation|
|Negatives/resources/regressions/ABI|Existing guards and worker/addon|Trace/current/projection negatives, saved-audit corruption cases, checks below|Board stackup/padstack/fill/return/path negatives await actual board extractor|

**P8:** existing authored P6 networks remain usable in accepted classes; no
admitted extracted PCB PDN input exists here, even for the complete coupon
screening band. Board PI remains blocked. **P11:** earlier P7a/P7b uniform-line
resources and P6 frequency/delay primitives remain usable; full channel,
transition/return-via, physical recovery and validated time inputs are absent.
No new PCB SI/TDR/eye readiness is granted. Neither analyzer was implemented;
the original P7 checkboxes stay open.

## Concrete architect decision

Safe P6 execution of extracted C needs a representation separating structurally
exact zero-energy gauges from real small common charge after F64 assembly and
basis rotation. A field-derived structural-null certificate/basis, with charge,
seed and physical-trace recovery, is a minimal contract direction. It must not
infer nullity from a loose eigenvalue threshold, erase exterior C or reinterpret
metal as ground. This contract was not silently added to P6. A physically SPD
infinity-box field is a different case and still needs independent spatial-trace
mesh/domain controls; it does not validate this refused insulated case.

Source-bound foil/barrel junctions and scalable operator/projection admission
also remain necessary. Supplied-basis projection cannot solve that scaling
problem alone. Caps were not raised, copper/modes not dropped, GPU/FMM and
replacement solvers not added. Stop for architect review of this partial packet;
no subsequent analyzer or next task is issued.


## Parent review — 2026-10-25

Accepted as an internal partial checkpoint only, not completion of P7 or a usable PCB AC model. Reviewed source-bound trace assembly, foil coverage/interpolation, congruence/recovery and the preserved failure boundaries. No change to P6 guards or public extraction admission is accepted here.

Parent executed 13 optimized regression targets: 116 passed, 4 existing ignored tests, 2 corpus filters; no failures. Rebuilt and reran current_space and layered_capacitance tests from current sources. Five saved-value audit corruption cases were rejected. Normal shared-addon build and direct DSL refusal/continue-after-check scenario passed. The AC refusal visualization was inspected.

Historical run.json matches 30 of 31 recorded source hashes and all eight board hashes; layered_capacitance.rs changed after that packet. Rather than treating the old packet as current, parent rebuilt the example and independently audited fresh parent-coupon.json into parent-audit/. The same eight failed frequencies and diagnostic-point errors reproduced. Historical evidence remains unchanged. Logs and fresh hashes are in debugging/p7-completion/parent-*.

Still blocked: structural-null handling across the P6/P7 boundary; real foil/barrel junctions and ports; scalable assembly/reduction and independent full-current-space accuracy; successful PCB extraction/DSL and mesh/domain controls. P8 board PI cannot start from an admitted extracted PDN. A separately scoped P11 for previously accepted uniform lines remains possible, but this checkpoint grants no additional readiness for arbitrary PCB channels or transitions. No new implementation task is issued.
