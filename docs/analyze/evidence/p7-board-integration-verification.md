# Historical P7 board integration ledger

This records earlier experiments and their original limits/paths. It is not the current dispatch or acceptance contract. Current direct-FEM-only state,300s/2GiB policy and actual refusal are recorded in [single-method checkpoint](p7-single-method-checkpoint.md). Retired preparation sources remain byte-preserved under ignored `debugging/p7-single-method-audit/retired-preparation/`; historical producer snapshots and results are unchanged.

# P7 board integration: design and preflight

Base `c3b436e`, CPU/F64. This continues the full existing assignment. Earlier
acceptance/evidence remains historical; this document will record the actual
board path and all remaining closure items. No P8/P11 implementation is started.

## Method decision, before new numerical experiments

Use the existing `local/mesh::extrude` conforming tetrahedral mesh and
`capacitance::CapacitanceOperator`, rather than force a bore into the z-only
product-field profile. The existing per-cell `Material` already represents XY
air/dielectric/copper ownership. Add conductor tetrahedra to that same mesh.
Continuous P1 metal potentials and three independent constant vector-current
bases per conductor tetrahedron give exactly the 3D P1 conductivity stiffness
after eliminating diagonal R. Shared finite foil/barrel faces therefore conserve
current without an ideal junction or duplicated metal volume. Point-only copper
contacts remain unsupported. All three current components are present inside
foil/barrel junctions, so the foil thickness is not silently removed from axial R.

For a cell volume V use w=e_axis/V^(2/3), R=1/(sigma V^(1/3)),
A=-V^(1/3) grad(N), and the unchanged physical volume-integral magnetic form.
Extend the accepted convex-polyhedron face/edge 1/r potential to tetrahedra;
retain the old prism path and its tests. Source-positive Duffy integration is an
independent fallback/reference; no softened distance or centroid self term.

Use sparse DC conduction and dielectric Schur actions, then split voltage/current
congruence before dense P6. Preserve actual finite electrode coordinates in the
voltage basis and include conductor constant modes. Internal potentials/currents
recover through stored bases. Never compile the large full space through dense
P6 merely to obtain a reduced model. Operator/basis errors must be checked on
independent holdouts, including local current/loss and charge/initial states.

Primary methodological sources read:

