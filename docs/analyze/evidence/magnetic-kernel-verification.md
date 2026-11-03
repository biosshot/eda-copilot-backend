# P7 magnetic kernel stability and full closed-current coupon

2026-10-24. Bounded implementation of `next-task-magnetic-kernel-stability.md`,
continuing §8.11. Baseline worktree commit`afe8ba3`; accepted C kernel unchanged.
No main merge, C/P6/Thermal/placement API changes, dependencies, public DSL,
board extraction, vias or commits. Analysis remains CPU/F64.

## Failure, cause and narrow repair

Frozen STM32G0_CAN_RS485 source supports668/556 retain every authored vertex
and35µm thickness. Their volumes are7.2130955722e-20/5.2021972093e-29m³.
Support556 has~8.24pm×.36pm XY dimensions against35µm z; support668 has a
~.559mm median and~8.24pm short edge. Uniform polyhedral potential is a small
positive remainder of opposing edge-log/face-solid-angle terms. Evaluating
these terms in F64 loses that remainder for556 viewed from668. Changing outer
Gauss order did not repair it: the original native formula again returns
`LOCAL_POLYHEDRON_RANGE` forward at XY12/z8 andXY24/z16. Reverse values remain
finite. Original sources/raw negatives/hashes are preserved, not overwritten.

`local/polyhedron.rs` now localizes source coordinates and estimates the analytic
sum conditioning. Resolved cases keep the face/edge expression; unresolved
cases use a positive source-XY integral with analytic source-z integration.
Inside-source fans retain exact radial integration. Outside points use an
original source vertex, so every integration weight is nonnegative. Stable
`log1p` z differences avoid subtracting nearly equal distant asinh values.
Orders7→15→31→63 must meet the explicit1e-5 source refinement estimate or refuse;
`at_diagnostic()` exposes the branch/order/change. This estimate/conditioning
indicator is not a universal certified bound; independent accuracy is separate.

The first candidate still refused slender self source refinement and missed
the neighbour target integral by~1.6%/0.42%. These failures remain saved.
Repair: split source-edge integration at the geometric distance minimum and
grade the full intervals with their Jacobians. Extreme target triangles
(edge²/twiceArea>1e6) cyclically select the shortest edge and grade the near-edge
parameter, retaining the whole triangle and exact volume/Jacobian. The1e-5
control and old0.2% integral gate were not loosened. No negative potential
clipping, regularization, physical resizing, support deletion or unknown-order
fallback is used. Existing invalid geometry/range/order refusals remain.

## Independent primitive references

Offline references import no native potential/formula. For same-z finite
prisms, integrate both z coordinates analytically:

`H(rho)=2h[asinh(h/rho)−(h/rho)/(sqrt(1+(h/rho)²)+1)]`.

Positive4D triangle-area integration uses independently scrambled Sobol powers
16/18/20, four saved seeds. Empirical uncertainty is`2*last change+6SE`; it is
statistical/observed, not a deterministic continuum bound. Problem pair reference
5.3494108459e-44m⁵ has0.001463% working uncertainty. Tiny556 self reference
2.5921181640e-51m⁵ has0.001608%. Both directions are independently checked.

Slender668 self QMC uncertainty0.36248% was **insufficient** for0.2% and remains
visible. A separate1D marginal convolution plus finite-width enclosure gives
9.0874542462e-35m⁵ ±1.3605132998e-39m⁵ (0.001498% to lower endpoint). For triangle
median lengthL and short-base widthw, the collapsed marginal density is
`g(v)=8/3−4v+4v³/3`. Pair displacement from that line is bounded byw. Split at
`v=2w/L`: far difference uses`|H'|<=2h/rho`; the entire near finite source strip
has area<=8Aw/L and a rearrangement disk upper bound. Near collapsed contribution
is integrated separately. Full derivation/components and MP50/80 numerical
change are recorded in`slender-self-reference.json`. Finite width is included
in the enclosure; native geometry is never collapsed.

| Integral | XY12/z8 signed error | XY24/z16 signed error |
|---|---:|---:|
|668→556|+.027172%|+.002221%|
|556→668|+.002814%|+.000531%|
|668 self|+.002648%|+.000223%|
|556 self|+.052832%|+.014032%|

Worst absolute-error-plus-reference-uncertainty screen0.054440%<0.2%. Rotation,
translation and scale2 (integral scales2⁵) preserve both directions within1e-9.
Healthy/touching finite box self, close return, separated/asymmetric/far and thin
cases retain existing independent analytical/FastHenry/box regressions. Signed
near-zero operator entries use absolute scale`.002sqrt(LiiLjj)`, predeclared
before runs, rather than division by a near-zero signed coefficient.

## Full operator, including a genuinely closed physical space

Supplementary four asymmetric finite bars include every self/mutual coefficient
of a4×4 partial-L and all **64 oriented prism pairs per rule** (two triangles
per bar). Independent box self difference-convolution/Duffy and smooth6D mutual
Gauss references verify full4D and all3D`sum(I)=0` terminal-current space. Refined
full errors1.65e-9/1.62e-9 are relative fractions; eigenvalues are positive.
This partial operator does not include exterior terminal closure fields, so it
is explicitly supplementary, not substituted for the closed-current test.

