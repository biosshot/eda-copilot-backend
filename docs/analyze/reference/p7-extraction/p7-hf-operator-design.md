# P7 HF operator design checkpoint — 2026-10-29

This is an implementation prerequisite, not HF acceptance. The current local
operator remains certified only conditionally in 1–100 kHz. Removing its band
guard cannot implement this design.

Use the existing CPU/F64 FEM and P6 physical ports. For a locally thick, smooth,
good conductor, with exp(+j omega t), the half-space impedance is
`Zs=(1+j)*sqrt(omega*mu/(2*sigma))`. On the exterior metal surface assemble the
complex symmetric bilinear form `integral Zs K_i dot K_j dS` for unit-current
surface basis fields. Its real part contributes R; its imaginary part divided
by omega contributes internal L. Retain the existing external magnetic L and
electrostatic C. Do not add volume DC resistance to the same current support:
volume and surface descriptions are alternatives for that support. Recover
regional RMS Joule loss from `Re(Zs)*|K|²`; preserve source and port orientation.

For sections the existing weak-loss surface perturbation remains the fast path
when its geometry and perturbation checks actually apply. Arbitrary conductor
sections require the full conductor mapping and matrix field solutions; modal
formulas assuming an equal symmetric pair cannot be generalized by relabeling.
For local via/pad surfaces, the present volume-current unknowns do **not** form
the required surface-current trial space. A conforming tangential surface-current
basis, continuity/contact constraints and an external magnetic operator on it
are a concrete missing implementation. Existing surface-loss code alone does
not supply that operator. This note does not authorize selecting a new backend.

Use volume current diffusion for the transition regime, resolving skin and
proximity effects with mesh checks. Accept switching to a surface operator only
after an overlapping observable comparison, including port response and regional
loss, against that volume solve. A candidate screening ratio is skin depth below
one tenth of local conductor thickness and curvature scale; it is a screening
criterion, not an accuracy certificate. Thin foils/edge crowding require their
own overlap check. At DC use the existing finite-conductor R limit; never
evaluate the half-space expression as a DC model.

Dielectric conductivity/loss enters the complex electric operator, with material
dispersion and its validity band supplied by resources. Do not freeze C and add
an arbitrary scalar G when dielectric field participation changes appreciably.
All operators share physical terminal/reference topology. Build a passive causal
P6 resource and verify time-domain energy/initialization before granting transient
applicability; a table of unrelated frequency samples is insufficient.

The bounded frequency planner must include requested DC, logarithmic seeds,
independent mid-band holdouts, skin-transition overlap and resonance resolution.
It must stop at the shared cold deadline/memory/solve caps. Reuse immutable
geometry/factors where the operator is identical; each job owns RHS/work state.
An HF factor is not identical to a DC factor merely because geometry is equal.
Cache identity includes operator/material versions, topology, verified band,
temperature and validation status. Broader-band reuse requires matching all of
those and an actually validated response representation.

Independent reference ladder: analytic coax DC/skin impedance and charge/delay;
multiconductor section matrix references; volume/surface overlap on smooth local
coupons; actual RF board channel with finite local discontinuity. Predeclare
absolute floors near response zeros and assess the assembled observable error
within 5%, not separate component errors each within 5%. Electrically large,
radiating or multimode discontinuities remain P13 cases. No 10 GHz whole-board
claim follows from a uniform-line check.

Sources checked 2026-10-29: [COMSOL TEM/quasi-TEM](https://www.comsol.com/support/learning-center/article/modeling-tem-and-quasi-tem-transmission-lines-21971)
describes impedance/volume regimes and physical port choices;
[Ansys transition RL](https://ansyshelp.ansys.com/public/Views/Secured/Electronics/v242/en/Subsystems/Q3DExtractor/Content/Q3D/TransitionRegionRL.htm)
describes extra transition solves inside/outside conductors. They motivate the
design and do not validate this repository's numerical accuracy.