- [Ruehli 1974, author's IBM publication record](https://research.ibm.com/publications/equivalent-circuit-models-for-three-dimensional-multiconductor-systems): integral-equation partial-element formulation.
- [Odabasioglu/Celik/Pileggi, original PRIMA paper](https://www.cs.york.ac.uk/rts/docs/SIGDA-Compendium-1994-2004/papers/1997/iccad97/pdffiles/01d_1.pdf): block moments and congruence preserving the passive matrix structure.
- [Long Chen, variational P1 FEM](https://www.math.uci.edu/~chenlong/iFEM/doc/html/femdoc.html): conforming potential and weak conductivity/electric field forms.
- [Werner, original polyhedron potential record](https://ntrs.nasa.gov/citations/19950035213) and [NASA equations](https://ntrs.nasa.gov/api/citations/20170002066/downloads/20170002066.pdf): finite convex-source face/edge potential, already used by the prism kernel.

These motivate the numerical construction; they do not validate its implementation.

| Observable | Independent reference and normalization | Fixed gate / refinement |
|---|---|---|
| Tetra source potential | positive Duffy source integration, exterior volume/distance limit | 1e-6 relative; independent angular/source refinement |
| Tetra magnetic partition | accepted finite-prism/box integrals, same total current and exact support union | inherited 0.2% quadrature change and 1e-9 directed reciprocity; refine near integrals, do not average |
| 3D DC current/R/loss | analytic straight conductor and accepted conduction/DC in matching contacts | analytic 1e-9; board mesh convergence <=2%, independent model discrepancy explicitly retained |
| Electric field | existing independent plate/dielectric controls; separate bore/material control | inherited field charge/work/free-residual gates; geometry/mesh/domain separately varied |
| P6 model | original residual/KCL/power/energy/PSD/charge/gauge guards | unchanged |
| Full/reduced response and recovery | direct discrete controls and independent physical references | inherited 1% complete-observable reference; 1e-6 reduction training/holdout target, weak-entry floor 1e-12 of observable scale |
| Board path | immutable source PCB + actual ROI + explicit materials/contacts/returns | no counts-only admission, no dropped adjacent copper, no ideal via |

Each new numerical batch is supervised at <=120 s / 512 MiB, compilation separate;
`CARGO_BUILD_JOBS=1`. No expensive debug corpus or unchanged costly reference replay.
Unknown stackup/plating/padstack/return/fill assumptions must be authored explicitly
in the existing DSL/resources or fail. A small ROI is the first integration
checkpoint and will not be represented as resolution of the original ROI scale.

### Sparse magnetic candidate, before its numerical experiments

In addition to the finite-polyhedron reference, test the scalar Dirichlet Green
operator for each Cartesian component of the Newton potential: K u = F J,
L = mu0 F^T K^-1 F. K is the P1 Laplacian over **both** copper and dielectric/air;
F integrates each constant current over its actual tetrahedron (V/4 per vertex).
This congruent positive form avoids an N-current squared matrix and retains all
mutual couplings. It is a finite-domain approximation to the free-space volume
kernel, not an exact open-boundary magnetostatic solution. Its finite boundary
must be reported and separately converged; no result is admitted merely because
the solve is reciprocal/passive. The existing polyhedron/prism reference stays.

The component Poisson relation and free-space Newton-potential solution are given
in [KIT Maxwell lecture notes, section 1.3.3](https://www.math.kit.edu/ianmip/lehre/maxwell2009s/media/skript.pdf)
and [Michael Fowler's UVa derivation](https://galileoandeinstein.phys.virginia.edu/Elec_Mag/2022_Lectures/EM_31_Magnetostatics_II.html).
For the open partial-current basis this is specifically an approximation to the
componentwise Newton integral, not a claim that each basis has divergence zero.

Fixed admission gates for this candidate: normalized solve residual <=1e-8;
directed bilinear reciprocity <=1e-9; magnetic mesh and outer-domain changes
<=0.2%; <=1% against independent finite-prism/box reference in the complete
declared current space. Use graded outer XY rings/z planes to investigate domain
convergence within the unchanged 120s/512MiB cap. Failure of any gate leaves the
candidate unavailable to production extraction. It does not relax the accepted
near-kernel gates or justify replacing metal by point currents.

### Exterior enrichment decision (before its experiments)

The plain finite-box candidate failed its independent cube control: n=8 and
margin=32 sides gave 1.758315459476e-10 H versus the independent convolution
1.882312644390e-10 H. The n=12/margin32 plan correctly refused the 512MiB cap.
These are failed candidates, not an accepted magnetic model.

Next test a variational infinite-domain enrichment, keeping the same gates.
Add the nine regular solid-harmonic potentials through l=2 to the finite-element
space. Let h_l be a real homogeneous harmonic polynomial evaluated at r/a;
inside r<a use psi=h_l*((2l+3)/2-(2l+1)*r²/(2a²)); outside use
psi=h_l*(a/r)^(2l+1). Values and first derivatives match at r=a, and each
function decays at infinity. The sphere encloses all copper, without replacing
or modifying that copper. Its exact whole-space gradient energy is diagonal:
K_ll=a*integral(Y_l²)*2(2l+1)(2l+3)/(2l+5). This follows directly by integrating
psi*(-Laplacian psi) inside the sphere; orthogonal angular modes have zero cross
energy. Thus exterior energy is included explicitly, not fitted or gmin-shifted.
The sparse field-to-enrichment coupling is integrated over the actual mesh;
the source-to-enrichment load integrates the actual constant tetra current.
Eliminate the nine dense coordinates through the positive Schur complement.
An unresolved/nonpositive Schur complement refuses; no eigenvalue repair.

This is a Rayleigh–Ritz trial-space construction motivated by the exterior
multipole expansion (the UVa source above) and variational infinite elements,
not a claim to implement a cited package. Independently check the analytic
whole-space energy coefficients, quadrature orders4/8/16, mesh/domain changes,
and complete declared-space accuracy against the same fixed references.

### Full solve and P6 conditioning decision

The matrix-free full frequency solver uses right-preconditioned restarted GMRES,
with an anchored sparse DC factor and the complete conductor-component charge
metric as a preconditioner. Physical constants are separate from shape voltages;
their gradient is identically zero. No preconditioner coefficient enters physical
R/L/C. Admission still requires original KCL/KVL, complex power and Joule-power
errors <=1e-8 (and a scaled GMRES residual <=1e-12). Both original unit-port and
opposing drives passed these guards on the STM32 mesh. The initial5+6 model did
not pass recovery/loss error; its diagnostic packet is retained as failed.

Full-solution snapshots now expand both potential and current spaces. Potential
bubbles vanish at the finite electrodes; a partition of unity is retained, so
the existing P6 physical terminal/charge contract is preserved. The first17+22
candidate hit the unchanged P6 NETWORK_RESIDUAL guard (6.63e-7). Investigate an
algebraic conditioning fallback: use the **already certified** null(A^T)
physical charge modes as explicit frequency coordinates, then equilibrate the
linear equations. Preserve full C/R/L and every physical mode; only the certified
zero A coupling is evaluated as zero. Recheck the original physical equations
with the unchanged1e-8 gate after recovery. Existing structural-gauge handling
is unchanged, and the fallback is used only after the old direct solve fails.
Run coupled/composition/time/mixed/delay and the accepted17-frequency structural
regressions; this is a frequency conditioning change, not a reopened gauge fix.

## Live verification ledger — implementation continues

This is **not a completion handoff**. P7 is not closed. No unsupported candidate
has been exposed as a successful extraction through the DSL.

The new board adapter uses the native board-scoped overlay, all retained copper,
actual finished-hole wall geometry and air bores. A raw ROI versus its overlay
image initially created artificial narrow air strips. The canonical ROI now
uses the same overlay grid; the spurious16nF term disappeared (the largest
remaining candidate C coefficient is about0.25pF). That failed packet is retained.
Provenance boundaries inside continuous equal-material copper need not be field
interfaces. **Electrode contours do**, and their mesh surface area must match the
prepared physical contact area within1e-8. Exact electrode contours are being
verified now; older contact-selection candidates are not acceptance evidence.

| Original obligation | Current evidence | Closure |
|---|---|---|
| Actual copper, holes, native fills, source/net/layer mapping | Native preparation plus a shared conforming material mesh; snapshot/cut policies now explicit; exact electrode contours and ownership reach candidates on all3 boards | Physical refinement and end-to-end admission pending |
| Finite foil/barrel/return contacts, no doubled metal/R | STM32 CAN_L/GND two-barrel candidate; independent annular DC/flux/loss control passes | Actual board admission pending |
| Explicit finite ports and physically defined ROI cuts | Electrode quotient with source contours and surface-area gate; unknown fill freshness and insulated cuts require authored policy | End-to-end negative checks pending |
| Consistent full R/A/L/C and recovery | Full14,130-current operator solve passes KCL/KVL/power; no dense full P6 compilation | Candidate only; magnetic/field geometry admission pending |
| Sparse/operator scaling beyond64 potentials/supports | Actual board >1,000 potentials and >14,000 currents; sparse factors and component-aware GMRES; new Newton plan has5,245,501 near pairs | STM32 q32 construction reaches its110s deadline; original larger ROI scale unresolved |
| Passive automatic voltage/current reduction | Harmonic partition plus full-solution voltage bubbles/current snapshots, congruent R/L/C | Holdout gates still fail; not admitted |
| Independent complete-space magnetic accuracy | Same-mesh Poisson+l<=2 fails the exact alternating-current lower bound; its finite-volume replacement passes independent24-tetra and eight-prism controls with explicit empirical reference uncertainty | Same-mesh candidate rejected; replacement has no actual-board admission |
| P6 frequency conditioning with unchanged guards | Certified charge-coordinate fallback, source equations rechecked; previous43 coupled/delayed/mixed/physical-terminal/structural-gauge regressions pass; fresh thin-field and layered-capacitance controls pass | Actual-board field mesh/domain accuracy remains pending |
| Actual full line/pair/different-layer paths | Existing P7a/P7b remain unchanged | Not integrated yet |
| Confirmed frequency-dependent conductor/dielectric loss and band | Full volume formulation can resolve eddy redistribution; current trial profile is constant sigma/lossless dielectric | Full requested loss/band coverage remains open |
| Physical PDN multiport and causal transient resource | Coupled descriptor construction exists for the candidate | No accepted PDN/transient evidence yet |
| Direct agreed DSL and worker | Additive viaId electrode selector/typecheck/positive and negative binding test pass | Native extraction executor not yet connected |
| STM32, TPS63060DSCR, BandAmpV5 positive scenarios | All three reach internal geometry/P6 candidates; immutable native fills retained; no accepted physical extraction | Three-board acceptance remains open |
| Visualizations and independent numerical results | Candidate copper/current PNG reviewed; independent70-digit descriptor/cell-volume loss check; fresh self/full24-tetra control PNG reviewed | Candidate/control figures do not establish actual-board continuum accuracy |
| Complete physical, mesh, domain, frequency and negative checks | Focused primitive/full-solver controls below | Full matrix remains incomplete |
| P8/P11 ready inputs | No accepted complete physical resource yet | **Not established** |

Focused results so far: tetra source vs positive Duffy; 3D conductivity elimination,
current conservation and Joule identity; finite annular barrel DC/axial flux;
global exterior harmonic gradient/energy; certified charge-coordinate small-loss
and load-cache invalidation. All pass. Historical4 ignored tests have not been
reclassified as passed. Broad native/addon/DSL regressions will be run after the
working path stabilizes, rather than repeated after each local diagnostic.

`p6-regressions-v0` ran the five already-compiled binaries (no duplicate build):
coupled13, delayed9, mixed4, physical terminals8, structural gauges9;43 passed,
0 failed/ignored. The structural test suite retains the17-frequency control.
This packet verifies the charge-coordinate P6 change, not later field algebra.

Selected immutable packets under `debugging/p7-board-integration/`:

- `candidate-v6`: canonical-ROI candidate, finite-box magnetic operator, not admitted.
- `full-candidate-v0`: full sparse unit/opposing solves; initial reduced model fails recovery/loss accuracy.
- `conditioned-candidate-v1`: improved P6 conditioning,17+22 candidate; holdout recovery still fails.
- `cube-n16-d8-e4`, `cube-n12-d8-e4`, `cube-n12-d4-e4`: independent cube mesh/domain controls.
- `core-tests-v1`, `charge-coordinate-tests-v0`: focused positive controls.
- `visual-v0`: explicitly labelled candidate figures and independent descriptor/loss audit.
- `exact-contacts-trained-v0`: exact finite-electrode contour attempt refused by the unchanged field balance guard. Investigation is continuing; no false success.

Each optimized packet remains <=120s/512MiB. A debug training packet hit its
120s supervisor and was retained as a deadline refusal. Compilation is separate,
`CARGO_BUILD_JOBS=1`; no expensive debug corpus, GPU or thermal changes.

### Stable variational evaluation diagnostic

`physical-ownership-v1` retains exact contacts and all canonical copper and now
passes provenance construction, but refuses field admission: charge balance
2.206e-6 and energy/reaction2.273e-8, despite assembled-row residual7.4e-16.
This is not evidence of acceptable capacitance. The next internal candidate
forms P1 stiffness and evaluates fields/reactions in nodal differences, with
constant shape represented algebraically, plus at most8 sparse iterative
refinements against that variational action. No element, mode or mutual term is
removed. The three existing1e-8 field gates remain unchanged; residual scaling
initially used actual element reactions, which is undefined as a useful backward
error at zero-field nodes. `field-differences-v0` achieved charge3.55e-15 and
energy4.82e-15 but failed that accidentally stricter residual2.65e-3. The followup
retains the original assembled-equation backward-error denominator and uses the
stable variational numerator; all original thresholds are unchanged. The
diagnostic `field-differences-v0` was launched before this
ledger entry (procedural preflight omission, not acceptance evidence).

Before further runs: test exact constant voltage and an independently known
affine field on a thin fully prescribed tetrahedral extrusion, requiring1e-8
energy/reaction/charge and1e-8 relative independent field energy; then rerun the
unchanged exact-contact STM32 input. Numerical batches remain120s/512MiB.
Convergence, field/material mesh and magnetic whole-space acceptance remain
separate; stable algebra alone admits neither continuum accuracy nor P7.

### Compensated residual refinement preflight

The exact-contact candidate's full GMRES stagnates at1.279e-12 against its
unchanged1e-12 gate (both4 and8 restarts); it remains refused. The next candidate
keeps the sparse factors and all public state in CPU/F64, but evaluates the field
variational residual and accumulated correction with a two-component F64
expansion (error-free sum and FMA product). This is local compensated arithmetic,
not a new material model or a tolerance change. It is motivated by
[Carson/Higham's residual-refinement analysis](https://eprints.maths.manchester.ac.uk/2629/1/cahi18.pdf)
and [Hida/Li/Bailey's two-double arithmetic](https://www.researchgate.net/publication/228570156_Library_for_Double-Double_and_Quad-Double_Arithmetic).
No general convergence guarantee is inferred for this mesh.

The thin-field affine control uses dyadic coordinates and voltages with a
nonzero common offset and all three derivatives, so its weak transverse
derivative is an exactly representable boundary input rather than a comparison
against a decimal voltage already rounded beyond the requested error.

The compensated-field-only candidate still refuses the same full solve at
1.273e-12. Next apply the same compensated dot/sum to the actual conductor
gradient and its adjoint divergence (all3 components/tetra remain). This tests
whether slender-element current arithmetic, rather than field solve precision,
sets the remaining floor. The gates, request, factorization and frequency set
are unchanged. No additional debug full-solve batches: both previous debug
attempts reached their30s per-solve limit, so use optimized supervised runs.

Conductor compensation alone did not lower the STM32 floor (1.280e-12 at10kHz,
2.059e-12 at100kHz). The same unchanged full solve remains refused. Next refine
the magnetic sparse/block solve against compensated residuals of its **existing
assembled coefficients**, preserving the same finite-domain/enrichment model.
The forward/backward source maps use identical stored F64 coefficients and
compensated accumulation, to avoid excitation-dependent rounding in the inverse.
Use at most8 factor corrections and require a1e-14 global RHS-relative block
residual in addition to the original1e-8 row guard. Test operator linearity and
the unchanged complete physical audits; no physical accuracy is inferred from
this numerical correction. All other domain/mesh/complete-space gates remain.
Magnetic compensation passes numerical linearity8.60e-17 but leaves the full
solve at1.280e-12. Next retain the low F64 correction when accumulating GMRES
state and evaluate A(x_hi)+A(x_lo), instead of rounding that correction out before
the next residual. Keep the same Arnoldi/preconditioner and1e-12 final gate;
physical audits use both shape components. This tests state representation,
not a tolerance change. Recovery must retain those components before admission.
The numerical linearity probe uses0.75*q_first-0.25*q_last of the independently
constructed DC seed modes and compares L of the combination to the combination
of L actions, with2-norm divided by the root-sum-square of the two weighted
images. Its numerical gate is1e-12; it is not an accuracy reference.

### Mandatory nonuniform magnetic-space control, before measurement

Uniform cube convergence cannot admit the complete constant-tetra current space.
For a cube split into its six ordering tetrahedra, alternate the sign of Jz by
coordinate permutation. Its P1 nodal loads vanish. The nine l<=2 enrichment loads
also vanish by permutation/central parity. The exact Newton energy is positive.
This is a possible categorical failure of the same-mesh sparse candidate, not a
reason to accept only uniform currents. Verify this control before production.

Independent reference: on the unit centered cube choose the compact scalar test
function psi=(x-y)(y-z)(z-x)*(1-4x²)*(1-4y²)*(1-4z²), extended by zero outside.
The variational Newton form gives the exact positive lower bound
mu0*a^5*(integral(sign(J)*psi))²/integral(|grad psi|²) for Jz=+/-1A/m² and cube
side a. Compute both polynomial integrals with exact rational arithmetic,
independent of the production FEM/kernel, then compare the candidate energy to
that lower bound. A value below the bound beyond1% refuses the candidate for its
declared complete space regardless of uniform-cube success. Keep120s/512MiB;
this reference is polynomial arithmetic, not the expensive board corpus.

Result: the exact rational integrals are M=8/1575, E=1408/165375,
M²/E=1/330. For the active0.5mm cube the true quadratic energy is at least
1.189997217269e-25J. `magnetic-null-e4-v0` returns6.850295217870e-57J
(0.38s,10.6MiB). Thus the sparse candidate fails the complete-space gate
categorically; its uniform-cube success is insufficient. It must be replaced
or enriched with a separately verified source-resolving method before physical
production/DSL success. This is an internal method rejection, not a claimed
external blocker or P7 completion.

The compensated GMRES state fixes the discrete numerical floor:
`compensated-gmres-trained-v0` completes in9.57s/85.9MiB, all full10k/100k training
and31.623k holdout solves pass existing residual/KCL/KVL/power gates. It constructs
28 voltage and40 current modes. However maximum weak current-coordinate
recovery error is0.154 at an exact training excitation and0.610/24.6 at holdouts;
cell loss errors up to4.22e-4 also exceed the1e-6 reduction target. These remain
refusals, even though total loss agrees much more closely. Low shape corrections
must also be retained in future projection/recovery. No physical resource is
admitted on the strength of these discrete candidate solves.

### Replacement magnetic operator preflight

Implement a source-resolving hierarchical finite-volume Newton operator:
exact finite tetra source potentials with outer volume quadrature for near
blocks; tensor Chebyshev interpolation of1/r only on separated cluster boxes;
integrals of the interpolation polynomials over actual tetrahedra, nested
upward/downward translations and reciprocal far blocks. No copper is a point
source and no far/weak mutual coupling is deleted. The cluster/interpolation
construction follows [Hackbusch/Grasedyck/Börm, sections2–3](https://www.dml.cz/bitstream/handle/10338.dmlcz/134156/MathBohem_127-2002-2_11.pdf)
and [Hackbusch/Börm's interpolation H2 formulation](https://doi.org/10.1016/S0168-9274(02)00121-6).
This is a new candidate, not an admitted implementation of either publication.

Use bounding boxes of complete finite supports, max box diameter<=0.5*box gap,
leaf8; first interpolation orders3/4 and near volume orders4/8/16, refined
separately. Bound owned source/cluster/near/far/translation/workspace storage
before allocation, plus original mesh retained storage, under512MiB; check the
120s deadline throughout. Do not change old64-support reference caches or P6 caps.
Initial references: the exact alternating-current lower bound and independent
cube convolution; then full small-space finite-prism/tetra reference,1% complete
observable accuracy,0.2% quadrature/interpolation changes,1e-9 directed
reciprocity and unchanged PSD/passivity guards. Admission needs all of them;
fixing the null counterexample alone is insufficient. No production/DSL exposure
before physical and reduction/recovery gates pass.

The first finite-tetra near candidate refuses order8 at directed difference
4.584e-9. Order16 passes all48-source near pairs (2.773e-10 worst), and the
alternating control gives2.72216e-25J, above the independent1.19000e-25J lower
bound. This fixes the categorical null example but does not establish1% accuracy.

Before attempting board near blocks, improve the positive tetra fallback by
analytically integrating its longest-edge coordinate. Write source barycentrics
as lambdaA=1-lambdaC-lambdaD and integrate the segment from
a+lambdaC(c-a)+lambdaD(d-a) to that point+lambdaA(b-a). The remaining source
integral is6V/|b-a| times a positive2D simplex integral of the endpoint asinh
difference. For near/far stability use2*atanh(length/(r_start+r_end)) when its
argument<=1/2 and sign-aware asinh sums/differences otherwise. Fan the2D simplex
from the closest projected query point, grading radial/angular coordinates by
t^4; all source volume is retained. The old positive3D Duffy function stays as
an independent reference. Check ordinary interior/boundary/exterior queries,
independent prism partition and thin/translated controls before using this in
board admission. Keep the existing source refinement and all outer/pair gates;
the transformation itself does not certify convergence.

Thin-prism partition test exposed a2.94e-6 discrepancy at the exterior query
(0.5L,-2W,0.5H), W=1e-7m; it remains a failed control pending diagnosis.
Before changing any tolerance, calculate an independent rectangular-slice source
reference: integrate x exactly from0 to L*(1-y/W), then independent tensor Gauss
over y/z, splitting at the query projection and the moving x-endpoint crossing,
grading each interval toward both ends by t^4. Orders32/64/128/256 must agree to
1e-10 relative before using the frozen reference. This isolates whether the
new tetra rule or the old prism fallback sets the discrepancy. The new tetra
source1e-6 accuracy target remains unchanged; no failed result is erased.

The rectangular-slice reference converges to~1e-14 and confirms the old prism
value. The tetra rule needed additional splits where an analytic segment
endpoint crosses the query's axial coordinate, plus positive evaluation of all
three barycentric coordinates and both graded-coordinate complements. The
corrected control (`positive-line-controls-v5`) passes all ordinary Duffy and
thin-prism partition queries in1.71s/6.1MiB. Next tighten **tetra-only** source
cancellation/refinement estimates to1e-10 to support the existing1e-9 near-pair
reciprocity gate. Existing prism source policy remains unchanged. Verify actual
PointPotential tetra calls against the same independent prism/slice values;
refusal through order63 remains a refusal, never an unchecked fallback.

`positive-line-controls-v6` passes those actual tetra calls; all6 focused volume
controls also pass. The first board H2 attempt (`integral-board-p3-q16-v1`)
refuses a source rule on tetra291. Its XY vertices differ by1.7e-18m and its
condition is~6e13. The2D CDT currently intersects independently prepared material
and electrode contours after native overlay; this creates an additional F64
arrangement below the board overlay grid. The packet retains exact tetra/query
and all7/15/31/63 source values. Also, its plan has5,408,334 near pairs and29,892
far blocks: it has **not** solved original ROI scaling.

Next geometry candidate: construct the common planar material/electrode partition
with the already accepted board-scoped overlay first, then constrain the CDT
only with that common noded partition. Retain every positive polygon, all native
sources/copper and finite contacts; no feature/area filter, proximity weld or
larger overlay tolerance. Carry material membership through each Boolean split
instead of recreating material interfaces by unrelated CDT intersections.
Before admission require each physical copper set's area within1e-8 of its
prepared area and unchanged component/hole counts; require each finite contact's
mesh area within the original1e-8 gate against its original prepared source
area. Keep explicit native overlay error metadata. Re-run independent geometry,
DC/field and negative controls before calling it physical. If any invariant
fails, retain the refusal; do not relax it to make the new mesh pass.

`common-arrangement-geometry-v0` passes those gates:15 positive atoms,
maximum copper area change1.271e-9, contact area change4.405e-9, unchanged physical
copper component/hole counts. No positive polygon was filtered. The largest
conductor-cell condition drops from~6.9e13 to~3.7e6; source/port areas remain
checked against the original prepared geometry. This is geometry preparation
evidence only (debug30.2s/37.6MiB), not acceptance of field/kernel/reduction.
The same native overlay error policy and CAD snapshot/cut assumptions apply.

### Fast self-integral preflight

The hierarchical plan's millions of near interactions require analytic dimension
reduction, not higher caps. [Reid's primary Taylor–Duffy paper](https://arxiv.org/pdf/1708.04666)
reduces identical-tetra1/r integrals to1D. Implement the constant-density self
case directly via its covariogram: in standard-simplex difference coordinates u,
the overlap volume is(1-max(sum positive u,sum negative |u|))³/6. Integrating the
homogeneous radial coordinate exactly gives I=(3/10)V² times the boundary integral
of1/|Au| over the difference body. Opposite facets pair by evenness, leaving
seven positive triangle/rectangle facet integrals. Integrate one facet coordinate
analytically by the same stable segment primitive; grade the remaining1D rule
at distance minima and endpoint crossings. This retains the exact finite support.

Before replacing near diagonals, compare with the existing analytic-source/
outer-Duffy tetra self rules32/64 on ordinary tetrahedra and independent prism/
cube partition controls. Keep0.2% quadrature change and1e-6 independent primitive
accuracy; use tighter1e-10 self refinement for near-pair normalization. No
face/edge adjacency or full-board speed claim follows from a fast self primitive.

Both additional actual-board candidate requests now prepare finite power-pad
electrodes and a ground-return barrel and reach a P6 candidate (`pdn-tps-geometry-v0`,
`pdn-bandamp-geometry-v0`, <67MiB and1.5s each). They are **not accepted physical
PDN resources**: magnetic accuracy, reduction/holdouts and DSL remain open.

Before board admission, verify exact representable sum/product cancellation,
the existing affine thin-element independent energy control and ordinary field
regressions. Repeat the same STM32 request/frequencies; retain1e-12 full solver,
1e-8 physical field/KCL/KVL/power and all original accuracy gates. Refinement is
bounded to8 sparse corrections,120s/512MiB per batch. If it does not improve the
unchanged residual, retain the refusal and investigate the operator rather than
relaxing the gate. Other magnetic/mesh/domain/reduction admission stays pending.

### Restart checkpoint: source-resolving diagonal refinement

No leftover cargo/rustc/board numerical process was running at restart. The
saved self control passed (0.37s inside the test); the previously unrun eight-prism
control now passes in `resumed-eight-prism-v0` (38.36s supervised, 5.79MiB).
It checks the complete **eight-dimensional uniform-prism** space, not all24
tetrahedral source coefficients. That distinction remains a physical acceptance
requirement. No expensive successful board calculation was repeated.

After one separate release build, the unchanged STM32 ROI with the new integral
operator (`resumed-integral-board-v0`, p3/q16/train) refuses at self source573:
1e-10 self refinement is unresolved through64. Its near plan is5,245,501 pairs
and31,327 far blocks; 2.825s/224.7MiB observed. This is still neither a scalable
accepted operator nor a DSL resource. The common arrangement/field dimensions
are unchanged (1182 potentials,14193 current coefficients).

Next primitive preflight: freeze this exact source573 as a regression and retain
all8/16/32/64 values before changing arithmetic. Compare the self primitive on
all vertex permutations and translated inputs; normalise by the original self
integral, keep1e-10 refinement and1e-6 independent primitive gates. Use exact
positive facet integration and compensated products only to resolve cancellation,
never delete the narrow physical support. Independent analytic-source/outer-Duffy
ordinary controls and the frozen prism reference remain in force. Supervise each
numeric packet at120s/512MiB; compile separately with one cargo job. A resolved
self rule does not admit any mutual pair, complete space, board, or reduction.

`resumed-sliver-self-v0` preserves the failed32/64 relative change2.77e-9.
The separate70-digit facet evaluation reproduces all16/32/64 values (F64 versus
70-digit relative difference~5.5e-12), so compensation is **not** the remedy for
this refusal. `self-arithmetic-reference-v0.json` is an arithmetic diagnostic,
not an independent physical reference. Next retain the same analytic reduction,
and subdivide the remaining facet coordinate at the geometric transition widths
rho_min/|d rho_vector/ds|, doubling outward from its distance minimum. Integrate
every interval positively with the existing fourth-power grading. This resolves
near-planar logarithmic transition layers without changing support, cap, source
order or the1e-10 self refinement gate. Keep the frozen source and all ordinary
independent controls. No source-volume or aspect filter is introduced.

Complete-space reference preflight: the existing eight-prism control admits only
uniform density within each prism. Extend its observable to the24 independent
constant-tetra source densities (Cartesian directions are three identical
uncoupled scalar Newton blocks). Freeze the full scalar matrix, not only one
uniform current. An independent randomized Sobol product-volume integration uses
uniform barycentric samples, original finite vertices and direct1/r, with four
fixed seeds and nested2^14/2^16/2^18 samples. It imports no production kernel,
self transform, source potential or magnetic solver. Reference uncertainty is an
**empirical screen**, not a rigorous certification: Frobenius norm after whitening
by the finest reference of2*last matrix change plus6*between-seed standard error.
Gate the sum of candidate full-space whitened error and this screen at1%; retain
failed/unresolved references. Existing prism source/outer and exact alternating
lower-bound controls remain independent checks. Each generation/audit batch is
<=120s/512MiB. Candidate interpolation/quadrature changes still require their
separate0.2% gate; this reference alone does not establish board accuracy.

The direct6D QMC reference is **unresolved**: its fixed full-space empirical screen
is7.211%, so it cannot admit the operator (`tetra-space-reference-v0.json`,
43.09s/121.0MiB). This failed reference remains immutable. Next evaluate the same
independent finite-volume expectation after analytically conditioning on one
uniform target segment. For target barycentrics(c,d), the marginal density is
6(1-c-d); draw c=1-u^(1/3), d=u^(1/3)*(1-sqrt(v)), then average the exact segment
asinh difference divided by its nonzero length. Draw the source uniformly with
three independent simplex coordinates. This5D estimator integrates the actual
finite support; it imports no production source routine, fan or covariogram.
Keep the same four seeds, powers,1% full-space-plus-uncertainty gate and resource
caps. Directed production pair reciprocity is not replaced by QMC averaging.

The conditioned estimator's unpartitioned four-seed run hit its internal110s
reference deadline before writing a result. It is a refusal, not a passed
reference. Subsequent runs persist one predeclared seed per supervised120s packet
and aggregate the four frozen matrices without recomputation; no sample count,
normalization or tolerance changes. The supervisor now also accepts a validated
local reference-script filename and captures its stdout/stderr/source hashes,
wall/RSS/refusals exactly as for existing native examples. No native user
`native/analyze/debugging/` output is touched.

The frozen four conditioned seeds produce a1.6324% reference uncertainty screen
and0.2063% candidate discrepancy; their sum1.8387% **fails**1%. The estimator is
not yet accurate enough to admit even this complete24-cell control. Preserve the
values and refine to2^19, then2^20 only if needed. Each seed extension samples
only the next Sobol half and combines it with its frozen earlier mean; it does
not rerun successful earlier integrations. Four fixed seeds,1% total gate and
120s/512MiB per supervised seed extension stay fixed. Report total generation
cost separately from the aggregation/audit cost. Make failed admission return
nonzero in the audit tool (earlier JSON false results remain explicit failures
even though their wrapper process returned0).

The source573 fix allows **all4731** diagonal rules on unchanged STM32 to pass.
The board then reaches a mutual-pair refusal: pair109/110 at q16 differs by
1.3493e-9 against the unchanged1e-9 self-normalized reciprocity gate
(`resumed-integral-board-v1`,6.305s/225.0MiB). The next same-input q32 packet is
only a separate near-quadrature refinement, not a physical/scale acceptance.

### Complete24-tetra control result and board refusal

Four incremental2^19 seed extensions pass the fixed complete-space screen:
candidate whitened Frobenius error0.0007576501, empirical reference uncertainty
0.0067806364, sum0.0075382866<0.01 (`resumed-complete-space-audit-p19-v0`).
The bound covers the24 independent scalar cell densities and hence each of the
three uncoupled Cartesian Newton blocks; it is stronger than testing eight
uniform prisms. The **reference uncertainty remains empirical**, not a rigorous
certified continuum enclosure. The finest matrix/vertices/uncertainty are frozen
in tracked `board-integration/tetra-space-reference.json`; future Rust regressions
reuse it without resampling. The older unresolved6D/5D references remain saved.
Independent uniform-prism full-space error is1.3159368e-7. Neither control covers
the actual board's sliver geometry or establishes the0.2% board refinement gate.

Measured supervised seed packets: p18=39.90+32.76+23.44+24.05s,
p19=23.39+20.02+26.03+23.14s;
peak<133MiB. Aggregation/audit2.21s/76.0MiB. Compilation is separate, release
library/example rebuild8m40s with one cargo job; these are not extraction times.

`resumed-integral-board-q32-v0` retains the same full ROI, physical materials,
electrodes, ports and14193 current coefficients. It reaches the110s internal
construction deadline (114.14s supervised,224.3MiB), before a P6 model exists.
Increasing the tensor near order therefore does not resolve original scaling.
No successful PCB→P6→DSL path, accepted PDN resource, channel or P8/P11 readiness
is claimed. No external permission/contract blocker has been established: a
faster verified mutual/source-resolving construction is still implementation
work inside the existing assignment. All old physical/reduction refusals remain.

The new frozen-reference Rust replay initially fails its **input identity** check
before the numerical gate (`resumed-frozen-space-release-v0`). The Python/native
emitted vertices agree bitwise in the independent audit; reading their decimal
JSON through default serde_json changes some coordinates by1ULP. Preserve exact
source-identity checking by storing binary64 vertex bits as JSON u64 integers;
compare those to native mesh-coordinate bits. Do not loosen the identity test or
change the physical geometry/independent reference. Matrix integration error is
still checked at the original1% including empirical reference uncertainty.

The old supervisor recorded child failures in run.json but returned shell0.
It now propagates the child status; historical packets with `exitCode!=0` remain
failures regardless of their old wrapper's shell code. The numerical outputs,
resource measurements and all refusal evidence are unchanged.

### Focused resume checks and reproducibility

Fresh optimized tests, all under the supervisor:

| Packet | Result | Supervised wall / observed peak |
|---|---|---|
| `resumed-volume-controls-release-v0` |8 passed,0 failed/ignored: independent tetra source/self, sliver permutations/reorigin, thin affine field, DC/annular barrel and Joule/conservation controls |2.013s /5.87MiB |
| `resumed-field-regressions-release-v0` |4 passed,0 failed,**4 historical ignored remain ignored** |0.705s /5.54MiB |
| `resumed-frozen-space-release-v1` |1 passed: independent complete24-tetra screen0.753829% and eight-prism bound1.31594e-7, resource/vector negatives |3.597s /5.23MiB |
| `resumed-supervisor-refusal-v0` |invalid predeclared seed rejected; both child metadata and wrapper shell exit1 |2.008s /74.95MiB |
| `resumed-kernel-visual-v1` |fresh static figure generated, hashes recorded, PNG visually inspected; original board still explicitly refused |2.251s /81.55MiB |

These13 fresh passing tests and4 retained skips are not a full P7 acceptance.
The older43 P6/physical/mixed/delay/structural-gauge passes remain historical
regression evidence; they were not rerun without a relevant P6 code change.
Broad addon/worker/DSL/typecheck/build completion checks remain for the actual
working board extraction path. This resume changed no TS or public runtime DTO.
`git diff --check` passes. Python files parse successfully. No commit, GPU,
Thermal, P8/P11 implementation or native user-debugging modification occurred.

Representative commands (repository root, CARGO_BUILD_JOBS=1 for compilation):

```powershell
cargo build --release --manifest-path native/analyze/Cargo.toml --example board_volume
cargo test --release --manifest-path native/analyze/Cargo.toml --test volume_current --test layered_capacitance --test volume_integral --no-run
python tests/fixtures/analysis-local-extraction/board-integration/run.py test:volume_current-979e82b0abf9c117 <fresh-packet> --release --nocapture
python tests/fixtures/analysis-local-extraction/board-integration/run.py test:layered_capacitance-2a5473eca6d53840 <fresh-packet> --release --nocapture
python tests/fixtures/analysis-local-extraction/board-integration/run.py test:volume_integral-1d3a4447e4a6ff41 <fresh-packet> --release --nocapture
python tests/fixtures/analysis-local-extraction/board-integration/run.py STM32G0_CAN_RS485 <fresh-packet> --release integral:3:16 train
python tests/fixtures/analysis-local-extraction/board-integration/run.py STM32G0_CAN_RS485 <fresh-packet> --release integral:3:32 train
```

Use current cargo-emitted test hashes when rebuilding; do not start duplicate
builds or overwrite immutable packet directories. Each packet contains command,
binary/input/source hashes and source snapshots. The frozen Rust fixture replays
the successful independent reference without regeneration; the QMC generator's
per-seed files preserve incremental refinement ancestry.

[Reviewed control figure](../../../debugging/p7-board-integration/resumed-kernel-visual-v1/kernel-controls.png)
and [figure/source metadata](../../../debugging/p7-board-integration/resumed-kernel-visual-v1/figure.json).

The required first real-board physical PCB→P6→direct DSL path is **still absent**.
Remaining work is substantive: verified faster mutual construction on the actual
ROI, actual field/magnetic mesh/domain/complete-space accuracy, adaptive passive
voltage/current reduction with all local-current/loss holdouts and initial-state
recovery, intermediate-frequency loss coverage, line/pair/layer-transition paths,
physical PDN multiport/transient admission, three successful direct board DSL
scenarios and final addon/worker regression matrix. No newly established external
blocker makes these implementation obligations disappear. **P7 is not closed**.

### Small physical via/return checkpoint preflight (2026-10-26)

The original CAN ROI remains refused. First integration now also considers the
actual STM32 RS485_B via0 and GND via65, with an explicitly insulated rectangular
foil cut around their complete barrels. All intersecting copper stays present.
Mesh/curve/ROI/domain changes are separate refinements, never reference fitting.
The original1e-8 prepared-area/contact and topology gates remain unchanged.

Repeated Boolean intersection rounding on the board-wide integer grid can exceed
those gates even for a coarser curve: measured changes1.27e-8..5.0e-8 refuse.
Next use a nested, finer dyadic overlay grid only after board preparation and
initial ROI clipping. Every original prepared coordinate is exactly representable
on the refined grid; no larger coordinate/feature tolerance is introduced. The
new grid covers all clipped material/contact cuts and uses the same native
Boolean implementation. Compare the original prepared area/topology and finite
contact area with the unchanged gates. Retain every positive cell and preserve
all earlier refusal packets. Source attribution still uses the original board.

For magnetic mutual construction, test order-adaptive directed outer cubature.
Start at order4 and double only unresolved pairs; require both directed values
to converge by the original0.2% quadrature gate and require their self-normalized
difference<=1e-9. Retain the forward value, never average. Maximum near order is
an explicit request ceiling; unresolved pairs refuse with all sampled values.
This avoids spending order32 on smooth distant pairs while preserving the
complete tetrahedral current space and the accepted source/self kernels. Keep
independent frozen complete24-tetra and primitive controls, then compare the
actual ROI at independently refined ceilings and meshes. Per-packet resources
remain120s/512MiB; compilation is separate.

The strict64-ceiling strip candidate still refuses directed pair268/201
(`adaptive-strip-integral-v1`); raising tensor order alone is insufficient.
Next refine the target tetrahedron locally by longest-edge bisection after the
fixed tensor ceiling refuses. Use positive child volumes and the existing exact
finite source potential. Bound the adaptive quadrature's estimated absolute
error to1e-11 times the self-pair normalization, then still require the original
1e-9 directed reciprocity and0.2% global quadrature-change checks. This estimator
is not a rigorous continuum enclosure. A4096-leaf cap and original wall/memory
caps refuse unresolved cases. No child or physical copper is removed.

An optional explicit minimum triangle angle0..30 degrees changes mesh refinement
only (default30 remains). A zero angle disables quality refinement, retaining all
physical contour constraints and area refinement; slivers are neither filtered
nor welded. Any resulting field/current/primitive ill conditioning must still
pass its original guards, and separate mesh convergence remains mandatory.

A direct dense P6 solve can pass its normalized equation residual while losing
the tiny real-power balance under a large capacitive common potential. Extend
the existing charge-coordinate fallback to this independently checked real-power
failure, then rerun the unchanged original equations and physical power gates.
Use all actual same-block conductances/resistances in that balance; do not alter
R/L/C, remove physical charge modes, regularize or lower any tolerance.

### Three-terminal smaller physical ROI preflight

The two-barrel strip still exceeds the110s integral construction deadline.
Next retain one complete actual RS485_B barrel (via0), the intersecting real
TOP GND pad-foil conductor from via65, and **all** intersecting BOTTOM ground
copper. Cut the ROI before the GND bore, so that bore/barrel is wholly outside;
no intersecting hole is omitted. Both physical differential ports use the same
finite TOP GND foil electrode. This is an explicitly three-terminal via resource,
not a channel with a plated return transition; it cannot close the separate
signal/return-via obligation. The BOTTOM GND foil remains floating after the
insulated ROI cut, even though its source net also says GND. The finite return
contact, ordinary signal barrel, material/contact mapping and all original gates
remain mandatory. Freeze ROI[23.1983,3.7709]..[23.9243,4.1229]mm, same materials,
curves0.001mm, max area1e-6m2, vertical step0.002m and angle0; independently vary
mesh, curves, external field domain and this cut scope before claiming accuracy.
This smaller scope is a first integration candidate and no scaling solution.

### Higher-order source-resolving Galerkin preflight

Even the825-cell three-terminal integral candidate reached the supervisor120s
cap before P6 (`rs485-three-terminal-integral-v0`). Preserve this refusal. The
next sparse candidate enriches the magnetic **potential** space to conforming
Bernstein H1 P3/P4 over the same actual copper/dielectric tetrahedra. It retains
every original three-component constant-volume current coefficient. The old P1
potential has a nontrivial invisible-current kernel; higher degree must resolve
that kernel, not merely make its energy positive by a diagonal correction.

Use the scalar Bernstein basis B_alpha=(p!/alpha!)lambda^alpha with globally
shared vertex/edge/face coefficient keys. The exact constant-cell load is V/Np,
Np=choose(p+3,3). With d_i B_alpha=p B_(alpha-e_i) of degree p-1, assemble gradient
products analytically from integral(lambda^gamma)=6V product(gamma!)/(sum gamma+3)!.
This is a conforming Rayleigh–Ritz space, not a fitted R/L/C or softened source.
Keep the existing nine explicit whole-space solid-harmonic enrichment functions
and their exact exterior energies, including all finite-source integrals.
Primary basis/assembly source: [Ainsworth and Fu, 2018, sections2 and5](https://arxiv.org/html/1804.10466).
The implementation here uses only scalar H1 Bernstein functions; it does not
claim to implement that paper's H(curl)/H(div) elements.

Predeclare gates: exact partition/source moments and local stiffness against
independent positive polynomial quadrature; solve<=1e-8 row/1e-14 RHS-relative,
directed bilinear reciprocity1e-9, independent complete current-space energy
screen<1% including the unchanged frozen reference uncertainty, and degree/
mesh/domain changes<=0.2%. Frozen principal subsets are permitted only with an
explicit statement of their smaller geometry/current class; they do not become
a whole-board reference. P3 then P4 are distinct refinements. Count every global
polynomial coordinate and sparse fill before allocation under512MiB; no higher
cap, diagonal repair, removed current mode, fitted correction or hidden grounding.

First higher-order control results: exact Bernstein partition/source/stiffness
control passes. The saved3-tetra physical-current subset at P3 gives16.4893%
whitened discrepancy, so it fails the inherited1% combined gate. P4 correctly
refuses512MiB on that field mesh. Keep `bernstein-frozen-three-space-v0` and do
not infer board accuracy from higher degree or positive energy.
Next retain the identical3 original source tetrahedra and exact frozen reference,
but reduce only outer air-ring/z grading levels (distance ratio4, not2), to fit
P4 within the same cap. First source-neighbour distance remains0.000125m and
outer margin0.008m. This coarser air mesh is an explicitly separate candidate,
not an accepted refinement or an accuracy relaxation. Both degrees still use
the unchanged complete-space independent gate, and any success still needs
separate air mesh/domain and degree convergence checks.

Full P4 on the coarser air control fits176.4MB planned/55.7MiB observed but gives
2.99326% complete3-current-space error plus0.678064% empirical reference screen;
it still fails1%. P3 there gives16.8091%. These are explicit failed controls.
Next retain the conforming shared P3 trace space and add **only inside each Cu
tetrahedron** all positive Bernstein interior functions of degree4 then6:
alpha_i>=1, sum alpha=p. Their traces vanish on every face, so they are legitimate
independent local H1 enrichments without changing neighbouring material interfaces.
They leave all original constant Cartesian current coefficients intact. Use exact
mixed-degree gradient moments (denominator(p+q+1)!) and exact source moment
V/choose(p+3,3) per basis function. Shared P3 constants have zero bubble coefficients;
only the P3 subset participates in partition-of-unity cancellation. Compare against
the same frozen complete3-tet reference and the same1%+uncertainty gate. Preserve
all failed P1/P3/P4 and deadline evidence; no positive diagonal correction is fitted.

Mixed interior bubbles alone plateau: P3+bubble4/6/8 complete-space errors are
9.6724%,8.3273%,8.1829%; all fail1% including uncertainty. Preserve that packet.
The remaining error is in shared traces, not an absent interior diagonal. Next
use a conforming entity-based H1 basis: linear vertex functions; Bernstein edge
and face interior functions of degree3 in air, degree4/5 on every entity belonging
to a Cu tetrahedron (and on its complete incident star); Cu interior bubbles of
degree6/8. Shared entity degree is global, so every adjacent tetra uses exactly
the same trace. All interior functions vanish on faces. Linear vertex functions
alone form partition of unity. This geometric decomposition spans full Pp when
all entity/cell degrees agree, by a triangular basis change from Bernstein Pp.
Only the scalar potential approximation changes; the original current space,
source geometry and fixed independent acceptance gates remain unchanged. Use
mixed analytic moments and explicit global-coordinate/matrix/fill planning.

The first entity-only patch (raising traces without full incident-cell polynomial
closure) fails: trace4/body6 and trace5/body8 give6.85% and7.02%. The conforming
closure now promotes every tetra in the complete Cu-vertex star, gives each
shared edge/face the **minimum** degree of incident cells, and retains the
promoted cell interiors in air. This makes the global P3 subspace nested in the
higher-order patch. Its coarse-air errors are3.0812% (trace4/body6),1.5315%
(trace5/body6) and1.47095% (trace5/body8); all still fail the independent gate.
Keep all packets and do not equate monotone improvement with acceptance.
Next test only trace5/body8 on the original finer air grading ratio2 instead of4,
with unchanged first distance0.000125m, margin0.008m and identical3 physical source
tetrahedra/frozen reference. This is an air-mesh refinement, not a source or
reference change. The fixed1% including0.678064% empirical screen stays.

Development controls now use an optimized Cargo package override
`--release --config profile.release.package.pcb-analyze.opt-level=1` with one
build job; dependencies remain their ordinary release profile. This reduces
iteration compile cost without changing numerical gates/resources. Final addon
and integration checks must still use the ordinary native build. Each packet
records the actual emitted test binary hash and source/input snapshots.

The finer air mesh still refuses: trace5/body8 complete-space error1.23152%;
trace6/body8 gives0.901637%, but adding the same empirical reference screen
exceeds1%. Next expand the promoted potential region by one more complete vertex
star (two stars total), with entity degrees again the minimum over incident
cell orders. No source geometry/current/reference changes. Test trace6/body8
with the same fine grading and fixed gate. Cache polynomial derivative coefficients
once per cell; this is algebraic work elimination, not a numerical approximation.
For later actual-board trials, expose only an internal air-ring divisions control
(default4 unchanged, explicit1..16): it changes air triangulation without changing
any copper/contact constraint. Measure source-identity and air/angular convergence;
a coarser air grid alone is not an accuracy acceptance.

### Separate vertical electric-field mesh preflight

The magnetic current mesh must not be refined merely to resolve air/FR4 electric
fringing. Add a separate field mesh with identical ROI, prepared XY copper/contact
contours, stack/materials and bore geometry, but independently refined z planes
and outer air grid/domain. Lift each exposed fine-field metal node to the coarse
current potential on the identical XY vertical edge. Exact matching nodes use
one coefficient; a genuinely copper-supported vertical edge uses two positive
linear weights summing to one. Missing XY support or a dielectric gap refuses;
there is no nearest-node, net-name or ideal-junction substitute. Quotiented finite
electrodes retain their exact common potential. Interior metal potentials retain
zero charge unless their basis reaches the physical exposed trace. This is a
positive trace congruence of the existing capacitance operator; keep all original
free-residual/charge/work/energy guards. Independently compare the complete
conductor-constant C matrix to SciPy, then vary z resolution, outer domain and air
angular grid separately. The new path initially supports vertical refinement only;
new XY copper vertices require a separate supported trace and must refuse.

### First higher-order primitive admission and actual ROI refusal

`csc-star3-frozen-three-space-v0` passes the unchanged complete3-tetra screen:
whitened error0.003094651852859, empirical reference screen0.006780636440065,
sum0.009875288292924<0.01;6.464s/146.9MiB. The source geometry is the frozen first
finite prism and all its three independent scalar cell densities (hence all three
Cartesian blocks). It is **not** an actual-board or full24-cell admission.
`vertical-field-trace-controls-v0`:2 passed, preserving affine trace energy/charge
and rejecting an absent XY trace or a dielectric gap;0.442s/8.30MiB.

The real3-terminal825-cell ROI with independent field8mm/z0.2mm reaches the
100k polynomial-coordinate guard at75.4MB RSS (`csc-three-terminal-actual-v0`),
before magnetic/P6 completion. Physical mesh node/tetra guards stay unchanged.
Polynomial coefficient count is now a separately byte-planned H1 expansion,
rather than incorrectly consuming the spatial `max_nodes` guard. No byte/time cap
is raised. Before allocating the CSC coefficients, form DOF-to-cell incidences,
count each deduplicated sorted column with one bounded temporary buffer, then
allocate exact row/value arrays. This replaces the memory-heavy all-pair BTree
storage. AMD scratch is admitted explicitly; exact simplicial fill is counted
before symbolic factor storage, and unamalgamated supernode panels are bounded
by twice that fill. Numeric factor/solve scratch and action buffers are separately
admitted before allocation. No matrix coefficient, current mode or physical
coupling is removed. The ordinary P1 candidate remains unaccepted.

The supervisor now captures frozen JSON fixture hashes/snapshots and samples
the whole owned process family; a cap terminates its descendants as well. This
prepares honest DSL-worker resource measurement. Old packets retain their original
metrics. The public existing `a.port` has additive `positiveLayer`/`negativeLayer`
for a mixed-layer finite return; `layer` remains the shorthand and cannot be mixed
with those fields. The focused via-selector/layer compatibility test passes.

The actual P6/star3 ROI reaches sparse ordering admission:849,715,710 planned
bytes against457,492,161 remaining after live current/field/basis allocations
(`column-csc-three-terminal-actual-v0`,7.236s/351.0MiB). This is a real refusal.
A separately replayed P5/star3 control gives0.634581%+0.678064%=1.312645%, still
FAIL1%; its lower memory does not justify using it instead.
Next store exactly one symmetric triangle of K, expand both entries in the
residual/action, and call AMD directly on the known sorted upper pattern. This
removes duplicate symmetric storage and the unnecessary unsorted/full-matrix
ordering copy; it does not delete a mutual coupling or average directed kernel
values. Permuted upper fill counts, unamalgamated symbolic/panel bounds, numeric
scratch and all physical gates stay unchanged. Recheck the frozen P6/star3 screen
before the next actual ROI. Keep the original failed memories and P5 screen.

### Exact private-interior elimination ordering preflight

`upper-csc-star3-reference-v0` retains the strict primitive PASS and
`upper-csc-field-trace-v0` passes both trace controls. The actual ROI still
refuses before P6: `upper-csc-three-terminal-actual-v0`,5.552s/359.7MiB,
symbolic-pattern886,058,654 planned bytes against457,492,161 admitted. Keep this
refusal. Next change only the exact permutation: eliminate every cell-private
interior polynomial coordinate first, then AMD on the shared trace graph. A
private bubble vanishes on every cell face, so its elimination creates only
entries in that cell's already present trace clique. No basis, matrix value,
source moment, boundary condition or accuracy gate changes. Count exact fill
for this permutation before symbolic/numeric allocation, and rerun the identical
frozen primitive before the actual ROI. This is exact ordering, not dropping
interior modes or a current-only projection.

Private-first ordering preserves the primitive PASS (5.084s/130.2MiB) but still
refuses actual symbolic memory:125930 scalar coordinates,50805 private,
9609641 upper entries,72717322 exact simplicial fill;867476070>457492161 bytes
(`interior-first-three-terminal-actual-v0`,5.994s/337.4MiB). Next apply an exact
geometric separator permutation to shared traces. A coordinate's support box
contains every incident cell, not just its point: strict disjoint boxes on the
two sides of a plane guarantee no stiffness edge between those sides. All boxes
crossing/touching the plane remain in its separator, ordered after both children;
AMD orders leaves/separators. No entries are omitted. Evaluate three median
planes, reject unbalanced cuts, admit workspace and count fill before allocation.
Recheck the same primitive. This is a permutation only, not a new field model.

### Independent field refinements: rejected coarse field, graded-z preflight

The separately frozen real3-terminal field has3 physical conductor components;
SciPy independently assembles all27189 dielectric tetrahedra and solves3782 free
field nodes. Charge/positive-energy discrepancy1.4077e-11 and free residual
1.4124e-12 pass (`scipy-field-three-terminal-v0`). This validates the discrete
assembly only. Relative Frobenius C changes against individual refinements:
z0.2->0.1mm18.7919%, domain8->16mm0.9509%, air subdivisions4->8 1.7723%.
All exceed the predeclared0.2% convergence gate. No C acceptance follows.
Next add optional geometric z grading from metal faces into dielectric and
external air, preserving every original material interface and uniform maximum
step. This is conforming P1 refinement, with the already tested vertical trace;
Cu current mesh, exact contact sets, source IDs and materials remain fixed.
Use interface steps8,4,2um to screen z convergence before combined domain/air
refinement. Preserve all failed coarse results and the same0.2% gate.

Geometric ordering also preserves the frozen primitive (4.291s/182.5MiB), but
worsens actual fill139850565 and refuses1404542014 planned bytes
(`geometric-order-three-terminal-actual-v0`,5.427s/343.2MiB). It is not a memory
solution. The independent electric z grading succeeds:8->4um C change0.033165%,
4->2um0.024487%, both below0.2%; air/domain and bulk-z still require checks.

### Full sparse potential iterative solve preflight

The exact Galerkin matrix fits; its direct Cholesky fill does not. Next test
preconditioned conjugate gradients on the *complete unchanged* symmetric K,
using SSOR(omega1) as a positive definite algebraic preconditioner. No incomplete
physical matrix, fitted correction, shifted pivot or dropped current/source mode
is used. SSOR changes only iteration: (D+L)^-T D (D+L)^-1. Positive original
diagonals and CG curvature are mandatory, otherwise refuse. Existing compensated
full-block residual/reciprocity/source-space gates still decide admission. Stop
at2000 iterations or110s operator lifetime, with the same supervised120s/512MiB
packet cap. Test the frozen complete3-source matrix through this solver before
using it for actual ROI. Primary algorithm/preconditioning source:
[Netlib Templates, sections2.3 and3.4](https://netlib.org/templates/templates.html).
An iterative failure remains a refusal, not an excuse to weaken1e-14 residual.

SSOR full-matrix CG passes the same frozen3 reference:0.309465%+0.678064%=0.987529%
(31.797s/45.2MiB,`ssor-cg-star3-reference-v0`). Actual ROI memory is now bounded
(226.7MiB), but it refuses at110s CG operator lifetime/112.656s whole packet
before P6 (`ssor-cg-three-terminal-actual-v0`). Keep that failure. Remove the
inferior geometric ordering from the live implementation; its complete frozen
sources and evidence remain in its packets. Exact private-first AMD stays.
Next add positive *unmodified principal entity-block* inverses to SSOR as an
additive preconditioner (vertices/edges/faces/cell interiors retain their complete
physical K). Each block is SPD or explicitly refuses; no shift/regularization.
Only the algebraic preconditioner changes. Admit max35-coordinate block storage
before allocation and retest the same frozen3-space with all residual/reciprocity
gates. Netlib Templates section3.3 covers block Jacobi/additive local solvers.

Additional electric checks retain failures: graded2um bulk0.2->0.1mm0.6331%,
0.1->0.05mm0.3973%,0.05->0.025mm0.2952%; no bulk-z acceptance yet. Domain changes
8->16/16->32/32->64/64->128mm are1.2567/0.6274/0.3138/0.1570%; last passes0.2%
for its fixed air4/z profile only. Angular4->8/8->16 changes1.5915/0.5570%, still
FAIL0.2%. Independent SciPy air8 COLAMD hit512MiB; switching only its symmetric
ordering to MMD_AT_PLUS_A preserves the full discrete matrix and strict guards,
then passes at255.5MiB. Air16 passes under459.1MiB. These are discrete independent
C checks, not whole-board field or continuum admissions.

Uniform bulk12.5um refuses the unchanged spatial extrusion guard before allocation
(`field-grade2-bulk00125-geometry-three-terminal-v0`). The25um case has23532 field
nodes/118758 dielectric+15222 copper tetrahedra and still0.2952% change. A dependent
reference invocation on the refused empty geometry also refuses JSON parsing;
it is not a physical result. Next vary only the dielectric geometric growth
ratio2->1.5->1.25->1.125 at fixed2um first spacing, preserving all original layer
planes and maximum step. Keep external air z growth2 separately, so refining
FR4 does not uniformly multiply far-air slabs. Same field trace and strict
charge/energy/residual/convergence0.2% gates, same spatial512MiB/200000-tet caps.

Entity-block+SSOR passes unchanged primitive accuracy but takes76.476s versus
31.797s for SSOR alone (`entity-block-ssor-star3-reference-v0`); it is a performance
regression, not a viable actual-ROI speedup. Keep its frozen sources, remove that
preconditioner candidate from live code. Next SSOR plus an exact coarse P1
*principal-subspace correction*: the linear vertex functions already belong to
the unchanged high-order space. Factor only that much smaller principal K block,
add its positive inverse through vertex injection to SSOR, and run CG on full K.
No reduction of the solved potential/current space occurs. Admit coarse sparse
fill/numeric workspace explicitly in addition to all live full-matrix/action
storage. Require the identical independent full3-space and strict residual gates.

Coarse-P1 additive SSOR retains primitive accuracy but50.631s is still slower than
31.797s scalar SSOR (`coarse-p1-ssor-star3-reference-v0`); no actual speed claim.
Next replace only its preconditioner with exact block SSOR. The existing global
DOF assignment gives each cell's newly introduced coefficients a contiguous,
nonoverlapping range. Factor each *full K principal diagonal block* exactly,
then perform block forward/back substitution with every off-block coefficient
of K. This realizes (D+L)^-T D (D+L)^-1 with SPD block D; full target K is
unchanged and every source/current remains. Reject a nonpositive local pivot,
never shift or regularize. Count sum(block_size^2) before allocating factors;
retain the same packet caps and residual/independent3-space gates.

The frozen independent field results now have a reproducible visualization:
`script:plot-field-convergence.py actual-field-convergence-plot-v1 --release
actual-field-convergence-figures-v1` (supervised6.51s/110.2MiB for v0; corrected
axis/contact layout v1 2.955s/109.8MiB). v1 was visually inspected. It displays
actual complete signal barrel, finite TOP return and retained floating BOTTOM
foil, plus successful/failed *isolated* C changes. It explicitly labels the field
unaccepted, and makes no P6/DSL claim. The earlier45-degree current-plot projection
is corrected in its source to the actual finite-contact separation; old figures
remain frozen and a new accepted P6 plot remains pending.

![Actual ROI and unresolved independent field convergence](D:/MyProject/NN/projects/Agents/easyeda-copilot/eda-copilot-backend/debugging/p7-board-integration/actual-field-convergence-figures-v1/actual-roi-field-convergence.png)

Dielectric growth2->1.5/1.5->1.25/1.25->1.125 changes0.6249/0.6538/0.2267%;
the last is close but still FAIL0.2%. Do not round it into an acceptance.

`cell-block-ssor-star3-reference-v0` passes the unchanged primitive,29.341s/50.2MiB;
actual same825-cell ROI still refuses at112.593s/273.5MiB, before P6. It has3146
principal blocks/5989852 factor values (48,120,160 retained factor bytes). These
are valid resource failures, not a claimed end-to-end result.

Before another discretization change, compile the identical source at ordinary
release optimization (no package opt-level1 override), then replay primitive and
actual packet under the same120s/512MiB caps. Earlier rapid iteration packets used
release with package pcb-analyze opt-level1, as their build commands state; do not
transfer those times to the ordinary native build. No physical or accuracy change.
If the ordinary optimized path still refuses, investigate tensor-prism H1 on the
already exact XY×Z extrusion. Magnetic permeability is the unchanged uniform mu0:
K=Kxy⊗Mz+Mxy⊗Kz. Preserve every original tetra-current source/load (no uniform
current over a whole prism), foil/barrel geometry, and the nine existing exterior
functions. Generalized z diagonalization gives shifted sparse2D problems; this
can address extrusion anisotropy without deleting modes or changing a distance.
A new tensor approximation must pass the same independent complete3-tet reference,
source moments, symmetry/residual and <=0.2% degree/quadrature/domain gates before
any actual/P6 admission. This is a preflight candidate, not implemented or accepted.

Primary tensor-solver context read:
[Zank2025, section3, equations4–7](https://arxiv.org/html/2504.07269v1#S3).
That paper treats space/time Schrödinger systems; no accuracy, timing or nonuniform
mesh claim from it is transferred here. Our prospective adaptation is a real
uniform-mu0 Poisson energy: with Kz,Mz SPD on the exact nonuniform Dirichlet z
mesh, Mz=L Lᵀ and the symmetric L⁻¹KzL⁻ᵀ has a complete real positive eigenbasis.
U=L⁻ᵀV gives UᵀMzU=I and UᵀKzU=diag(lambda); each shifted2D Kxy+lambda Mxy is
SPD. Refinement must use the original full tensor K residual, not merely the
transformed mode residual. Source loads integrate each original tetra separately.
A tensor candidate is not the same approximation space as the existing tetra
candidate, so its independent admission must be repeated rather than inferred.

Ordinary release also refuses actual ROI at112.399s/273.6MiB; source primitive
still passes (30.551s). Optimization level is not the scaling solution.

First tensor-prism implementation retains the same physical tetra loads and
nine exterior functions. `tensor-prism-frozen-three-space-v0`: P6xZ6 source-space
error1.10213%; P8xZ8 error0.440616%+0.678064%=1.118680% FAIL1%, coupled-degree
change0.664360% FAIL0.2%. Preserve that explicit failure. Construction is1.714s
for88733 coefficients/66591888 planned bytes and5.228s for211463/176604288;
whole packet14.856s/157.2MiB. No actual/P6 acceptance follows.
Next use a declared directional refinement cross: XY8/Z10, XY8/Z12, XY10/Z12.
The tensor degrees are independent controls. Compare the two Z refinements at
fixed XY8 and the XY refinement at fixed Z12, both with the unchanged0.2%
whitened full3-source energy change gate. Require XY8/Z12 and XY10/Z12 independently
under1% including the same inherited reference screen. This is not a new reference
or a tolerance change. The largest cross is admitted before allocation; no cap
is raised. Source polynomial quadrature raises with total polynomial degree;
external quadrature remains a separate8/16 refinement. Stream the nine K-image
residual checks one at a time so their temporary full-field sums fit the planned
workspace. All final full-block1e-14 RHS/1e-8 row residuals and1e-9 reciprocity stay.

Directional uniform-Z cross also fails (`tensor-directional-frozen-three-space-v0`,
119.756s/319.3MiB):8x10 error0.360335%;8x12 error0.358823%+0.678064%=1.036887%,
still FAIL1%. Uniform10x12 refuses exterior Schur SPD; no repair is applied.
Its461538848-byte plan fits the control budget, but the result is not admitted.
Next use a conforming variable-degree Z space: keep target degree on every source
slab plus its three neighbouring slabs in each direction; far Z intervals use
P3. Endpoint coefficients are shared exactly, and XY×Z separability remains.
This mirrors the earlier declared complete-star near/far H1 approach; it is a
new trial space, not an adjustment of its energy or the reference. Source slabs
retain their original tetra moments. Predeclare a common far-P3 cross8x10,
10x10,10x12; require both final10-degree cases under1% including unchanged
reference screen, and separate XY/Z changes<=0.2%. Far degree/quadrature/domain
remain separate checks; no actual board/P6 admission from this control alone.

After a tensor primitive passes, actual825-tetra accuracy still needs independent
whole-source evidence. Plan a separate immutable analytic tetra-pair reference
using the existing PointPotential/tetra_self/adaptive directed Newton integrals,
not another tensor/FEM solve. Reuse successful self integrals and pair entries by
exact vertex-bit/source geometry identity; split only new entries into bounded
packets rather than repeating an already successful expensive reference. Keep
1e-10 self refinement,0.2% directed quadrature refinement and1e-9 self-normalized
reciprocity; never average failing directed pairs. Assemble the complete scalar
source matrix and compare all825 independent cell densities after diagonal
scaling/Cholesky whitening; Cartesian blocks are identical and cross components
vanish analytically. Independently record reference quadrature-change uncertainty
before the1% combined full-source gate. The existing frozen3/24 references remain
unchanged and do not by themselves admit the real825-cell ROI. This offline
validation must not be inserted as a manually authored runtime RLC replacement.

Far-Z P3 alone does not resolve exterior quadrature:10x10 still refuses Schur
SPD (`tensor-far-p3-frozen-three-space-v0`,19.148s/177.7MiB);8x10 error remains
0.360399%+0.678064%>1%. No acceptance. The nine smooth whole-space functions
change second derivatives at their source sphere. Next move that trial sphere
from1.05 to2.0 times the retained copper bounding radius, keeping it inside the
current finite box; this moves its interface into the coarser far space. This
changes only the variational enrichment, not copper/R/L values, Gram formula,
reference, tolerances or any energy correction. It needs the same independent
source, directional-degree and8/16 quadrature checks. First probe10x10 alone to
avoid repeating unsuccessful lower-degree controls; if it passes, replay the
full directional cross for this changed trial family. Schur non-SPD still refuses.

`tensor-radius2-q16-target-frozen-three-space-v0` passes the complete3-source
independent gate:0.227337%+0.678064%=0.905400%<1%;24.926s/215.3MiB,
258633 coefficients/247296336 planned bytes, construction15.628s. Radius2/order8
still refused Schur SPD (8.429s). Thus8-point exterior quadrature is insufficient;
keep that failure. Next compare16 to24 at the identical target; degree cross also
uses16. Every packet remains<=120s/512MiB. This is a primitive, not825-cell or P6
admission. No expensive frozen QMC reference was regenerated.

### Coherent finished-hole/barrel curve preflight

The prepared drill is deliberately circumscribed (including the accepted boolean
conversion guard), while the previous independently sampled outer barrel circle
was inscribed and used a different angular count. Their errors subtract in the
small25um annulus; this explains why a1um surface tolerance can still produce a
several-percent DC error. Do not shrink the native bore or weaken area/contact
or curve gates. Next derive outer vertices by adding the nominal plating radius
to each prepared round-bore vertex along its actual radial direction. Preserve
all bore vertices/edges and source IDs. Explicitly verify the entire outer polygon
(radial vertex maximum and segment radial minimum) stays within the *same*
nominal outer-circle tolerance plus the existing drill conversion guard. This
is a coherent polygonal approximation of the same nominal circular wall, not a
manual R or an enlarged plating parameter. Its normal thickness varies by the
polygon chord error and converges to the authored25um. Test polygon/circular
annular area, three actual curve refinements, independent DC/KCL/power, all
geometry area/topology/contact gates and source volume mapping. Old successful
references stay frozen for their old geometry; changed-geometry checks are new.

### Full electric Schur materialization preflight

The coherent real ROI has795 copper tetrahedra/172 potentials. Independent DC is
1.053726375038mOhm inside the circular1.034406563..1.081732353mOhm interval;
KCL5.408e-12 and power5.587e-13 pass (`coherent-barrel-three-terminal-dc-v0`).
`coherent-tensor10-three-terminal-actual-v0` reaches nx10281/432446 XY entries/
559140 exact2D fill then refuses total modal storage (9.942s/97.2MiB), while the
73,497,647-byte field factor remains live. Preserve this refusal.
Next materialize *all exposed current-potential* capacitance coordinates from the
same FEM, with no voltage/current projection: solve every unit trace, retain its
nodal potential column, and accumulate the positive dielectric energy Gram in
bounded chunks. Compare each complete charge column to the energy Gram under the
existing1e-8 guard and directed reciprocity1e-9. Retain buried-potential zero
charge rows and all component/infinity charge coordinates. Drop the large field
factor before magnetic construction. Plan the field/transfer/chunk peak and the
subsequent retained dense C/geometry separately under512MiB. A large surface still
refuses its planned dense storage; no spatial or dense-cap limit is raised.

Audit tensor workspace by actual lifetimes: streamed image checks use one RHS;
physical block solves use at most3 Cartesian RHS. The largest simultaneous
state/residual/transform phase is bounded by384 bytes/product coordinate. Admit
416 bytes plus explicit faer column padding, source/mesh/XY headers, dense-z
workspace and all shifted factors before allocation. Reject RHS counts>3. This
replaces the earlier512-byte blanket overestimate with a documented phase bound,
not a higher budget. CSC term and adjacency capacities are now exact. All old
refusals remain as observed; verify both planned and sampled process-family peak.


### Full C result and bounded complete-source column preflight

`full-c-schur-controls-v0` passes all3 focused electric controls, including every
potential and complete energy Gram (2.476s/9.25MiB). On the coherent795-Cu/172-V
actual ROI, `full-c-tensor10-three-terminal-actual-v0` materializes C with
91,952,559-byte assembly peak and7,472,760-byte retained plan; charge/energy
column error1.25976855e-14 and directed reciprocity2.73644145e-14 pass. Tensor
10x10/order16 then constructs successfully in53.079s with487,628,442-byte plan.
The whole packet still reaches120.239s/482,590,720-byte observed peak before P6;
it is a preserved deadline refusal, not an accepted extraction.

Next skip only exactly zero Cartesian RHS blocks in this uniform-mu scalar
operator; compare compact1/2-axis and simultaneous3-axis results against the same
complete independent-cell matrix. No cell or nonzero current mode is removed.
Measure complete source columns on the real geometry in immutable bounded packets.
A future geometry-bound scalar matrix cache must retain every795 cell density
(and all2385 Cartesian currents), validate complete geometry/normalization bits,
all directed reciprocity1e-9 and SPD, and record cold preparation separately from
warm P6/DSL. It cannot be a manually supplied equivalent circuit or bypass the
whole-source independent accuracy/field convergence gates. Completed column
packets can be reused by exact identity; do not repeat successful expensive runs.


### Bore-field and near/far air refinement preflight

The old planar bore triangulation has only wall-bound vertices; a P1 field on
that XY grid cannot resolve a free aperture/bore potential. Existing isolated
z/domain controls therefore do not establish physical C convergence. Add optional
strictly interior bore-air centre/concentric rings (0..4), and separate near/far
rectangular air-ring divisions with a declared first/near distance. Preserve all
native bore/Cu/contact constraints and original current mesh; only field air nodes
are inserted. Refuse unknown field Cu XY nodes through the existing trace gate.
Defaults preserve previous requests. Retain positive-volume/material/contact
checks and the100000-node/200000-tetra/512MiB limits. Compare centre/one/two bore
rings, near angular8/16 and far domains independently on the coherent geometry;
no capacitance or field-accuracy admission from earlier wall-only grids.


`tensor-radius2-q16-directional-active-axes-v0` passes48.220s/241.9MiB:
XY8->10 whitened change0.130991%, Z10->12 change0.0341705% (both<=0.2%);
10x10 error0.227337% and10x12 error0.193467%, each plus frozen0.678064%
reference uncertainty<1%. Compact one/two-axis actions match simultaneous3-axis
mixed cell loads within1e-12. Frozen quadrature16/24 matrices give a separate
quadrature control; source-space primitive admission still does not admit795
actual cells. The test binary is the completed active-axis build; source snapshot
also contains later uncompiled, unrelated field/cache additions. Its binary hash
and the preceding columns packet identify the exact exercised implementation.

`tensor10-source-columns-0000-v0` retains columns0..2 (23.841s/375.2MiB),
`tensor10-source-columns-0003-v0` retains3..52 (86.984s/375.2MiB). Each column
uses the original795 independent source cells and1e-14/1e-8 magnetic solve guards.
No complete-matrix or physical accuracy claim yet. `cargo check --all-targets`
passes after replacing historical copied-module coupon imports with actual crate
imports (29.84s; existing fixture dead-code warnings). No native addon build yet.


Independent full172-potential SciPy traces pass charge/energy/reciprocity on
coherent bore0/1/2 grids (128061/152589/177117 dielectric tets;8.317/8.156/10.989s;
268.8/314.3/407.0MiB). However full-space bore0->1 and1->2 whitened C changes are
552.125% and215.011% (worst relative energies92.5772% and39.2257%): clear FAIL,
not field convergence. Resolve conductor-constant and physical-solution changes
separately; no arbitrary full-C accuracy claim follows from component controls.

`coherent-full-source-reference-pairs0000/0020/1020-v0` calculate20/1000/10000
analytic pairs within their caps. Exact identity comparison rejects these as
actual-geometry references:552 coordinate-bit occurrences changed on Rust decimal
JSON decoding. Python's correctly rounded decoding matches all original native
source bits; the prototype Rust reference does not. Preserve those packets as
transport diagnostics, not actual accuracy admission. Require exact u64 node bits
from the source-column header, reconstruct withf64::from_bits, then calculate new
`coherent-exact-source-reference-pairs*` packets. This is a spoiled geometry
measurement, not a reason to repeat any unchanged expensive frozen reference.


### Actual-source rejection and source-preserving graded potential preflight

`coherent-source-subset148-independent-witness-v0` rejects tensor10x10 on148
original actual cells: whitened Frobenius113.808877% plus independent reference
change0.323436%=114.132314%>1%. Minimum relative energy0.65537954; maximum
0.99809898; largest self-energy error16.7864%. All303 prepared candidate columns
remain evidence; stop that candidate's queue and do not generate the other492.
The primitive did not expose thin-foil boundary layers in millimetre Z slabs.
The streamed exact analytic reference retains successful pairs through the
`coherent-exact-source-reference-pairs11026-v0` refusal at268/126 (reciprocity
2.154961e-9>1e-9 at64). Preserve its prior successes, require the existing adaptive
positive-target fallback before accepting that pair; no averaging or loosened gate.

Next grade only the magnetic potential Z axis at8/4/2um dyadic distances from
original foil/slab interfaces. Keep original795 physical tetra/current densities,
source IDs, contacts and R exactly. Integrate each original tetra load on the
new potential slabs by exact-degree positive cross-section quadrature: one/two
triangles per horizontal slice of the original canonical prism split. Check
positive weights, each original tetra constant/linear moments, and unchanged
ungraded primitive matrices. Compare the saved independent worst current first,
then complete-source accuracy; no probe alone is admission. Keep256 Z-coordinate
and all existing physical geometry/memory/time limits. Serialize Cartesian RHS
blocks (all nonzero original components retained) and admit256 bytes/product
coordinate: persistent nine coupling+nine image columns144 bytes, one-RHS solve/
DD refinement phases<=96 bytes plus16 guard. Enforce count<=1 in the private
potential solver. This is a lower simultaneous workspace, not a higher budget.
Bore component-constant C changes0->1/1->2 are0.0697332%/0.0284195%; full172-space
C remains unconverged, explicitly separate from these three component controls.


Initial graded controls remain rejected: `graded-original-tetra-source-control-v0`
passes exact original-load moments but XY6/Z6 still gives0.864044%+0.678064%>1%;
5.810s/61.0MiB. `graded6x4-z8um-actual-witness-v0` admits300135162 bytes but
refuses image RHS residual4.93877e-10>1e-14 after8 corrections (13.604s/290.3MiB).
Keep both. Subsequent unmeasured controls use the same exact H1 moment machinery
with shared XY edge degree8 on source-Cu edges, Cu interior8, other edges/interiors3.
Inactive basis functions receive no DOF; promoted edge degree is shared across
both cells, preserving conformity and every original current. This is the2D
counterpart of the saved mixed entity spaces, reducing far potential storage.
Grade only interfaces adjacent to original Cu slabs; retain existing slabs<=64um
(the polynomial potential already resolves their thickness), and test8/4/2um
external/bulk refinements independently. Extend internal DD correction attempts
from8 to24 under the unchanged110s lifetime and1e-14/1e-8 gates, recording residual
progress. No matrix repair or automatic admission. Test XY8 on the frozen complete
primitive before actual witness/full-source checks. Order8 negative test now
asserts its known explicit Schur refusal, rather than expecting acceptance.


Source-only XY8 promotion fails the primitive9.55580%+0.678064% and the actual
worst148-source witness32.67297% relative energy (`mixed-graded8*` packets).
It does fit actual full-source memory292777650bytes and strict image solves now
converge (15.871s/281.0MiB); this is a numerical solve, not accuracy admission.
Expose0..3 complete planar vertex stars so the same higher-order basis extends
into adjacent air. Default graded control uses2stars. Compare1/2/3stars and Z8/4um
without changing source cells or any guard; failed source-only results remain.


`coherent-exact-source-reference-pairs036172-v1` uses the unchanged adaptive
fallback and retains successful pairs36172..36707; refuses4096 adaptive leaves
at next pair270/123,101.177s/28.4MiB. The source point potential's existing1e-10
roundoff/source-rule budget can be larger than the1e-11 self-normalized target
refinement floor. Add an opt-in *tighter* tetra point-potential budget1e-12
(default unchanged), use its existing positive line/source rule when analytic
cancellation cannot support that budget, and retry only the failed new pair.
Keep1e-10 self,1e-11 adaptive target,1e-9 reciprocity and0.2% pair-change gates.
No accepted pair is recomputed; failed pair attempts and their geometry remain.

New full-trace field checks: near8->16 stillFAIL0.631491% on independent4-mode
DC harmonic space (component0.607920%, full172-space8.258620%). Bore1->2 passes
harmonic0.0843484% and component0.0284195%, but full172 remainsFAIL215.011%.
Harmonic traces use independently assembled conductivity, residual8.11215e-16.
These physical subspace controls do not admit arbitrary full-C accuracy or a P6
result. Finer near-air resolution is still needed under existing limits.


Two XY vertex stars still fail the primitive1.50930%+0.678064% and actual witness
30.39998% (`mixed-star2-graded8*`). Actual strict solve passes24.036s/444.1MiB,
473340458 planned bytes. This is not accuracy admission. Add an exact memory/time
tradeoff: retain one shifted2D numeric factor, regenerate other positive shifted
factors from unchanged Kxy+lambda* Mxy during each solve, reusing the same symbolic
factor and explicitly admitted numeric scratch/CSC clone. Keep all product DOFs,
F64 coefficients and strict full-K residual gates. No disk/mmap hidden memory.
Compare resident versus regenerated factors on the identical ungraded space.
This permits a higher uniform/near XY and Z trial within512MiB; it may cost more
CPU and still must refuse110/120s. Probe the saved actual witness before any new
complete-column preparation. Previous cached candidate/refinement failures stay.


Tightening point-source precision alone does not resolve270/123: the single-pair
`coherent-exact-source-reference-pairs036708-tight-v2` again exhausts4096 target
leaves,14.290s/28.2MiB. Preserve refusal; no successful pair repeated. Next add an
explicitly budgeted reference-only target-work limit16384 (<=200000 tetra bound),
preflight256 bytes/leaf before allocation, keep production default4096 and all
1e-11/1e-9/0.2% accuracy gates unchanged. Log global estimated target error and
requested tolerance at exhaustion. Same110s internal/120s supervisor/512MiB cap;
more work may still refuse rather than weakening a physical/numerical tolerance.

Independent DC-harmonic field controls: growth1.125->1.1 passes0.130400%, whereas
domain64->128mm fails0.322300%. New256mm field-domain control is required; earlier
wall-only/old-geometry domain success is not transferable to this resolved bore.


For further electric angular convergence add32 near-edge divisions as an
additional internal mesh-resolution setting; the16 setting blocked refinement.
This changes no0.2% accuracy tolerance or100000-node/200000-tet/512MiB/120s limit:
all exact ring/extrusion counts and allocations remain admitted before use.
Predeclare common profiles at2um first Z spacing, growth1.18/1.2, no redundant
uniform bulk cuts (maxStep2mm), domain128/256mm, one near ring at11um, and bore0/1.
Compare16->32 angular,1.2->1.18 Z,128->256 domain and0->1 bore separately on common
profiles; preserve old finer-growth/guard refusals and full172-space failures.
No finer profile is accepted merely because it fits its spatial count.


`regenerated-modal-factor-equivalence-v0` passes the identical complete operator
against resident factors9.801s/54.5MiB. Uniform graded XY8/Z6 primitive stillFAIL
0.356863%+0.678064% (13.401s/63.6MiB). Uniform XY10/Z8/32um actual witness admits
523584666bytes but times out111.784s/456.8MiB during first image: initial RHS0.374565,
after8 corrections5.05324e-14, still above1e-14. No accepted accuracy result.

Next replace repeated shifted numeric factorization by an owned temporary binary
factor file. Factor every exact positive shifted2D matrix once, preserve all F64
bits, retain only one factor/read buffer in process memory. Verify magic/dimensions/
length before every solver reader; strict full-K residuals remain authoritative.
Use create_new in OS temporary directory, no mmap, no public matrix/file DSL,
no existing user file overwritten. RAII removes the owned file; supervised hard
termination cleanup verifies exact temp parent and child-PID filename prefix
before unlinking only that owned file. Admit buffers explicitly, record file bytes
and cold/warm cost; keep512MiB/110/120s. This is additional internal cached solver
storage, no port/current reduction or changed physical kernel.


Prepare an independent actual-nine-source diagnostic at fixed indices108,117,
124,125,126,127,132,133,270 (dominant saved witness plus one barrel). Use the saved
5D scrambled-Sobol conditional longest-source-segment method with fixed seeds
1307/2719/4513/6337 and powers14/16/18 on exact native vertex bits. It imports no
production source/integral code. This is new actual geometry, not a rerun of the
successful frozen24-source reference. Empirical seed/power uncertainty stays in
its gate; it may diagnose source/refinement errors but cannot admit795 sources.


Temporary-factor equivalence passes7.938s/54.5MiB with owned file cleanup.
Uniform XY10/gradedZ6 complete primitive passes0.195252%+0.678064%=0.873316%
(`uniform-spilled-graded10-original-tetra-control-v0`,16.622s/90.6MiB).
Actual XY8/Z8/8um still times out112.093s/350.8MiB while refining9 images;
its472861736-byte temporary factor file is removed. Preserve refusal.

Replace stationary DD inverse refinement with compensated preconditioned CG on
unchanged positive K and the full enriched block. Keep exact recomputed full
residuals,1e-14 RHS/1e-8 row gates, positive curvature/inner-product refusal,
24 corrections and110/120s caps. Use the exact modal positive inverse as K
preconditioner. For enriched preconditioning use the symmetric block factor
[I,-Y;0,I] diag(P_K,S^-1) [I,0;-Y^T,I], with the same admitted Y and positive S;
Y^T load replaces C^T P_K load only in the *preconditioner*. Original K/C/G
remain the full residual operator. No potential/current/accuracy projection.
Admit304 bytes/product coordinate for CG direction/action and guards, retaining
all existing512MiB limits. Recheck frozen operator/linearity/independent primitive
before actual witness; a solver change cannot admit a failed spatial candidate.


Prepare a separate *positive modal source-matrix candidate*, not an exact PDE
solve claim: materialize all independent cell columns of the fixed positive block
preconditioner P. Its scalar kernel is MU0 times sum_m B_m^T A_m^-1 B_m plus
(E-Y^T B)^T S^-1(E-Y^T B). This is a geometry-derived full-current approximate
Newton matrix, not a manually authored RLC or a reduced current space. Stream
one shifted2D factor and full source RHS panel per mode; budget the actual
post-constructor retained state, two nx-by-source panels, faer multi-RHS scratch,
full directed source matrix, DD summation and SPD validation before allocation.
Do not weaken exact Operator.apply's1e-14/1e-8 PDE checks. The new approximate
kernel is admissible only after complete independent whole-source energy error
plus reference uncertainty<=1%, directed reciprocity1e-9, SPD/no clipping,
separate spatial/quadrature/domain convergence and comparison to strict PDE
controls. First materialize the declared148-source diagnostic subspace; it is
never795-space admission. This avoids795 repeated3D solves in each trial.


`actual-nine-source-independent-qmc-v0` passes its new actual8-source analytic
cross-check:0.0862675%+empirical0.349248%=0.435515%<1%; all nine self discrepancies
<=0.020104% (17.706s/155.6MiB). The ninth barrel's remaining cross pairs are not
available, so no complete9-space or795-space claim. This independent diagnostic
supports the analytic actual reference and confirms the much larger tensor
candidate losses are spatial approximation failures, not a nominal-unit factor.

[Actual-source rejection diagnostics](D:/MyProject/NN/projects/Agents/easyeda-copilot/eda-copilot-backend/debugging/p7-board-integration/actual-source-diagnostic-figure-v0-output/actual-source-rejections.png)
shows retained physical Cu, saved worst-current energy losses and full versus
physical-subspace field changes; explicitly unaccepted, not P6/DSL evidence.


### Complete ordinary signal/return ROI scope control

The saved worst source sits next to the intentionally tight three-terminal cut;
its dominant cells include a242.56-by4.60um planar span. Do not filter that copper
or claim the three-terminal foil return is the ordinary two-via channel. Prepare
an expanded real two-via ROI[22.7233,3.7219]..[23.9733,4.1719]mm: signal0 and
return65, all four original finite TOP/BOTTOM contacts, same1um curve bound,
25um plating/58MSm/35um foil/1.53mm FR4/epsilon4.2 and insulated cuts. This leaves
at least50um nominal clearance around both outer barrels instead of a near-tangent
cut. It retains all intersecting native Cu, source ownership and bore geometry;
no geometric feature is removed or manual current loop authored. First preflight
geometry/contact/DC/independent annular bounds; then whole-current operator and
P6/DSL checks. Success on the expanded ROI would not close original tight ROI
or large-board scaling; all prior failures and the three-terminal control remain.


CG algebra controls pass, but benign primitive construction regresses to107.784s
versus16.622s. Keep the measured regression. Select stationary DD refinement when
its initial full residual<=1e-6; otherwise use the compensated CG path. This is
only preconditioner/iteration selection: final1e-14/1e-8,24-step and time gates
are unchanged. Positive modal materialization control passes all original cells,
SPD equivalence and negative memory/count cases (29.573s before this policy).
For the larger complete ordinary ROI replace dense all-source RHS panels by64
source columns and sparse original-cell modal projections. Retain the full n²
source matrix; preflight faer64-RHS scratch, all sparse projections and directed
matrix/SPD work. Source/current counts are not reduced.

The expanded ordinary ROI passes native geometry/contact gates:1656 Cu tets,
4968 currents/337 potentials, two actual barrels and all four finite contacts;
Cu area4.91658e-11/contact1.29369e-9 (both<1e-8). Independent DC passes13.927s/
65.2MiB (`coherent-clearance50-two-via-dc-v0`). This is geometry/DC only, not AC,
whole-source magnetic accuracy or direct DSL acceptance.


The16384-leaf failed-pair packet exceeds its supervisor wall cap123.577s and is
refused; preserve it, do not report it under120s. More bisection is not a solution.
Predeclare a positive source-preserving layered-target transform instead: slice
an original two-Z-level tetrahedron horizontally into its exact triangle/quad;
triangulate the convex section, apply triangle Duffy coordinates and split the
height into t=u^4/2 and t=1-u^4/2, each dt=2u^3du. Integrate the original analytic
source potential with positive weights; check recovered original target volume
and all three first moments before accepting a value. Refine8/16/32/64, retain
0.2% directed refinement and1e-9 self-normalized reciprocity, same110/120s limits.
No source/cell/geometry change. Primary context:[Duffy1982](https://epubs.siam.org/doi/10.1137/0719090)
(abstract accessible; full text requires access). The actual slice/Jacobian mapping
above is independently derived and checked here, not claimed as copied from the
unavailable full paper. Retry only270/123, then reuse successful earlier pairs.


Positive layered target controls pass three original prism-split shapes through
order16 with volume/first moments and unchanged1e-9 reciprocity (12.845s).
Actual270/123 still reaches110s/112.332s supervisor while source evaluation is
expensive; preserve deadline. Add byte-identical cached Gauss rules and a bounded
positive tetra *far-point* cubature before the slow positive source fallback.
For source bounding-ball ratio q<1, positive10³ Duffy/Gauss cubature is exact for
polynomials through17. The Newton Legendre remainder gives relative true-minus-
cubature bound<=2(1+q)q^18/(1-q), plus measured weight mass error and conservative
1024eps(1+q)/(1-q)^2 guard. Accept only if this total meets the original source
budget (or opt-in tighter1e-12); otherwise keep the old analytic/positive path.
No per-pair averaging or weakened target/reciprocity criterion. Lazily retain
1000 positive source samples per touched tetra; increase the integral operator's
preflight source storage allowance from16KiB to64KiB/cell plus256KiB fixed cache
allowance, without raising512MiB. The existing far.rs degree4 Legendre remainder
is the same geometric-bound principle; this is a new positive degree17 source
cubature, not a whole-current energy-norm certificate. Test cache bit identity
and the far point against independently transformed positive source integration.

### Ordinary two-via actual-source screen and foil potential refinement

`clearance50-two-via-positive-modal148-graded8x4-z8um-v0` completes65.194s/
378.95MiB, positive/SPD/directed reciprocity gates pass, but its independent
148-original-source screen FAILS: whitened error43.8020125% plus reference
change0.3345774%=44.1365898% (minimum relative energy0.8647621). The exact-bit
reference `clearance50-exact-source-reference-pairs0000-v1` passes8.366s/
30.88MiB. This is an incomplete diagnostic subspace, never1656-space admission.
The preceding v0 launcher packet used wrong argument order, exited1 before any
calculation; preserve it, exclude from numerical evidence.

Predeclare the next spatial control: remove the potential-only64um slab grading
exemption. It left both35um Cu foils as single potential intervals even when an
8um step was requested. Dyadic grading now applies inside the original foil
slabs too; original Cu tetrahedra/current coefficients, contacts, materials and
source loads remain unchanged. Exact source slice volume/first-moment controls,
256 Z-DOF/memory/time limits and all residual/accuracy gates remain. Compare the
same frozen148-source reference; a smaller XY trial needed for byte admission
is a measured alternative, not an accepted accuracy result. Do not rerun the
old successful whole-space primitive reference.

Foil grading8um/P8xP2 completes79.049s/271.91MiB but independent148-source
error21.1008038%+0.3345774%=21.4353811% FAIL. P6xP4 foil grading reaches the
strict image residual for three auxiliary loads, then110s internal timeout
(112.595s supervisor/299.71MiB); preserve failure. Refine the same P8xP2 potential
step8->2um next, under unchanged byte/DOF/time gates, reusing the exact reference.

Ordinary two-via full electric trace `clearance50-field-near8-g125-d128-bore1-trace-v0`
passes all337 coordinates (150006 dielectric tets): residual8.43347e-11,
charge/energy2.98038e-12, directed reciprocity1.59010e-12;58.165s/444.22MiB.
This is discrete Schur verification only; spatial/domain convergence is pending.

Predeclare an alternative original-volume Newton *near* integration control,
independently derived from the divergence theorem (NIST DLMF1.6.58/59,
https://dlmf.nist.gov/1.6): since div_y grad_x |x-y|=-2/|x-y|,
I(V,W)=-1/2 sum_faces(nV dot nW) integral_faceV integral_faceW |x-y|.
For a triangular source face with plane distance h, in-plane outward edge
projection s and exact edge line integrals L=integral1/r, J=integral r,
integral_face r=(sum_edges s*J+h^2*(sum_edges s*L-h*solidAngle))/3.
Evaluate only the remaining positive target-face Gauss nodes. This is a signed
boundary identity, not a positive cubature claim: record summation condition,
refuse nonfinite/nonpositive results, retain directed1e-9 and0.2% refinement,
and compare against the existing independent analytic-volume reference before
any production use. First control actual-source pairs with known references;
no cell/current removal, no averaging. Separated pairs retain independently
bounded existing methods because boundary cancellation grows with distance.

The8->2um P8xP2 operator completes its nine strict auxiliary images in91.076s,
but its148-source materialization reaches110s lifetime (114.511s supervisor,
357.51MiB); no numerical matrix accepted. Retain this resource failure.
Signed-boundary Python controls on ordinary sources0..7 and the saved worst
sources15/16/17/28/60/61/62/63 complete6.276s and5.506s; order32 maximum
self-normalized discrepancies3.22812e-9 and1.89799e-9, directed reciprocity
3.69050e-13 and1.34783e-12. These are local controls, not whole1656-space admission.
Add an opt-in native `surface` candidate using the same original hierarchical
far interpolation, exact self integrals and unchanged directed/refinement gates;
the old volume-integral path remains separately available. Preserve all signed
cancellation diagnostics and refuse nonpositive/nonfinite integrals. Run the
frozen complete24-source independent screen before ordinary-board P6 attempts.

Native signed near integration passes the frozen *complete24*-source independent
screen `signed-surface-frozen-complete24-control-v0`: candidate0.0757650% plus
reference0.6780636%=0.7538286%<1%; prism-space check1.31599e-7;2.361s/12.76MiB.
Ordinary surface32 stops on pair280/129 directed reciprocity after5.651s; surface64
later refuses a nonpositive signed boundary result (92.774s/37.74MiB). Neither
reaches P6. Preserve both. The ordinary exact independent analytic-volume queue
now retains161026/1371996 pairs, without repeats or relaxed gates.

Inspecting retained tetrahedra finds real discrete current cells with volumes
as small7.89508e-22m³ and geometric condition4.20113e7. They are not removed or
merged. Predeclare a numerical integration fallback: on a nonpositive signed
boundary result or diagnostic128eps*sumAbs/absSum>1e-9, evaluate that same
original tetra pair using the existing positive volume/source formula. This is
path selection, not an error certificate: retain every directed sample,0.2%
refinement and1e-9 mutual gate plus the independent whole-current energy screen.
Log original offending pairs and count fallbacks/maximum signed condition; no
averaging/clipping and no new tolerance. Keep timeout failures fatal. Improve
reverse-pair failure context. Release independent electric assembly-only arrays
before factorization, retaining gradient/volume energy audit and identical math;
this reclaims memory for the next actual field mesh without raising512MiB.

Predeclare geometry-bound near-integral checkpointing for the existing bounded
constructor: each immutable packet stores only independently directed/refinement-
admitted original tetra pairs, exact native geometry/self-integral bits and fixed
method/order metadata. Reuse requires exact header identity, finite positive values,
unchanged0.2% refinement/1e-9 reciprocity and no duplicates. Missing pairs are still
computed; no approximate zero or hidden default. An8-byte triangular lookup per
original pair plus bounded input/output buffers is added to the existing preflight.
Stream completed pairs before the110s constructor deadline; each supervised packet
remains<=120s/512MiB. This preserves work across expensive near construction and
separates cold aggregate preparation cost from a warm P6/DSL call. It is a complete
finite-volume kernel intermediate, never a manual equivalent RLC. A successful warm
call cannot be reported as a cold-under120s scaling solution. Whole-source independent
energy/SPD/refinement validation is still mandatory before physical admission.

Hybrid ordinary retry `clearance50-hybrid-surface64-p6-candidate-v0` retains its
LOCAL_DEADLINE at near336/645/order32 (114.436s supervisor/41.23MiB), after logging
the conservative signed-cancellation transitions. No P6 candidate is accepted.
Checkpoint implementation now includes bounded NDJSON lines, integer F64-bit payloads,
exact geometry/self/method identity, directed/refinement revalidation, duplicate and
memory refusals, canonical directed values after the mutual gate, immutable output
creation and checksummed supervisor input/output provenance. Aggregate cold time
includes failed supervised preparation packets; a warm call is separately identified.
`cargo check --tests` passes with the two existing unused fixture-helper warnings.
Release examples and the focused integral/checkpoint integration test are rebuilding;
checkpoint execution and whole-current admission are still pending at this entry.
The controller is `prepare-integral-checkpoints.py <prefix> <request> <max-packets>`;
it resumes only packets with new accepted rows and a deadline, and stops on any
other refusal or no progress. No source pair is silently skipped.

Checkpoint control `original-pair-checkpoint-controls-v0` PASS8.429s/12.44MiB:
all three Cartesian cells replay bit-identically, wrong geometry/order and duplicate
inputs refuse. Ordinary cold packets000/001/002 preserve45320+15+0 valid near pairs,
340.788s aggregate; each packet<=120s/512MiB. Packet002 repeats the636/645 deadline
without progress; the controller stops, not retries. Keep all45335 accepted pairs.

The concrete local obstacle is finite tetra645 (volume2.16271e-20m³, condition1.13985e6)
against ordinary tetra636. F64 double-boundary cancellation is about1.456e6;
positive conditional-source fallback exceeds110s at target order32. No source can
be removed or replaced by its centroid. Predeclare extending the existing two-F64
compensated arithmetic to the *same* boundary identity: compensated products/division,
Newton square root, range-reduced logarithm/atan series, and compensated Legendre
nodes/weights. Primary numerical context:[Hida/Li/Bailey](https://www.davidhbailey.com/dhbpapers/qd.pdf),
[NIST logarithmic series](https://dlmf.nist.gov/4.6),
[NIST inverse-trigonometric series](https://dlmf.nist.gov/4.24).
This retains CPU F64 storage/operations; no new scalar transport/runtime or physics.
Use it only on signed-cancellation/range cases, retain all1e-10 source/1e-9 mutual/
0.2% refinement/time gates and the original positive fallback on compensated failure.
Check elementary values against frozen independent mpmath values and Gauss moments,
then actual636/645 both directions against cheap ordinary-volume integration with
well-conditioned source636. Existing strictly accepted cache rows remain reusable:
the analytic identity and admission gates have not changed. No acceptance is inferred
from reciprocity alone; whole-source independent energy validation remains pending.

Compensating elementary arithmetic alone cannot cure quadrature cancellation:
when a tetra becomes thin, one large face and two smaller faces cover nearly the
same plane, but independent Duffy node sets are not additive. Predeclare matched
projected target integration for the *same* boundary identity: choose the dominant
axis of the largest tetra face; project the four original vertices. A projected
vertex inside the opposite triangle gives its common fan; a convex quadrilateral
uses the intersection of its two diagonals and four fan panels. Each nondegenerate
panel lies in exactly two projected boundary faces. Evaluate both affine face
heights at the same positive nodes and combine the compensated vector flux before
summing. Exactly vertical faces retain their original surface integration. No mesh
vertex, current support, material or contact is moved; only quadrature coordinates
change. Sum source-face analytic flux at each target point before integration too.
Use the earlier two-sided fourth-power endpoint mapping for q>=8; q2/4 remain
ungraded controls. Verify projected coverage/opposite normals, positive weights,
Gauss moments, and actual636/645 versus the separate positive volume transform.
This projection cancellation derivation is ours; the cited sources establish the
divergence identity and compensated arithmetic, not this particular construction.

The current-production-files direct math build (small rustc harness, same actual
prism/mesh/polyhedron/surface source files, checksummed; no alternate electrical
runtime) passes both elementary/Gauss controls. This is a fast method-development
check; full crate/native build remains mandatory. Actual636/645 now passes through
order64: directed4.70819e-10, independent ordinary-volume discrepancy5.00075e-11,
unchanged0.2% refinement,82.356s/31.91MiB including supervision. Preserve everyq
sample; q32 still fails1e-9 mutual. The order64 phase costs55.153s, so improve
series evaluation before using this on the whole ROI. Precompute DD rational
reciprocals and stop range-reduced series only when twice the geometric/alternating
next-tail bound<=1e-35*abs(partial sum), below DD rounding; fixed30/56 maximum terms
remain. Physical tolerances stay unchanged. Reuse the already successful independent
ordinary-volume reference value from the frozen packet for performance replay;
do not recompute that expensive reference. Profile both compensated directions.

Optimized direct math control v2 passes all three elementary/Gauss/shared-edge and
three canonical prism-split controls. Actual636/645 v2 completes5.318s/31.91MiB;
q64 directed4.70819e-10 and frozen independent discrepancy5.00075e-11 remain
unchanged. Its two compensated directions total1.492s atq64. Reuse the frozen
ordinary-volume reference; no repeated reference integration. Shared source-edge
primitives replace duplicate face-edge evaluations; projected face coefficients
and source radii are reused. All projected faces retain coverage and original
volume checks (1e-12 relative volume, no geometry change).

Integrate this arithmetic path only after the F64 surface cancellation/range guard;
its conservative diagnostic uses1024eps² times the measured signed condition against
the same1e-9 budget, followed by the unchanged actual directed mutual/refinement
gates. Nonfinite/range failures still use the old positive-volume formula; deadline
and Gauss/moment failures remain fatal.64KiB/cell covers original source, optional
1000-point far cache and lazy compensated face/edge/panel storage;256KiB fixed
allowance covers both Gauss caches and reciprocal table. No512MiB/DOF cap changes.
The bounded example also emits all-original-source columns from a complete kernel
checkpoint, enabling full1656-source independent/SPD validation before P6 admission.

Corrected arithmetic regression: before-fix packet `compensated-projection-regression-before-fix-v0`
reproduces an exactly vertical face's nonzero residue1.83671e-40 and an unresolved
actual272 duplicate projection. Symmetric compensated multiplication and exact
duplicate-point identities fix these without moving any vertex or applying a
geometric tolerance. Five post-fix elementary/Gauss/actual-source-flux and projection
regressions pass (`compensated-projection-regression-after-fix-v0`). Add all three
closed-surface first-moment identities, not only projected volume.

Conservatively quarantine *all*64227 new rows in near packets003/004, including any
unaffected rows; keep original evidence and CACHE-REJECTED.json with reason/SHA.
The reader refuses marked packets and the controller excludes them. The original
45335 rows from pre-DD000/001 remain resumable; independent analytic references
161026 pairs are unaffected. Aggregate cold work still includes rejected packets.

Actual645/646 now isolates a quadrature issue: q64 directed3.76235e-8 FAIL1e-9
(8.685s; no independent reference was computed, explicitly null). Both thin supports
share a face and their projected density ridges are not aligned with the target fan.
Predeclare splitting matched target panels along the six projected source-edge
lines for touching original supports. These are integration panels only; retain
both original supports and all finite coordinates/materials. Use positive convex
line splits, exact-zero degeneracies only, and1e-28 projected area/first-moment
conservation. Bound workspace to64 polygons/12 vertices per polygon and256 final
triangles within the existing256KiB fixed allowance. Quadrature ceilings,0.2%
refinement,1e-9 directed reciprocity, original self integrals and resource caps stay
unchanged. No physical admission from this local directed-only control.

Source-edge aligned control `compensated-pair645-646-edge-aligned-control-v0`
passes unchanged refinement/mutual gates atq16:1.23778e-15 directed error,
0.943s/31.95MiB including supervision. It has no independent reference and is
explicitly not physical admission. Five elementary/projection regressions pass
in `compensated-projection-edge-aligned-regression-v0` (6.931s/32.36MiB).
Actual636/645 replay against the unchanged frozen independent positive-volume
reference also passes (`compensated-pair636-645-edge-aligned-control-v0`):
q64 directed1.62307e-10, independent discrepancy5.00075e-11,8.263s/31.89MiB.
Rebuild the full crate before producing further checkpoints; old003/004 remain
quarantined. These controls do not establish complete-source or PCB/P6 accuracy.

Post-fix original ROI near packets005/006 retain43373/38264 new admitted rows;
total126972. Packet007 repeats target618/source616 without progress,111.946s:
source-edge target-area audit refuses, then the old positive-volume path reaches
the unchanged deadline. Cold aggregate including all rejected/empty packets is
819.385s. All successful pre-failure rows remain reusable; no whole constructor.
Predeclare an affine barycentric integration-panel split: perform line cuts on
the unit target triangle, map points back by its original affine transform, and
multiply each positive local determinant by the original parent determinant.
This avoids re-subtracting near-collinear global coordinates when measuring tiny
panels. Retain1e-28 area/first-moment checks, all original supports, fixed workspace
and physical gates. Only exactly zero represented integration panels may vanish;
no original copper cell is removed. First reproduce618/616 in the direct harness.

The unchanged direct global-split harness reproduces618/616's area refusal
(`compensated-pair618-616-global-split-refusal-v0`). Affine split passes q16
directed6.16974e-15 with unchanged refinement (`...affine-split-control-v0`,
0.623s/31.96MiB). Actual645/646 also passes after this change;636/645 continues
to match its same frozen independent reference5.00075e-11, directed1.62307e-10
(2.552s/32.08MiB). Six direct production-source unit controls now pass, including
a permanent exact-bit618/616 regression (`compensated-affine-split-regression-v1`).
No threshold changed. Original successful000/001/005/006 rows remain valid; no
whole-ROI/P6/DSL admission follows from these local controls.

Affine full Rust build passes; frozen complete24 screen passes again
(`compensated-affine-frozen-complete24-control-v0`,1.169s). Independent ordinary
positive-volume reference packet161026-v1 retains28973 additional successful
rows (total189999/1371996) before the120s supervisor cap on615/579. Preserve its
partial successes; no repeated reference integration for these rows. This is a
resource refusal, not an accepted complete table. Resume later at189999 with a
fresh bounded clock, retaining all prior successful exact-identity packets.

Affine008 adds8 successful pairs, then009 confirms no progress at615/619:
the original-coordinate first-moment audit loses resolution on its narrow panel;
both attempts reach the unchanged deadline. Total126980 near rows, aggregate
cold1043.081s including failures. A direct harness reproduces the moment refusal
(`compensated-pair615-619-global-moment-refusal-v0`). Apply the same affine change
of variables to the first-moment audit: normalized local area=1 and each local
first moment=1/3, each checked at1e-28 before the original affine map. Because
integration and first moments commute with an affine map, this checks the same
panel conservation without an ill-conditioned reconstruction. No threshold or
physical geometry changed. Direct615/619 then passes through the existing gates
(`compensated-pair615-619-affine-moment-control-v0`,0.832s/32.05MiB); independent
physical whole-space admission is still pending.

Current full build/frozen24 pass after the affine moment correction. Packets010,
011,012 add8136/3725/1379 valid rows:140220 total.012 is a supervisor refusal
(120.278s,55.02MiB), not a completed constructor; partial admitted rows persist.
Cold aggregate1399.519s. The1/666 mixed/paired quadrature experiment reaches the
sameq64 reciprocal result6.75850e-12; paired forcing does not improve this case
and is not implemented in the operator. Direct306/653 already passesq4 in1ms.
Add read-only completed-row timing observations to the controller to identify
actual slow pairs before changing algorithms. Observations sample batches and
are explicitly not exact per-pair timings; all numerical guards stay unchanged.

013 retains5319 more rows (145539 total),111.341s; aggregate1510.859s. Its
observations show repeated moderate costs rather than a single stuck pair;
3899 rows requireq16,702 q32,121 q64. Optimization-level3 on the direct frozen
636/645 control preserves every reported integral bit and passes seven units,
but gives no demonstrated speed benefit (q64 phase1.848s); retain the current
operator build configuration. Actual612/653 hybrid control passesq32,0.523s
pair phase (`hybrid-pair612-653-touch-only-control-v1`;v0 launcher ran before its
build manifest existed and performed no numeric work). Predeclare allowing the
same conservative projected source-edge integration split for non-touching
compensated pairs too; projected density transitions can cross a target even
without shared3D vertices. Retain all area/moment, workspace, quadrature and
physical gates; measure this pair before deciding whether to retain the change.

All-edge compensated612/653 passes atq16 rather than32: directed7.09640e-13,
q16 phase0.235s and1.410s supervised (`hybrid-pair612-653-all-aligned-control-v0`).
Seven unit controls also pass (`compensated-all-aligned-regression-v0`). Retain
the conservative numerical panel split pending complete-source admission. Lower
the checkpoint/column constructor's internal deadline110->100s, leaving room for
board preparation/provenance inside the unchanged120s external supervisor; no
accuracy or memory limits changed. Full crate/frozen24 must pass before resuming.

All-edge full Rust build, frozen24 and checkpoint/quarantine regressions pass.
Packets014/015/016 add2470/2570/2676 rows:153255 total, aggregate1818.702s.
These packets stay below104s; no complete constructor yet. Predeclare dyadic
table centering for the existing compensated elementary series, not altered
quadrature: ln(m)=ln(c)+2atanh((m-c)/(m+c)), c=1+k/128; atan(t)=atan(c)+
atan((t-c)/(1+t*c)), c=k/128. Freeze129+129 two-F64 constants once from mpmath90
and independent128 logarithm/512 quadrant off-grid controls on exact two-F64
inputs. The same1e-35 series-tail criterion and30/56 maximum terms remain;
tables reduce argument size, not precision. Static storage4128bytes remains
inside the256KiB fixed allowance. Sources rechecked: [DLMF4.6.4](https://dlmf.nist.gov/4.6.E4),
[DLMF4.24.3](https://dlmf.nist.gov/4.24.E3),
[DLMF4.24.15](https://dlmf.nist.gov/4.24.E15). No expensive PCB reference rerun.

Table centering passes640 new off-grid references and all seven previous units.
636/645 keeps every integral bit/reference result. Immediate historical/current
checks using checksum-matched immutable source snapshots give1.579/1.567s q64
phases, so no substantial table speedup is established. Before full build, test
standard quotient-correction division and a checked Newton sqrt: retain up to
two sqrt updates, return after the first only when the computed squared residual
<=8eps²*abs(input); this is below the existing1024eps² diagnostic budget. Reuse
FMA two-word scalar products for each quotient residual rather than multiplying
the whole accumulated quotient twice. [QD sections3.5/4.2](https://www.davidhbailey.com/dhbpapers/qd.pdf)
describe quotient corrections/Newton refinement. Add128 independent sqrt and128
division controls spanning signed operands/exponents, target1e-29 relative.
Skip atan's table division/addition for the exact zero center. No physical gate,
original source geometry, elementary tail bound or quadrature ceiling changes.

Corrected elementary core passes nine unit tests, including256 new arithmetic
and640 off-grid transcendental references. Frozen636/645 values stay bit-identical
(`compensated-corrected-core-frozen636-645-control-v0`,q64 phase1.235s,2.122s
supervised); this is a local performance control, no global speedup claim.
Predeclare bounded four-thread CPU near construction. Each pair retains the exact
existing formulas/order/refinement/reciprocity gates. Preallocate the same ordered
Near vector, distribute disjoint16-pair chunks dynamically, store each result in
its original slot, serialize admitted checkpoint writes with a mutex, and join
all threads on any failure/deadline. Report every worker error; never admit an
incomplete operator. Explicitly budget256KiB workspace/thread plus512KiB stack
for each of at most4 scoped workers; keep512MiB/100s internal/120s external caps.
Single-thread/four-thread complete frozen matrix actions must be bit-identical,
and checkpoint replay/refusal tests must pass before real ROI continuation.

Implementation preflight additionally accounts for each worker's existing4096-
leaf positive-volume fallback: final parallel allowance is1MiB workspace plus
512KiB stack per worker (6MiB at4 workers), rather than256KiB. This tightens the
memory plan while retaining the512MiB cap. cargo check --tests --examples passes;
only the two existing fixture unused-item warnings remain. No parallel real run
before the full optimized build and complete-source bitwise/reference tests.

First parallel complete24 test v0 exposes an invalid test comparison: parallel
checkpointed vs sequential *uncached* uses different retained directions for
reversed tree-order pairs. One0/2 entry differs5.4e-11 relative, below the existing
directed gate, but correctly fails the declared bitwise assertion. Keep this
failed evidence. Compare single/four-worker **cold checkpointed** constructors
instead, preserving identical canonical direction semantics and the exact
bitwise gate; do not weaken the assertion or average the two directions.

Corrected four-worker complete24 control v1 passes (0.986s/12.48MiB): all72
Cartesian columns are bit-identical to the cold single-worker checkpoint, and
the frozen full-source energy bound remains0.0075382867014<0.01. Cache/refusal
controls including workers0/5 and explicit insufficient memory pass (0.390s).
Real packets017/018/019 retain11670/11563/9943 new pairs, total186431; each ends
with a reported deadline and joined sibling cancellations,101.4..101.8s and
70.1..71.1MiB. Aggregate cold work2123.368s includes all earlier rejected runs.
No complete constructor/independent real matrix/P6/DSL admission yet.

Independent615/579 fresh-clock retry (`...reference-pairs189999-v1`) also reaches
the120s supervisor cap with zero new rows. Preserve189999 prior valid pairs and
both refusals. Future reference launcher records/checks the child executable SHA;
older packets retain their recorded source snapshots but did not explicitly
record that child hash. Do not regenerate their costly numerical successes.
Predeclare a reference-only compensated point-potential/positive target-volume
route, independent of production's signed double-boundary distance integral:
Phi(x)=1/2 sum_faces h*(sum_edges s*logEdge - h*solidAngle). Derive this from
div((r-x)/|r-x|)=2/|r-x| and the planar face identity; [Werner's NASA record](https://ntrs.nasa.gov/citations/19950035213)
confirms the closed finite-polyhedron approach (the linked2017 PDF fetch failed;
do not claim it was read now). Construct reference faces separately, integrate
positive Phi with original tetra Duffy volume weights; retain exact source bits,
source roundoff1e-12, positive moments1e-28,0.2% refinement,1e-9 reciprocity and
existing ceilings/resources. It shares the independently tested scalar two-F64
arithmetic, not production's pair integrator/flux/distance code. Validate Phi
against separate positive source-volume cubature at near/far/interior points,
then validate actual636/645 against its unchanged frozen ordinary-volume value.
Only after those controls use it to resume missing reference pairs.

Reference point controls v0/v1 refuse on the anisotropic interior control:
positive source q31->64 changes3.67059e-4, while the analytic/q64 discrepancy is
7.09484e-5. Both exterior points agree within2.1e-14. Preserve this unresolved
interior control; do not loosen its1e-6 gate. Predeclare positive source-radial
integration with adaptive face triangles for that interior reference: the radial
coordinate is exact, each face cone stays positive; Gauss8/16 face estimates,
longest-edge subdivision,1e-10 partition audit,1e-9 aggregate estimate,4096-leaf
and100s caps. This changes only the independent control's integration panels,
not its source or evaluation point and not the candidate point-potential formula.

Positive interior reference converges with96 face leaves, estimated9.43225e-10;
analytic potential differs4.92919e-15. Actual-point control then reveals an
independent geometry-coefficient issue. Exact-vertex audit over all1656 sources
finds only1641/1642/1643 exceeding the unchanged1e-10 volume gate, with maximum
4.69660e-10 at1642. These are the three splits of the same physical thin prism;
exact rational Fraction evaluation gives identical volume bits4291318731771751028
for all three, while naive native determinants differ. Preserve all failures.
Predeclare compensated determinant/cofactor P1 coefficients from unchanged
vertices, with exact rational volume/gradient controls over all24 orientations
of each split. No support removal or coordinate movement. Existing source-pair
payloads remain immutable: their node/cell identity is unchanged, but updated
self-diagonal headers must be obtained from native code and every retained row
revalidated against both old and new strict gates before checkpoint reuse. Full
independent matrix admission still required; never silently accept old headers.

Exact Fraction72-orientation regression fails before the coefficient fix and
passes afterwards. Updated volume audit matches all1656 exact two-F64 volumes
bitwise; point controls now pass (max2.02094e-14). The new positive target-volume
reference still refuses frozen636/645 atq64 (directed2.52177e-8), preserving that
failure; no acceptance from one-sided closeness. Predeclare positive adaptive
target tetrahedra, Gauss4/8 estimates, original-coordinate midpoint bisection in
two-F64 arithmetic, exact half-volume weights and1e-12 geometric partition audit;
aggregate estimate1e-11*self norm,4096 leaves, same110s deadline. Compare both
directions and previous rule under unchanged0.2%/1e-9 gates, retaining the frozen
independent value as an additional control. No higher quadrature ceiling.

Positive adaptive636/645 control also refuses:4096-leaf estimate2.53829e-39
exceeds5.63553e-40,35.299s/37.21MiB. Preserve refusal. Predeclare contact-aware
target vertex permutation: exact shared original vertices first. In the same
positive tetra Duffy map this puts a shared vertex at u=0, edge at v=0, face at
w=0, matching the existing endpoint grading. Reordering does not change support,
Jacobian magnitude, material, quadrature ceiling or4096-leaf/error limits.
Retest the frozen pair before using any new independent rows.

Contact permutation alone does not resolve the frozen control (v0,67.214s,
same4096-leaf estimate refusal). Use the established positive reference's8/16
adaptive rule instead of4/8, with identical1e-11 norm error,4096 leaves and
110s deadline; enter adaptive refinement afterq16 instead of repeating the
already-refused uniform higher rules. This increases local integration order,
not resource/accuracy limits. Keep all previous failures.

Adaptive8/16 frozen control reaches the110s internal deadline (117.683s
supervised); no accepted pair. Predeclare a guarded ordinary-F64 evaluation
of the *same reference point1/r formula* for well-conditioned sources only
(shape condition<=16). Require positive finite value and512eps*absolute-term
condition plus a bound for rounding the target point<=1e-12; otherwise retain
the compensated formula. Bound point displacement using the smaller of the
enclosing-ball4piR²*delta/V and disjoint-AABB R*delta/distance² estimates. This
retains the existing source error budget; no target error/reciprocity relaxation.
Check against compensated evaluation and independent positive source cubature,
including actual636, before using the fast branch for the frozen pair.

Guarded point controls pass (`guarded-volume-reference-point-controls-v1`), max
independent2.53669e-14; one fast case matches compensated exactly. Its adaptive16
frozen pair still reaches110s (111.690s supervised), no accepted reference row.

Corrected geometry's four-worker complete24 bitwise/frozen control passes
(`exact-coeff-four-worker-complete24-control-v0`,2.754s). Actual source signature
and all1656 native self diagonals are regenerated only because coefficients
changed (`clearance50-exact-coeff-source-signature-v0`,1.544s;
`clearance50-exact-coeff-self-header-v0`,0.953s). Exact node/cell identity is
unchanged. `clearance50-exact-coeff-cache-rebase-v0` copies all186431 old admitted
rows byte-for-byte after checking every old AND new1e-9/0.2% gate,2.208s/28.86MiB.
Only the native self-bit header changes; original packets stay immutable. This
does not admit the full kernel or replace whole-space independent validation.

**Resume the corrected-volume cache with this NEW prefix/seed (not old headers):**

```powershell
python tests/fixtures/analysis-local-extraction/board-integration/prepare-integral-checkpoints.py clearance50-exact-coeff-near debugging/p7-board-integration/STM32-rs485-two-via-clearance50-request.json 3 debugging/p7-board-integration/clearance50-exact-coeff-cache-rebase-v0/seed-manifest.json
```

The seed carries inherited cold packet cost2123.368s, plus its signature/header/
revalidation packet costs; no prior failed/rejected cost is hidden. A complete
constructor, real full-source independent matrix, P6 and DSL remain pending.

Continue reference performance without changing its integral: regroup the two
adjacent face contributions per original edge in Phi's existing formula. Cache
six original edges, compute four vertex radii once, use six logarithms instead
of twelve; keep the four face solid angles and the same absolute-term roundoff
budget. Retain the separate face implementation for point-control comparison.
This is an algebraic evaluation change, not production boundary-flux reuse or a
new source approximation. Add adaptive stage/leaf timing to locate remaining
reference cost; all limits and prior refusals remain.

Shared-edge point controls pass (`shared-edge-volume-reference-point-controls-v0`,
3.210s). Frozen636/645 still refuses at110s: forward reaches1452 leaves with
estimated9.99741e-12*self norm in15.092s, reverse passes1024 leaves but has not
reached the unchanged1e-11 estimate. No reference pair admitted. All1656 native
volume bits separately match the saved independent exact Fraction results.
Predeclare immutable reference-only adaptive tree checkpoints: retain exact
source/target vertex bits, tolerance bits, executable hash, deterministic
original-coordinate midpoint splits and both low/high compensated leaf sums.
Validate full binary coverage, unique parentage, longest-edge splits, finite
positive sums and recomputed error before reuse. Evaluate both children before
replacing a parent, so a deadline retains full original support. Continue only
within the same4096-leaf/quadrature/accuracy ceilings and bounded packets; carry
all cold packet costs. This is intermediate work, never physical admission.

Checkpoint controls pass (`positive-reference-checkpoint-controls-v1`,2.622s):
forced interruption during a child retains its covering parent; resumed/cold
115-leaf results match bitwise; repeated-child and changed-tolerance states refuse.
The initial v0 control did not exercise interruption because the distant root
already converged; preserve that failure. V1 uses a nearer overlapping control
source, not changed production tolerances. State input hashes must match prior
supervised output records and the same reference executable.

**Frozen636/645 positive-volume reference now passes.**
`checkpoint-volume-frozen636-645-control-v0` retains1452 forward and2016 reverse
leaves before110s (110.448s/39.96MiB); v1 resumes and completes in11.716s/40.20MiB.
Final leaves1452/2244, error estimates9.99741e-12/9.98961e-12*self norm;
forward3.014410795115482e-29, reverse3.014410795115379e-29,
directed1.83010e-14. The additional unchanged frozen independent ordinary-volume
value differs2.12338e-10*self norm, below1e-9. Previousq16 and all existing
0.2%/1e-9 checks pass. This validates the reference pair/method control only;
actual full-source matrix, physical field convergence, P6 and DSL remain pending.
Resume the previously blocked independent pair615/579 with this controlled route.

Corrected near packet003 retains4797 new rows (195755 total),101.535s/70.91MiB;
constructor is still incomplete. Keep its reported joined-worker deadlines and
cold aggregate2558.810s. Original ROI scaling is not resolved by these checkpoints.

Independent615/579 now passes via positive-volumeq16 (`...pairs189999-v2`,1.515s),
directed4.53866e-11. The next table packet retains29 more rows,190029 total, then
reaches110s on615/609. Its explicit checkpoint v1 retains124/2565 leaves;
v2 resumes to4096 reverse leaves but refuses: estimate4.751822626497017e-42
exceeds2.0157154485288577e-42 (2.35739 times). Preserve all refusals.
Predeclare hp refinement *at the existing4096-leaf cap*: keep complete original
geometry and increase only the worst eligible leaf's8/16 rule to16/32, then32/64.
Retain the existing64 quadrature ceiling, positive weights/moments, aggregate
1e-11 norm estimate,0.2% refinement and1e-9 directed gates. If all eligible work
is exhausted, refuse. Store each leaf order and both sums; replace a leaf only
after its higher rule completes. Do not increase leaf/time/memory limits.
The old bounded sums may be copied into a new immutable state only after the
old supervised hashes and unchanged exact integrand/geometry/arithmetic source
files are verified; record this explicit policy-only rebind, never silently
accept an arbitrary old executable identity. Retest cold/resume/order controls.

`positive-reference-hp-checkpoint-controls-v0` passes cold/resume bitwise and
refusal controls (3.233s). `clearance50-positive-reference-hp-rebind-v1` verifies
the frozen old-policy SHA and unchanged source potential/target cubature,
arithmetic, mesh, manifest and example files, then copies124/4096 leaves with
unchanged low/high bits and explicit order16/new executable identity (0.909s).
V0 is an empty metadata refusal due to Windows source-key separators; retain it.
`...reference-pairs190029-v3` now passes in93.518s/41.69MiB: same124/4096 leaves,
598 reverse leaves raised16->32,3498 remain16, none need64; estimates
9.77876e-12/9.99578e-12*self norm and directed7.44067e-15. Both previousq16
0.2% checks pass. Full source reference is still incomplete. The successful
frozen636/645 payload is copied byte-for-byte, with original source/run hashes,
to `...reference-pairs208971-v0`; its expensive calculation is not repeated.
Reference planning now streams saved rows, selects fresh retry versions, honors
already-completed gaps and retains successful partial rows even after deadline.

Near packets004/005 retain4272/4511 more rows:204538 admitted near rows,
cold2762.832s; both packets report joined-worker deadlines. Constructor incomplete.
`real-via-return-scenario.js` records the intended existing direct DSL with actual
via IDs, TOP/BOTTOM finite contacts, the same ROI/stack/materials, and no authored
equivalent circuit. Syntax checked; layered runtime remains unimplemented.

Field bore2 geometry v0 refuses the diagnostic's200000-tetra count cap before
allocation (1.282s/28.09MiB). This is not a failed physical convergence result.
Predeclare deriving the diagnostic count ceiling from its *unchanged*512MiB
byte budget and the builder's existing2048 bytes/tetra preflight (262144 cells),
retaining100000 nodes,120s supervisor and all sparse-factor/live-byte checks.
This changes only the diagnostic mesh count guard, not a dense operator cap or
public production default. Record the original refusal. Refine only bore air
points, keep exact Cu/contacts/material/domain, then compare the full337-trace
Schur matrix against the saved bore1 under the existing0.2% energy-space gate.
No electric or PCB acceptance until that comparison and domain/scope controls.
The independent reference also releases remaining duplicated JSON geometry lists
before sparse LU for geometry-only packets; numerical NumPy inputs and all
charge/energy/residual audits are unchanged. Candidate comparison metadata stays.

Bore2 geometry v1 passes (200382 total/180582 dielectric tets,2.383s/125.74MiB).
Cu volume changes3e-16 relative, and electrode areas, barrels and source mapping
are identical to bore1. Independent337-trace solve passes38.450s/440.91MiB,
residual8.74215e-11, charge/energy2.55513e-12, directed2.44145e-12.
**Physical full-field convergence still FAILS:** bore1->2 full whitened
Frobenius3.036179 (303.618%), max relative energy39.2917%, versus0.2%.
Component/DC-harmonic restrictions pass0.023985%/0.066794%; neither admits the
full trace or PCB. A separate diagonal-scaling diagnostic reproduces3.036179,
scaled condition133639 and scaled/unscaled eigenvalues differ only5.24e-12;
this is not an unscaled eigensolver artifact. Continue bore4 with identical
physical inputs, fixed byte/time/node limits and full-space gate.

Bore4 geometry passes2.430s/152.04MiB. Its direct independent LU exceeds512MiB
before any accepted full trace: v0 at7.344s, v1 after earlier JSON release at
9.965s. Stage logs show only95.7MB immediately before LU; the factor causes the
refusal. Keep both failures. Predeclare an optional reference-only AMG/CG solve
of the *same assembled SPD free-node matrix*, with symmetric diagonal scaling,
unchanged normalized original-equation residual/charge-energy/reciprocity gates,
and bounded column packets if needed. AMG is only a preconditioner; never modify
K, impose extra ground or regularize. Compare initial columns against saved
bore2 LU without repeating that successful solve, then complete all337 columns
before a full-field claim. Use pinned PyAMG5.3.0 in ignored evidence-local
`reference-python` (no global Python/project dependency change), installed
without dependencies. [Official PyAMG aggregation documentation](https://pyamg.readthedocs.io/en/latest/generated/pyamg.aggregation.html)
describes symmetric smoothed aggregation and CG preconditioning. Retain120s/512MiB
per packet, explicit inner deadline and iteration cap; all source geometry stays.
Initial eight-column smoothed-aggregation control reaches100s inner deadline
(101.038s/195.16MiB), no admitted columns. The hierarchy itself takes2.803s and
130.5MB. Preserve refusal; log per-column iterations and compare the alternative
documented symmetric classical Ruge-Stuben preconditioner on one saved bore2 LU
column under identical SPD/residual limits before attempting larger packets.
Classical one-column control also refuses at2000 iterations (18.848s/194.53MiB),
no accepted field column. Preserve both AMG refusals; they are solver controls,
not a change to the physical discretization or a reason to lower residual gates.
Predeclare reference-only sparse Cholesky through faer on the independently
assembled SciPy CSC matrix. Unlike SuperLU's two factors, retain one symmetric
factor; transport exact binary64 CSC/RHS bits through a local bounded helper.
Require symbolic factor/scratch byte admission before numeric allocation,
300MiB child allowance plus the supervisor's unchanged512MiB family envelope,
100s inner deadline, at most4 RHS/request, positive definite refusal without
shifts, and unchanged original-equation/energy/charge audits in Python. Hash the
CSC file and child executable. First compare saved bore2 LU columns, then use
the identical solver on bore4. Sharing generic linear algebra is explicit;
field assembly, geometry/material values, trace map and audits stay independent
of the native extraction implementation. No duplicate costly successful LU run.

Reference sparse-Cholesky control passes (`clearance50-field-bore2-faer-eight-column-control-v0`,
2.641s/254.54MiB conservative family peak): saved-LU column relative error
1.27001e-13, original residual3.54238e-15, charge/energy7.54135e-13,
subset reciprocity3.54132e-16. Exact CSC/executable hashes retained; no full-field
claim from eight columns. `clearance50-field-near8-g125-d128-bore4-faer-trace-v0`
then completes ALL337 columns in31.095s/340.23MiB: residual1.31147e-13,
charge/energy1.97818e-12, directed1.42777e-14. The independent241734-dielectric-tet
matrix uses8988894 Cholesky values, planned127479991 bytes in the helper.
**Physical gate still FAILS:** bore2->4 full whitened change2.5626414233
(256.264%), max energy36.5522%. Component/DC-harmonic changes0.017515%/0.047999%
pass their restrictions but do not admit the full field, physical PCB/P6 or DSL.
The sparse solver removes the factor-memory failure, not the unresolved spatial
accuracy. Retain all older LU/AMG failures and full337-coordinate successful data.

Near packets006..011 now retain264403 total rows, cold3376.494s; incomplete.
Predeclare locally adaptive field tetrahedra instead of inserting every bore
point through all92 global planes. Start with a separate sparse field seed that
preserves the ORIGINAL Cu tetrahedral complex and its P1 boundary functions,
then bisect an edge in every incident dielectric/air tetrahedron together.
Children inherit the unchanged material; split boundary faces consistently and
carry exact parent-edge P1 trace weights on prescribed metal vertices. Original
Cu/current cells, coordinates and contacts remain immutable. Require full face
incidence/coverage, positive child volumes and1e-10 parent-volume partition,
finite convex trace weights, exact coarse geometry binding, unchanged512MiB/
120s/100000-node/262144-tetra diagnostic limits. Refinement is guided by all337
source columns' normalized interior flux-jump indicators; indicators choose work
only, never replace the existing full energy-space convergence gate. Retain all
uniform-bore failures. This also avoids treating vertical nodal interpolation
on a differently triangulated side face as exact original coarse P1 trace.
Verify the seed's full Cu tetrahedron vertex-bit multiset before adaptive work.

`clearance50-adaptive-field-seed-geometry-v0` passes2.273s/33.71MiB:24354
dielectric tets,4644 nodes, and all1656 Cu tetrahedra match original source
vertex bits as a multiset. Cu volume is bit-identical9.91815427006387e-11m³.
No adaptive or field-accuracy admission yet; independent actual full337 solve
and refinement controls remain required before using this field in P6.

Adaptive edge-star controls pass; source-consistent initialization passes.
Initial full337 solve v0 meets an exact-zero free-equation batch (all prescribed
values have zero coupling to free nodes), causing0/0 in the diagnostic only.
V1 explicitly requires zero residual when its original-equation scale is zero;
all existing nonzero residual gates remain. Full trace then passes7.204s/102.70MiB,
residual4.68573e-15, charge/energy6.12044e-14, directed1.09430e-13, no zero-charge
coordinates. It does not imply spatial convergence. First mesh refinement v0/v1
refuses a newly proposed midpoint: exact Fraction audit confirms parent-volume
error4.80415e-10>1e-10 on edge150/156. Preserve refusals. The selector now checks
that same unchanged gate before choosing new edges, tries other edges of marked
faces, records rejected *new midpoint proposals*, and retains all original cells
and supports. First accepted pass uses390 edges/1480 splits, max2.68640e-11,
37 refused proposals; all Cu cells remain immutable. No tolerance is lowered.

Adaptive full337 passes through25 complete (80001 dielectric tets). Every
physical convergence comparison still FAILS: last full change0.6408607405
(64.086%), max energy63.851%; earlier20/24 changes4.137%/5.326% do not admit it.
Predeclare replacing diagonal-only work-selection normalization with an
invertible FULL337 charge-energy whitening from the previous complete C. Solve
all337 transformed RHS, recover the original full C by exact congruence, retain
raw reciprocity/charge/energy/original-equation gates, and accumulate flux-jump
indicators in that full basis. This changes only refinement priorities/arithmetic,
not the source space, boundary physics or acceptance gates. No voltage/current
projection or dropped weak mode. Check reconstructed seed C against its saved
full matrix before continuing; do not repeat costly uniform-bore/LU successes.

Internal full-trace checkpoint replay is being added for later P6 integration:
bind original node bits/Cu cells/contact groups/physical inputs, validate raw
directed symmetry, complete exposed SPD metric and exact buried zeros, and keep
all potential coordinates. Its numerical loader is explicitly not physical
admission; no public layered DSL is enabled by a cached unaccepted candidate.

`clearance50-full-whitened-field-seed-control-v0` passes5.841s/115.86MiB:
recovered physical C relative error4.54402e-16 and complete source-energy error
4.82008e-9 versus saved seed, whitening residual4.98843e-9, all numerical gates
unchanged. Applying the previous25 metric to26 refuses *before RHS work*:
full-coordinate whitening residual1.15991e-7 exceeds1e-8. Preserve that refusal.
Use the already verified fixed full337 seed energy metric for refinement
priorities instead; it is an invertible weighting, not a replacement of the
physical C or its convergence gates. No shift, clipping or dropped coordinate.
Reuse the prepared26 geometry and saved seed control; do not repeat them.
Fixed-metric transformed26 RHS route also refuses: charge/energy5.86650e-5>
1e-8 (18.450s/155.42MiB). Preserve refusal. Use the invertible metric ONLY for
flux-indicator postprocessing: keep original unit-coordinate RHS/C/charge/energy
audits unchanged, retain their nodal solutions within an explicit live-byte
preflight, and multiply each bounded face-jump block by the verified full337 Q.
This is work selection only; no transformed reaction matrix is used as physical
C. Keep all337 columns, numerical/physical gates and512MiB family cap; refuse
before allocating a nodal-solution buffer that would exceed its live plan.

This postprocessing-only route passes26..33 without transforming physical RHS/C.
Last complete mesh has100951 dielectric tets;33 full-space change4.58199% still
FAIL0.2% (32:3.71445%). Trace33 takes37.871s/207.88MiB conservative family peak.
All337 original columns, original Cu cells, contact weights and numerical audits
remain; no physical field or PCB admission. Fixed337 metric's check stays4.98843e-9.
Selected frozen adaptive comparisons also show small negative generalized
energy shifts (worst observed−7.90832e-5 at26); record finite-precision/nonexact
midpoint effects rather than claiming exact variational monotonicity. They do
not license acceptance while full changes remain orders above the gate.

`DenseField::load_checkpoint` and `prepare-field-checkpoint.py` now implement
internal full337 coefficient interchange for the P6 path, with exact original
node-bit/Cu-cell/potential-group/physical-assumption identity. Raw directed gate
precedes canonical upper-triangle storage, full exposed SPD is required without
shifts, and buried coordinates must retain exact zero charge. Producer evidence
explicitly says physicsAdmitted=false until separate physical checks pass.
The board diagnostic can consume this *intermediate Schur* via `field-cache:`;
it never accepts a port circuit and omits fabricated field-grid output. Native
replay/refusal tests are added; optimized build/regressions are in progress.

Continuation verification (2026-10-27): 62 focused optimized Rust tests PASS
(field checkpoint4/current8/integral7/gauges9/terminals8/coupled9/mixed13/delay4),
with no selected ignored tests. Ordinary `npm run native:build` PASS11m41s;
addon SHA2569ea872617ee6b8b320d2c9adb56ad56f55eae833a8036d75aeb68a8b39c50f15.
Typecheck/package build PASS. Full Node DSL batches, including fresh processes,
were refused by the unchanged512MiB cap; preserve their journals. The latter
completed18 named runtime tests before the declaration-only tsc test. Compilation
is separate from numerical caps; run remaining runtime names separately and
verify declaration compilation with the ordinary build/typecheck workflow.

Predeclare aggregate scheduling of one near checkpoint and one adaptive-field
step: both complete descendant families under ONE outer120s/512MiB supervisor,
plus their existing independent per-packet caps. Near uses4 bounded workers,
field Cholesky is sequential and BLAS one-thread. No builds or expensive saved
reference repetition during this combined phase. Retain both raw child packet
walls and measured aggregate wall/peak; combined wall is not the sum of parallel
child walls. Abort on aggregate memory/deadline, do not increase caps. This only
schedules existing unchanged numerical methods and gates, without physical
admission. Start with a single trial; preserve any refusal before further work.

Aggregate near/field trial034 PASS102.805s/403.16MiB family peak: field34
full337 solve remains numerically valid; spatial change2.72820% FAIL0.2%.
Near cache292929 off-diagonal rows, constructor incomplete. Predeclare adding
ONE independent reference packet to the same aggregate512MiB/120s supervisor:
saved reference peak41.7MiB plus controller overhead fits the remaining108.8MiB;
no independent cap is raised. Test one combined packet, retain any aggregate
refusal. The hybrid reference queue now persists the next failed ordinary pair's
DD choice across one-packet calls so it cannot repeatedly redo the same failure.

Three-lane aggregate035 REFUSES outer120s (120.095s,506.29MiB); preserve it.
Field35 completed numerically, full-space change3.09718% FAIL0.2%; near297312
rows retained with complete child provenance. Independent ordinary pair190030
produced only its header before aggregate termination; no row/state is admitted,
and its incomplete packet has no completed run.json. Do not repeat that known
ordinary wall failure: next attempt is controlled DD with the original strict
pair/refinement gates. Reference bookkeeping uses a fixed total-pair byte bitmap
instead of a Python integer set to bound controller memory as coverage grows.
No increase in any memory/time cap. Three-lane scheduling is not yet validated.

Three-lane036 completes113.423s/505.80MiB, including a properly retained strict
DD deadline tree for pair190030 (124 forward/2028 reverse leaves; no admitted
pair). All child run metadata exists. The full field36 has110107 dielectric
tets; a newly resolved weak source mode makes its full change43.9129%, still
FAIL0.2%. The aggregate is too close to512MiB for automatic growth; serialize
larger field/reference stages or use two lanes with measured headroom. Keep the
successful scheduling result separate from both unresolved physical admissions.

All30 runtime DSL names PASS21.769s/268.33MiB in fresh processes; declaration
compilation is verified separately as compilation. Worker packet contains18
completed named PASSes, then refuses512MiB in the placement/dispose integration
case; do not mark that final case passed or rerun placement for this CPU-only
analysis task. Frozen actual-ROI visualization was generated and viewed:
[actual via/return geometry and full-space failures](../../../debugging/p7-board-integration/clearance50-actual-via-progress-036-v1/actual-via-return-progress.png).
It shows original foil/barrels/finite contacts and the failing full337 witness,
not a successful physical model. Near301279/836160 off-diagonal rows retained.

Predeclare eliminating duplicated nested supervisory Python processes from
aggregate scheduling. Planning runs remain separately bounded and immutable;
then one outer120s/512MiB supervisor directly owns the three numerical families.
Keep per-child commands/input/executable/source hashes, conservative peak RSS,
raw exit/error/checkpoint files and delegated aggregate cap provenance. The same
native near100s, independent DD110s and full337 field gates remain unchanged.
No compilation overlaps a numerical phase. This is scheduling/provenance only,
not a new integral/field method or a physical acceptance certificate.

Direct supervision037 PASS113.786s/362.78MiB versus505.80MiB with nested
supervisors; bounded planning separately PASS11.958s/233.26MiB. Child source/
input/executable hashes and raw exits are retained with the aggregate provenance.
Near305416 rows; field37 full change3.36118% FAIL0.2%; independent pair190030
reverse tree reaches4096 leaves but still2.81765 times its unchanged tolerance,
retained on deadline. No reference row/physical admission. First aggregate result
includes a launcher JSON record before its final object; preserve v0, direct
comparison-launch output to its own log in subsequent packets. Use measured
complete family peak + projected field growth +64MiB explicit headroom for later
planning; refuse or serialize if that unchanged512MiB plan is exhausted.

Predeclare a frozen work-selection-only metric control before further adaptive
refinement. The fixed seed337 weighting can overemphasize already-resolved weak
modes; previous-matrix whitening formerly failed its1e-8 arithmetic guard.
Test a50-decimal Cholesky/inverse on the same complete337 canonical-upper C
(after the unchanged raw directed1e-9 gate), then round Q once to F64 and check
the ENTIRE Q^T C Q-I in50-decimal arithmetic. No projection, diagonal shift,
clipping, averaging or altered physical RHS/C. Preserve the old refusals. This
only proposes refinement priorities; physical0.2%/independent/domain gates
remain unchanged. Separate bounded control/preparation, original unit-coordinate
field solves remain untouched. Switch only if the full-rank arithmetic control
passes; do not rerun completed physical reference solves for this control.

Frozen latest full337 metric37 control PASS25.406s/116.41MiB. Raw directed
8.79964e-11 passes1e-9; Decimal50 factor/inverse plus full audit of rounded Q
has2.73133e-13 residual. Naive raw-matrix F64 whitening1.12769e-7 refuses1e-8;
canonical-upper F64 evaluation2.00491e-9 passes. Hilbert8 all-mode control
1.25053e-12 passes; indefinite/asymmetric inputs refuse. Use only this verified
full-rank Q for indicator priorities, keeping original physical RHS/C unchanged.

Aggregate038 later refuses120s (123.691 observed,366.13MiB); retain failure.
Near child completes105.039s with1704 flushed rows and reference child110.973s
retains a covering4096-leaf tree with strict estimate2.01270 times tolerance;
neither implies constructor/reference admission. Field38 has no complete result.
Read-only process observation found competing rg/editor/antimalware CPU activity;
no unrelated process was stopped. Serialize the first field38 retry, reusing its
prepared geometry; its previous attempt was interrupted, so this is a justified
retry. Retain complete unit-coordinate nodal solutions/C BEFORE optional flux
indicator postprocessing, with file/source hashes and unchanged numerical gates,
so later work-selection deadlines cannot discard an already completed337-RHS
solve. Existing successful field solves/references are not regenerated.

Serialized field38 retry PASS46.383s/222.66MiB using the verified full337
metric37 solely for indicators. Full-space change2.68058% still FAIL0.2%.
The complete337 unit-coordinate solution/C is retained at19.038s, before
indicator work, with exact nodal-array/source/coarse hashes. Audited recovery
PASS1.298s/24.41MiB retains1704 valid near rows from bounded105.039s child019
and the bounded110.973s covering reference state; aggregate038 wall refusal
remains intact and its incomplete field is not admitted. Near307120 total rows.
To leave CPU headroom under observed competing host work, the next aggregate's
near constructor uses2 existing bounded workers (same strict mathematics/cache
identity; four-worker default remains for legacy manifests). No deadline or
memory cap is raised. Compile this diagnostic-only option separately before
starting further numerical work.

Two-worker complete24-source control PASS1.412s/12.99MiB: original Cartesian
space, cold single/two-worker bitwise agreement and the saved independent
reference bound are retained. Diagnostic option build PASS21.18s; test compile
PASS3.94s. This adds one distinct focused regression (63 with prior62).
Continue with a verified *latest* full337 Decimal50 metric per new field step,
reusing an existing matching control when present. Metric preparation is its own
bounded packet; actual full C/physical RHS and acceptance gates remain unchanged.

Aggregate039 PASS106.174s/367.70MiB with2 near workers. The previously hard
independent pair190030 finally PASSES83.720s (all original strict pair/refinement
controls retained); full field39 remains FAIL20.1196%. Aggregate040 refuses
120s (122.562 observed/362.82MiB), so suspend concurrent field/reference work.
Its complete337 physical solution/C was safely retained at57.715s before the
interrupted indicator stage; do not rerun that solve. Preserve the ordinary
reference190031 header-only wall failure and use controlled DD next. Test
bounded retained-state indicator replay against frozen successful38: force a
partial face prefix, require wrong-metric state refusal, resume ALL faces and
require bitwise equality with the saved full scores. Then reuse the completed
field40 state. Every face/all337 source coordinates and physical accuracy gates
remain; partial work-selection data cannot drive refinement or admit a model.

Retained-state indicator control PASS: forced1024-face prefix4.617s/151.89MiB,
wrong-metric checkpoint rejects1.590s, correct resume22.776s/154.09MiB reproduces
ALL saved38 face/owner/normal/score arrays bitwise. No field RHS was recomputed.
Use this replay for the safely retained40 solution; original wall refusals stay.
Serialize field work from the near/reference phase to avoid host-contention
failures; each remains separately <=120s/512MiB. Use4 near workers when field is
absent. Planning may select the already-controlled positive DD reference upfront
for an original shared edge/face and end an ordinary batch before that pair.
This only chooses between existing independently checked algorithms on unchanged
source supports and strict tolerances; it avoids predictably expensive ordinary
failures and never skips/adopts a pair without its complete original gates.

Correction to the prior 'header-only190031' description: preserved ordinary
packets190031-v1 and190032-v1 EACH contain one completed strict reference row,
then stall on the following pair. The queue correctly sees those rows; current
first missing is190033, not190031. Preserve those successes and both enclosing
wall failures. Raw per-stage cold subtotals do not yet equal complete extraction
elapsed time, especially missing child metadata from interrupted aggregates.

Field41 PASS62.023s/262.10MiB, full change2.79584% FAIL0.2%; separate near/
reference phase still refuses121.429s despite only133.11MiB peak. The ordinary
reference can stay inside one order64 target loop beyond its inter-order clock.
Predeclare a reference-only deadline hook every128 unchanged target samples,
with exactly the existing target map/weights/addition order and bitwise controls;
reduce inner deadlines110->95s to reserve outer provenance/cleanup time. All
quadrature/reciprocity/accuracy/leaf/order limits remain unchanged. No unfinished
DD state is needed now:190029 and190030 completed, later failures are ordinary.
Keep old executable snapshot and successful numerical rows; no expensive completed
reference is regenerated. Compile separately, then verify the checked ordinary
rule against the existing primitive plus forced deadline rejection.

Checked ordinary reference control PASS3.799s/31.10MiB:9 actual-target
order2/4/8 results are bitwise equal to the existing positive primitive, and
forced deadline rejects. New executable is recorded in that packet; all prior
completed references remain immutable, and no unfinished DD tree needs migration.
Reference deadline now95s, checked within ordinary target quadrature as well as
DD/adaptive work. Source support, all numerical tolerances, cubature orders and
4096-leaf limits are unchanged. The independent reference manifest now flags
partial producers without child metadata and labels its raw wall subtotal,
preventing that subtotal from being misreported as full extraction cold elapsed.

Predeclare native replay of the SAME conforming P1 adaptive field method:
rebuild the saved seed from the actual board/request, require identical complete
Cu tetra vertex bits/materials/current contacts, then replay only recorded
edge-star bisections of existing edges. Inherit materials, verify positive child
volume/1e-10 partition, reconstruct finite-contact/P1 traces from source node bits
and midpoint ancestry, and compare all final node bits, tetra/epsilon and boundary
sets with the frozen reference geometry. Assemble a fresh native sparse field/
complete337 C through existing CapacitanceOperator/DenseField, then compare the
already saved independent SciPy result; no reference C values enter native
assembly. Keep all source potentials and numerical gates. No physical admission
from this numerical check alone.

Plan mesh replay temporaries separately from the subsequent sparse solve; after
those temporaries are dropped, count the actual retained mesh as live storage,
not its historical extrusion/replay peak. Preserve512MiB/120s and validate each
phase's full live bound. Test affine/finite-contact trace, actual full-source
charge/energy, wrong ancestry/geometry/material and insufficient-memory cases.
This is the authorized internal general field-trace extension, not a new solver
runtime or a separate DSL. The full physical/PCB/P6/DSL gates remain outstanding.

Predeclare batching the existing independent reference backends inside one
bounded native packet: try the same checked positive F64 target rule with a2s
per-pair work allowance, then use the already controlled DD positive-volume
method if the unchanged0.2%/1e-9 gates are unmet. One95s native deadline governs
the whole batch. Preserve every complete row; per-pair adaptive state prefixes
prevent collisions and support strict single-pair resume. No integral formula,
source support, tolerance, leaf/order limit or matrix gate changes. This avoids
idling most of a near/reference phase after one inexpensive reference pair;
verify finite batch coverage, raw gates, namespace/state identity and the legacy
bitwise controls before using it on still-missing pairs. Completed costly
references are never regenerated. Current next missing190645 has no unfinished
DD tree, so no state rebind is required for this executable-only routing update.

Native replay controls pass2 tests (affine/finite-contact trace, exact primitive
energy, preserved Cu and buried zero; wrong ancestry/source/trace/material/budget
reject), and4 legacy full-field tests pass. Native seed000 direct reconstruction
v0 refuses exact saved node-bit comparison2.869s/54.41MiB; preserve it. Cu/source
identity had passed. Suspect decimal intermediate transport rather than loosen
geometry equality: add optional authoritative u64 transport for node coordinates,
epsilon and P1 trace weights, consistent with existing exact source signatures.
Create that transport in the fresh evidence packet from the unchanged saved
mesh, with both original/transport hashes; no global JSON parser/placement/input
change. The native replay must still match every original point/cell/material/
trace exactly. Verify the cause with the fresh native run; do not assume success.

Tiny native F64/DD batch control PASS4.699s/57.78MiB: all6 pairs on three actual
unchanged tetrahedra, and three mutual values equal saved references exactly.
All original raw/refinement gates remain; physical admission is still false.

Predeclare a boundary-resolution audit/work policy for the same P1 method.
Frozen field seed diagonal C spans2.16499e-14..3.09918e-7F; field49 still reaches
2.98111e-7F. Do not call these physical accepted PCB coefficients. Audit the
mandatory tangential Dirichlet gradient on every actual Cu face and the adjacent
field-cell height: fixed trace implies a local P1 energy lower bound
EPS0*epsilon*volume*|grad_surface(trace)|². This can identify underresolved narrow
source basis supports without discarding any source/mode.

If supported by the frozen audit, grade existing field edge stars in the normal
direction using target min(globalStep,featureRatio/sourceTraceGradientFrobenius),
with exact original contact/P1 weights. Predeclare families(globalStep,ratio):
(2um,1),(1um,.5),(.5um,.25),(.25um,.125), plus tangential Cu-edge cap200um and
an independent100um control. Source gradient uses ALL original trace columns,
not a projection; constant contact faces retain the global step. Only work
selection changes. Original Cu/support/materials,1e-10 partition,100000 nodes/
262144 total tets/512MiB/120s and full3370.2%/independent gates remain unchanged.
Use the already native-built seed and keep all prior adaptive/uniform failures.
No C/P6/DSL admission is inferred from completing geometric grading.

Exact-bit native seed replay PASS24.730s/88.22MiB, independently matching all337
SciPy source columns (maximum relative column error1.573e-10, full-source energy
Frobenius7.043e-9). Decimal transport caused the earlier refusal; no coordinate
or parser tolerance was loosened. Numerical consistency only, no C admission.
Boundary audit seed finds2.035pm source feature against2mm adjacent cell height;
mandatory tangential-energy lower bound1.550e-7F explains the inflated diagonal.
Grading32 passes reaches93598 dielectric tets and lowers that bound to2.384e-12F.
Retain initial grading failure001-v0 and diagnostic002-v0: Windows NumPy integer
face keys overflowed32bit; enforce int64 and exact whole-face comparison, never
key equality alone. Mesh001-v1 geometry remains valid but its cost overlapped a
15s compile by mistake; do not use that packet timing for performance claims.

Some normal midpoint proposals still fail unchanged1e-10 exact partition gates.
Predeclare deterministic fallback dyadic interior parameters1/4,3/4 after1/2,
only when ALL incident field tets pass the same orientation/volume gates. Store
and replay exact fractions, inherit original trace with matching convex weights,
and compare every final node/material/boundary/source bit as before. Cu cells
and all337 sources stay unchanged. This changes work geometry selection only;
no volume, residual, physical accuracy or resource gate is relaxed. Controls
must cover quarter-point affine/contact trace, bad parameters and native replay.

Quarter-point field controls PASS (Python affine/contact/geometry controls);
native tests now include1/4,1/2,3/4. Grading003 reaches137265 dielectric tets,
27226 nodes,47 refinement passes, while the worst normal height/feature remains
65248: all three tested dyadic parameters fail the same exact-volume gate on
certain source-boundary edge stars. A bounded denominator16 audit preserves
this failure: edge2206/2249 and2465/2737 have no admitted parameter among15;
2724/2737 admits3/8 and5/8, so this is a coordinate representation issue rather
than permission to weaken the gate. Full337 independent field002 passes14.006s/
150.52MiB, residual2.170e-14 and charge-energy5.262e-9, but maximum diagonal
1.365e-7F and full-field convergence/physical admission are still absent.

Predeclare exact-dyadic field coordinate ancestry as the next local extension.
Original seed/current/Cu F64 vertex bits remain authoritative and unchanged.
Refined conceptual vertices are exact dyadic convex combinations, represented
internally by the already controlled two-F64 arithmetic; stored F64 positions
are explicitly rendering/transport projections, not rounded physical geometry.
Native coefficients use compensated conceptual coordinates. Independent reference
coefficients derive from original exact-rational seed vertices plus exact affine
child basis/volume transforms; no native coefficient input. Exact parent child
orientation/1e-10 partition, unchanged contact traces, all337 sources and all
resource/numerical/continuum gates remain. Keep rounded-coordinate failures.
Test a frozen actual offending edge against Fraction geometry, native/reference
coefficients, wrong ancestry/transport/coordinate-model and legacy controls.
Only use the new coordinate model after these primitive controls pass.

Exact-dyadic primitive controls PASS: independent Fraction source edge chain13,
120 checked cells, volume relative error2.084e-16 and gradients round identically;
wrong rendering rejects. Native3 tests PASS0.909s/2.59MiB including32 exact splits
of one actual thin parent versus frozen Fraction coefficients, while the same
rounded-coordinate replay refuses the original volume gate. Affine and finite
contact traces pass at1/4,1/2,3/4 under both coordinate models. No original Cu or
source-coordinate change; exact-field display positions explicitly projected.

Actual exact-graded000:48990 field tets,11354 nodes,6710 conceptual bisections;
independent full337 solve PASS8.902s/128.52MiB. Native fresh replay/assembly PASS
14.291s/138.52MiB. Full original source comparison PASS1.112s: max column relative
error3.241e-10 and full-source energy Frobenius3.804e-7. Numerical consistency
only; continuum C admission still false. Exact-graded001 reaches93513 field tets,
19546 nodes,32 work passes, worst normal height/feature468.635 (rounded route
had stalled65248). Preserve both routes and all refusals. This is progress on
the SAME P1 field/current method, not a replacement source-space projection.

Single original source323 diagnostic (same001 geometry, one RHS only, no full337
redo) PASS9.398s/171.88MiB and reproduces saved diagonal bit-for-bit. Its leading
field cell carries1.114e-7F: a non-boundary interior diagonal still connects Cu
vertices across the entire FR4 slab. Face-owner height grading never subdivides
that edge after its neighbor is refined, so the affine field remains forced
far from the very narrow physical trace. Preserve this explanatory failure.

Predeclare extension of work selection to ALL field edges incident to exposed
Cu nodes, excluding every edge already on a physical Cu boundary face. For each
incident Cu face, retain its all-original-column trace gradient and normal;
require |edge dot normal| <= min(globalStep,ratio/traceGradientFrobenius).
This includes interior Cu-to-Cu diagonals without refining the physical Cu mesh
or discarding source modes. Deduplicate coplanar normal directions, retaining
the strictest target. Continue exact dyadic edge-star splits with unchanged
material/contact/source/volume and512MiB/120s/count/continuum gates. Keep the
face-only route as failed evidence. Verify an interior edge beyond the owner
cell is selected; do not accept C from the selector's completion alone.

Cone-selection control v1 PASS11.895s/127.12MiB: the frozen interior source edge
is selected after3 nonconflicting-star passes; v0's stronger immediate-one-pass
assertion failed and is retained. Physical Cu surface edges stay excluded,
materials/source/trace and original Cu are unchanged. Cone000 adds16 work passes
to exact-graded000 and reaches roughly94k field tets, but its worst projection/
target is still8.15e7. This suggests excessive work from globally uniform Cu-face
subdivision, not a continuum acceptance result.

Predeclare a coarser WORK-family to resolve the narrow basis supports first,
using the SAME source mesh and exact P1 trace: (globalStep,featureRatio,tangentCap)
=(50um,1,2mm),(25um,.5,1mm),(12.5um,.25,.5mm),(6.25um,.125,.25mm).
Keep the original2um/200um family and failures. No physical gate is weakened:
all337 energy0.2%, independent agreement, domain/curve/scope controls and the
independent200um/100um tangential checks remain mandatory before admission.
The work-family is geometry generation only; coarse completion cannot admit C.
Reuse the exact same native seed geometry/current/DC and completed references.

Cone000 full337 reference PASS28.041s/176.95MiB; max diagonal falls from1.365e-7F
(face-only001) to1.454e-8F. This confirms the interior edge mechanism but still
fails physical convergence; do not call the smaller diagonal a physical result.
Continue the predeclared coarser work family on the identical native seed, under
separate source-cone prefixes, retaining all fine-work packets and all sources.

Predeclare memory optimization of native replay, before any mesh count change:
reconstruct independent command batches from the existing ordered ancestry.
A batch may reference only nodes present before it and at most one selected edge
per current tetrahedron. Scan the current tetra vector to prove those facts and
count all incident cells; split every such cell, retaining the same conceptual
coordinates, coefficients, trace and boundary checks. This replaces the global
edge-to-retired-cell BTree arena with two compact live tetra vectors plus a
bounded<=512-edge lookup. Preserve the legacy path and every final node/cell/
material/boundary/source equality gate. Recheck frozen Fraction primitives and
fresh full337 native/reference assembly. The512MiB/120s and current count caps
remain unchanged in this optimization; any later count-policy change requires
separate phase/live-storage evidence and never a byte/time-cap increase.

Predeclare priority correction for exact-coordinate work: rank dielectric edge
cones by the actual normal-projection/target ratio, without the former squared
alignment preference. The latter was useful for rejecting rounded midpoints but
can starve a strongly violating almost-tangential edge even after exact ancestry
removed that coordinate problem. Preserve the old work route. All geometry,
source spaces, work targets and numerical/physical acceptance gates stay fixed.
Continue from source-cone000 under a separately marked priority policy; retain
001/002 as work/cost evidence. No claim of C accuracy follows from this change.

Batched native replay primitive controls PASS3 (1.825s/7.91MiB). Geometry producer
now explicitly transfers ownership of in-memory old arrays after writing no
source files; default non-consuming helper stays unchanged. Consuming control
PASS14.489s/123.34MiB. Correct normal-ratio priority control PASS5.371s/111.01MiB
and reaches the actual problematic interior diagonal in the first pass.

Predeclare a factor2 highest-ratio work band: when very narrow supports dominate,
select only edges whose normal-ratio priority is at least half the current
maximum. Recompute ALL cones every pass; lower-priority work is deferred, never
removed or certified accurate. This avoids filling a512-edge packet with much
coarser unrelated work while only a few severe stars can be split independently.
All source/geometry/count/resource/physics gates and the final full-source h
checks remain unchanged. Record band width and selected-edge count explicitly.

Fresh native batched full337 assembly on unchanged exact-graded000 PASS13.111s/
138.69MiB; independent all-source comparison PASS1.238s (saved reference reused).
Replay plan drops95.84MB->51.94MB, but measured whole-family peak at this small
size does NOT improve materially; report the actual peak, no extrapolated speed/
memory claim. Field factor/parent transport are still part of the family peak.
Banded normal-priority ownership-transfer control PASS4.804s/100.14MiB. The
fixed source/contact/current geometry is unchanged; C/P6/DSL admission absent.

Worst-constraint diagnostics expose an unnecessary work constraint: Cu node2744
on the BOTTOM outer surface (+z) atz1.6mm was graded against a field edge below
that plane, outside the sidewall. Absolute/canonical normals erroneously demand
normal resolution into the Cu half-space there. Predeclare outward-only cones:
orient each Cu-face normal toward its adjacent dielectric tetrahedron and apply
only a positive projection from the specific Cu endpoint. Opposite directions
are left to the actual sidewall/bottom face constraints. The frozen FR4 interior
diagonal must still be selected. This removes an excessive work requirement,
not a physical accuracy gate; full337 continuum/independent checks remain fixed.
Keep both absolute-normal histories and their actual costs/refusals.

Outward-only actual interior-diagonal control PASS6.090s/100.73MiB. Banded outward
000 preserves every original Cu/source/contact and reaches84763 field tets,
16306 nodes,80 accumulated work passes,119970.55 worst ratio. It stays84.906s/
156.48MiB; many ordinary source cones remain deferred, so no completion or C
accuracy is inferred. Prior absolute-cone000/001 and their wrong-half-space work
remain recorded. Count and byte/time caps have not changed.

Outward000 independent full337 field solve PASS11.681s/165.68MiB. Max diagonal
2.768e-10F (source3232.751e-10F), versus prior face-only1.365e-7F; continuum
accuracy is still unaccepted. Preserve the source323 energy diagnostic in the
same packet; no additional full-source solve is needed for that evidence.

Predeclare extending the already deterministic near constructor to8 CPU workers
on this12-logical-CPU host. Keep compile jobs1, numeric120s/512MiB, the existing
per-worker allocation/stack bound and every integral/refinement/raw gate. Pair
math/order/checkpoint identity stay unchanged. Control the complete24-source
operator bit-for-bit against one worker and the frozen independent reference,
plus out-of-range and memory refusal, before using8 on still-missing pairs.
No completed integral/reference is regenerated. Default callers remain unchanged;
the evidence controller chooses8 explicitly to progress the retained full M.

Eight-worker focused Rust packet PASS all9 tests,3.980s/13.52MiB, including the
complete24-source bitwise/frozen-reference control, all72 exact coefficient
orientations, wrong cache identity, memory refusal and invalid worker0/9. No
ignored tests in that packet. Rebuilt the ordinary diagnostic board_volume
against the current library; no other P7 numeric process was active before
resuming saved missing near/reference work. Controller serializes the two
separately supervised120s/512MiB packets; no time/memory cap increase.

Outward work001/002/003/004 succeeds under caps, but004 stalls at ratio1025.34
with153246 field tets/28411 nodes/140 passes. The worst vertical edge from Cu
node2185 alternately approaches a1.02mm projection: another source vertex lies
on the same long physical Cu sidewall edge. Applying EVERY incident face's
normal target to this edge still overconstrains the corner; the sidewall trace
is prescribed along its full length and cannot decay from the end-face plane.
Keep these failures/costs; no h-completion or physical C is claimed.

Predeclare nearest outward-face work sectors. For each Cu endpoint and candidate
dielectric edge, find the closest incident actual Cu triangle on the field side
using plane projection/barycentric inclusion and clamped edge distances; retain
all ties. Apply the same full-source gradient/normal target only from that
closest face sector. This defers an end-face target when the point is next to
the actual sidewall. No source/geometry/trace/material/physics gate changes.
Retain the absolute/all-outward policies. The frozen interior FR4 diagonal must
remain selected, physical surface edges remain excluded, and full337 independent
h/domain/material/curve checks remain mandatory regardless of work completion.

Nearest actual-face control PASS11.449s/101.69MiB: required FR4 diagonal remains
selected, Cu surface edges are excluded and original Cu/trace/ancestry survive.
This is a work-selector control, not a board physical result. Coarse work has
not completed and no C/P6/DSL claim follows. Continue from outward004 under
new nearest-cone prefixes so the older policies remain reproducible evidence.

Nearest-sector work003 reaches255568 field tets/46259 nodes/177 passes,
95.786s/321.54MiB. Work remains incomplete (ratio19.4204). A frozen mandatory
Cu-face audit PASS4.166s/218.80MiB: maximal tangential-energy diagonal lower
bound3.283e-14F, worst face height/source-feature14.4206, minimal normal height
2.742e-17m. Thus the old huge lower bound is gone; some smooth-face work still
remains. Do not infer physical accuracy or source C convergence from this audit.
The nearest work queue is held between packets for numerical C/resource review;
this is only an internal queue marker, not a pause of the user's P7 assignment.

Nearest003 full337 independent solve REFUSES43.023s/391.62MiB: original1e-8
charge/energy gate measures1.311e-3. Preserve it; no C values admitted. Native
fresh replay reaches the exact same255568-cell topology, then refuses DenseField
memory planning14.033s/431.13MiB (operator assembly peak466.59MB was incorrectly
carried as live storage into subsequent all-source Gram materialization).

Predeclare two corrections within the SAME discrete problem and fixed caps:
(1) retain only free/fixed RHS couplings after Kff assembly, and count actual
retained operator buffers separately from assembly/factorization peaks;
(2) preserve both F64 parts of refined nodal potentials into the complete-source
energy Gram. The operator already solves/refines in nodal differences/two-F64;
DenseField currently discards its low parts before reconstructing that Gram.
Independent SciPy will refine the same equations using its independently written
compensated elemental nodal-difference action and compensated potential updates,
keeping the unchanged1e-8 residual/energy/charge and1e-9 raw reciprocity gates.
Use bounded source-column packets if needed; never omit any of337 columns.
Recheck analytic/finite-contact controls, legacy full-field regressions, and
native/reference agreement. No C continuum/P6/DSL admission from these fixes.

Native nearest003 v1 passes the corrected Rust preflight but the unchanged
supervisor REFUSES at543137792 observed family bytes (12.933s), preserving that
failure. The Python exact-bit transport producer retained a large parsed JSON
object while the native solve allocated its source states. Predeclare a separate
short-lived transport subprocess: its arrays must die before the native solver
starts, so the SAME conservative per-process-peak family metric/cap applies to
actual overlapping phases. Keep total wall120s/512MiB and original/transport
hashes; no metric or budget relaxation. No C/P6/DSL admission from this fix.

Corrected numerical controls PASS:5 full-field regressions (including a new
sub-ULP-potential Gram case),3 native replay controls, and independent Fraction
variational flux on33 actual thin children (reaction error2.81e-33; charge
balance2.71e-17; exact constant zero). Native nearest003 v2 now stays within
422080512 observed bytes but REFUSES120.095s wall. Preserve the timeout.
Independent source323 on that mesh PASS8.827s/390.48MiB: one compensated
correction reduces free reaction1.075e-9->8.035e-18, energy error2.911e-15,
C323=1.1100826942e-12F. This is one column, not complete337 acceptance.

Predeclare up to4 independent source-column workers in DenseField, default1.
Each worker uses its own existing sequential factor solve/variational refinement
and scratch; column buffers are disjoint, final Gram ordering unchanged.
Preflight max(assembly peak, retained operator + all high/low source states +
worker scratch, retained operator + source states + Gram tile), including bounded
thread stacks; keep120s/512MiB. Verify complete single/four-worker C bit identity,
sub-ULP energy, invalid worker and memory refusal before actual use. No source,
coefficient, solver tolerance or physical acceptance criterion changes.

Independent reference columns may stream only AFTER unchanged per-column raw
residual/energy checks. Bind every row to exact field/current hashes; assemble
all337 before global1e-9 reciprocity/SPD/native/full-source comparisons. Partial
packets remain partial and retain failures; source323's successful column is
reused, never integrated again for this unchanged geometry/backend.

Four-worker native controls PASS6 field tests1.294s/15.19MiB and3 exact replay
controls1.181s/2.59MiB, zero ignored. Native nearest003 v3 now completes ALL337
original coordinates on255568 dielectric cells:109.157s/419.84MiB family peak,
actual native elapsed105.308s, charge/energy5.391e-15, raw reciprocity8.701e-16.
The assembly/live plan is481696807 bytes; retained operator171406323 bytes.
Keep all v0/v1/v2 refusals. This is the first complete native numerical matrix
on this discrete grid, NOT physical h/domain convergence or real-board P6/DSL.

Independent streamed-cache corruption controls PASS1.211s/49.78MiB: truncated
last row excluded; incomplete/source-mismatched/duplicate/out-of-range/nonzero
structural-zero prefixes, raw asymmetric and indefinite full maps refuse.
First16 reference columns PASS54.178s/391.37MiB, unchanged120s/512MiB. Reused
successful source323 through import-field-column.py after exact geometry and
unchanged field_dyadic.py/field_variational.py hashes; retain original producer,
run/result hashes in import.json. No successful source323 solve repeated.
Continue missing columns only with prepare-field-columns.py; partial rows do
not constitute complete-C or physical admission. Full337 raw reciprocity/SPD
and native/reference checks remain mandatory before mesh convergence work.

Predeclare bounded concurrency for still-missing independent M pairs: launch up
to4 copies of the UNCHANGED reference executable under ONE120s/512MiB family
supervisor. Give disjoint missing intervals and pair-specific output trees;
resume pending trees with their original executable/hash. Merge only validated
complete rows, retain every refusal and hash all input/output states. No pair
math, uncertainty,1e-9 reciprocity,0.2% refinement, source space or caps change.
First control the frozen complete3-tetra source and malformed/overlap jobs.
This targets missing work only and never repeats completed expensive references.

Predeclare a fresh nearest-sector work history from the SAME immutable native
seed clearance50-adaptive-field-mesh-000-v0 after completing the current full
reference. This preserves all original Cu/current coordinates, finite contacts,
materials and exact dyadic rule; it avoids inheriting the previously rejected
absolute/all-face overrefinement. Keep the255568-cell history and its full
numerical comparison. This is a different internal work history, not a new board
or replacement circuit: all unchanged DC/M references remain reusable. Use the
same50um/1/2mm then25um/.5/1mm and finer WORK-family, count/node/120s/512MiB caps
and complete337 physical h/domain/curve/scope gates. A selector completion alone
still cannot admit C. Prefix clearance50-fresh-nearest-cone-field.

Nearest003 independent ALL337 merge PASS1.368s/70.39MiB: raw reciprocity/SPD
without shifts/projection. Native/reference comparison PASS1.382s/68.71MiB:
maximum column relative error5.3583085e-15; full-source whitened Frobenius
2.2294290e-11; largest energy error2.2275404e-11. Inputs bound to unchanged
original337 trace and1656 Cu cells. Reference stage subtotal1060.350982s over13
bounded packets plus reused source323; largest reference family410902528 bytes.
This subtotal is NOT cold full-board extraction cost. Expected90s internal column
packet boundaries retain partial prefixes and nonzero exits; they are not erased.

Preserved comparison outward000 -> nearest003 explicitly FAILS physical full C:
full whitened change789.230477, component0.152469408, DC-harmonic0.339390720,
all >0.002. These are different work histories, not an asymptotic h-pair; no
physical convergence is inferred from excellent same-grid numerical agreement.
Continue the predeclared fresh nearest history and unchanged final gates.

Parallel unchanged-reference transport control PASS1.097s/69.43MiB: all6 frozen
three-tetra pair records and self bits identical using3 processes, plus overlap,
already-completed, invalid interval and multi-pair-DD schedule refusals. No
reference executable/math changed, and no expensive completed board pair rerun.
Actual missing-job concurrency remains to be measured under ONE family cap.

Predeclare optional normal boundary quarter splits for subsequent work packets:
when a selected NON-surface normal edge has exactly one prescribed Cu endpoint,
place the existing exact dyadic point at1/4 of the edge from that endpoint;
Cu-to-Cu and tangential/surface refinement retain midpoint splits. This reduces
the normal projection by4 instead of2 per pass while preserving the same full
edge-star conformity, original sources, trace, material, count/resource and
parent-volume gates. Existing native exact ancestry controls already cover
1/4,1/2,3/4; add an actual-source selector/unchanged-Cu control before use.
Default half splits remain unchanged, and all prior histories stay immutable.
This is a work efficiency option only, not a physical accuracy relaxation.

Fresh nearest half-history003 reaches127272 dielectric tets/22972 nodes/125
passes,89.155s/199.66MiB, remaining work ratio234.317. Retain it unchanged.
Actual quarter-work control PASS4.207s/175.24MiB: exact same selected edges and
normal constraints,8 full edge-star quarter splits, unchanged original Cu,
metal faces and337 trace; full conforming/dyadic complex checked. Existing
native1/4,1/2,3/4 replay controls remain passing. Continue from this half-history
under distinct clearance50-quarter-nearest-cone-field prefixes; old files stay.

Quarter history003 REFUSES42.579s/320.28MiB at the existing262144 total-cell
preflight: last in-memory dielectric state258367 then next full star batch
exceeds the cap; remaining work is not zero. Last successful immutable geometry
is quarter002:241067 cells/42953 nodes/174 passes, ratio3.66121. Preserve failed
003. This proves a current method/count limit, not an external P7 blocker.

Predeclare exact phase-based sparse FIELD storage before any larger geometry:
replace duplicate16-per-cell Triplet/term materialization by one deduplicated
node-neighbour CSC pattern, compensated signed and absolute element sums, compact
free/fixed RHS couplings and canonical upper Kff. Preserve the identical P1
variational action and all numerical/physical gates. Reuse existing
sparse_potential symbolic exact-fill/ordering/numeric admission (no amalgamation)
before factor allocations. Check legacy finite-contact/analytic/gauge tests and
all337 against the already successful immutable nearest003 independent reference;
no expensive reference rerun. Keep current262144 geometry cap until complete
phase plans and bounded source-state storage support any explicit larger count.
Raw n/t/120s/512MiB safeguards are not silently raised. Source-state streaming,
if required, is a separate predeclared stage; no partial C admits P6/DSL.

Viewed actual-via-discrete-field-check.png from
clearance50-complete-discrete-field-visualization-v0 (3.388s/127.20MiB). It shows
actual Cu/barrels/four contacts, all337 diagonal energies, native/reference error
and the explicit FAILED spatial/unfinished M/P6/DSL gates. This is intermediate
numerical evidence, not the requested final physical AC visualization.