The main physical coupon is a finite1.5×1mm×35µm foil split into12 exact prisms.
Two continuous P1 streamfunction hats at interior nodes define
`J=(dpsi/dy,−dpsi/dx,0)/h`. Normal current is continuous at every internal face
and zero at every exterior face; maximum flux residual2.808e-17A/A. Hence all
linear combinations are compactly supported, distributionally divergence-free
3D currents, with no inferred external return or hidden ground. The **entire
two-dimensional physical space** is tested, not just one differential mode.
All144 directed prism pairs are saved/timed for every native rule.

Independent positive4D exact-double-z integration covers all78 unique scalar
pairs, four scrambles at powers12/14/16. Full-matrix last change0.133923%; maximum
scramble deviation0.076444%; predeclared working uncertainty screen
`2*lastChange+6*maxDeviation/sqrt(4*3)`=0.400251%<1%. This is empirical/statistical,
not a certified continuum bound. Reference C is not used; L is not inverse(C).

| Volume Gauss order | Full energy error | Raw reciprocity | Strict1e-9 gate |
|---|---:|---:|---|
|8|.045950%|4.78286e-7|FAIL preserved|
|12|.046479%|1.35853e-8|FAIL preserved|
|24, one justified control|.046500%|5.55247e-11|PASS|

The one order24 control was selected from the saved8→12 reciprocity failure,
recorded before launch with the same512MiB/60s limits, geometry, kernel and
independent reference. No further controls or tolerance changes followed.
Reciprocity is checked on **raw directed coefficients**, without averaging
them to manufacture a pass. Symmetric parts are used only for quadratic energy.

Final raw L in nH is approximately
`[[.3394866765,−.02377042925],[−.02377042927,.3394866765]]`.
Physical eigenvalues are`[.3157162473,.3632571058]nH`, both positive; full
normalized energy-error spectrum`[−.00046500026,−.00029137199]`. For every physical
current direction, native error0.046500% plus separate reference uncertainty
0.400251% gives **0.446751%<5%**. Raw matrices, all signed entries, scalar kernels,
spectra and all pair costs are retained. This validates the frozen current
basis, not arbitrary board geometry/current distribution or mesh convergence.

## Resources, regressions and remaining production blocker

Early bounded fixtures/scripts fixed rules, pair counts, memory/time caps;
closed reference preflight26.84M4D samples and256MiB envelope preceded its run.
Actual requests: legacy STM.079s/4.47MB (old128MiB cap); repaired primitive/bar
coupon.681s/4.33MB; independent primitive/box reference11.488s/195.72MB; closed
native.212s/5.29MB; closed reference3.747s/127.82MB; sole24 control.926s/5.30MB.
Every supervised numerical request stayed under512MiB/60s; repaired STM also remains below its
old128MiB observed-resource gate. The lightweight source enclosure took.247s;
its peak RSS was not separately sampled. No cap was raised.
Builds reused pinned dependencies with standalone rustc/codegenUnits1 or cargo
jobs1 in the isolated P7 target; shared addon/GPU defaults were untouched.

Focused Rust: existing local11/11, new magnetic3/3, then the newly added closed
full-space regression1/1 PASS; **15 total**. Existing green tests were not rerun
after the test-only closed addition. Source/geometry/nonfinite/order negatives
remain; first candidate and strict low-order reciprocity failures remain.
Saved-value PNGs`magnetic-stability/audit.png` and`closed/audit.png` were viewed.
Python syntax, source/input/output hashes and diff checks are recorded in the
final manifest. No browser controls are claimed.

**Dense full-ROI cost is still a blocker.** Measured oriented primitive mean
costs on the four-bar coupon: .496ms(coarse)/3.924ms(refined), before mandatory
two-direction/two-rule validation. Packed scalar-pair counts remain4,683,499
BandAmpV5;521,031STM;2,539,107TPS. Optimistic one-direction refined extrapolations
are18,377/2,044/9,963s respectively; full validation estimates41,403/4,606/22,446s.
These are preflight extrapolations, not full-ROI runs or certified timings.
No large ROI was launched and no production resource refusal was bypassed.

Next proposal only: use the existing bounded far moments to count near/far work
and plan total cost before any ROI solve; require an aggregate magnetic
quadratic-energy error bound in the physical current space, not only per-pair
remainders. Then independent geometry/current/field-recovery coupons would be
needed. No hierarchy, reduced model, service, public API, PCB or via integration
is implemented here. P7/P8/P11 remain open. Heavy slots were returned P6/Thermal;
stop for parent code review, without commits or new agents.

Tracked reproducible inputs/references/tools:
`tests/fixtures/analysis-local-extraction/magnetic-stability/`.
Detailed raw packet/manifests:
`debugging/p7-design-checkpoint/magnetic-stability/`.
