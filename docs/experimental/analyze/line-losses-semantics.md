# P7b loss perturbation: implementation notes and feasibility

Status: implemented for review. Reviewer-approved independent BEM references
close R1 and the explicitly substituted R2 criterion; historical Qucs, scalar
mutual and Johnson endpoint discrepancies remain documented. Interactive HTML
controls remain unverified due browser policy. This document does not declare
P7b accepted. See [verification](line-losses-verification.md) and
[review decisions and reference errors](line-losses-review-response-2026-10-23.md).

The convention is exp(+jωt), peak phasors. For the vacuum electrostatic basis
φ_j with signal j at 1 V, let q_sj be boundary charge density, including the
signed charge on each reference surface. The longitudinal current basis is
K_si = Σ_j q_sj (C0⁻¹)_ji. Indeed the vacuum TEM relation I=c C0 V and
H_t=c q_s give this cancellation of c. Thus the signal current integrals are
the identity and all physical reference currents sum to −1 per basis. Actual
dielectric 1 V fields cannot replace this vacuum current basis.

For conductor s, B_s,ij=∫ K_si K_sj ds [1/m]. Its contribution is
ΔZ_s=(1+j) sqrt(ωμ0/(2σ_s)) B_s [Ohm/m]. Therefore
R=Σ_s Rs B_s and Linternal=R/ω; Lexternal remains μ0 ε0 C0⁻¹.
The average power is ½ I* R I [W/m], not I* R I. Mutual terms retain signs;
PSD follows from Gram integrals and must be checked numerically. Both reference
planes are independently integrated, without assuming equal return splitting.

The bounded recovery uses vacuum FE Dirichlet reactions, which
are weak charge moments, and the lumped surface mass. One scalar current value
is shared at a sharp conductor corner, and the squared piecewise linear
current is integrated exactly on every boundary edge. No corner moment is
omitted, clipped, or assigned an artificial radius. This recovery conserves
integrated currents exactly but needs a separate mesh convergence test for
each signal/reference Gram matrix; capacitance convergence is insufficient.
The analytical plate and four finite-thickness section probes pass before
integration. Existing caps remain fixed; a difficult weakly coupled microstrip
requires the already-supported explicit 300000 node/1024 MiB admission budget.

The loss comparison was specified before acceptance and approved during review:
for each entry use |ΔB_ij|/sqrt(max(B_ii,previous B_ii) max(B_jj,previous B_jj)).
The self entries are ordinary relative changes. Both unchanged zero diagonals
give an exactly zero row; a nonzero change on a zero row cannot converge. Retain
every absolute matrix difference. In addition use the spectral norm bound of
the entrywise-absolute diagonally normalized difference matrix, so several
small entries cannot conceal a larger power-space change. Total even/odd
conductor and dielectric coefficients separately require ordinary relative
change ≤2%. Every physical signal and reference surface and every dielectric
region is checked independently in mesh and outer-domain comparisons. P7a's
1% and 0.001-self weak-mutual floor are unchanged. This numerical correlation
normalization does not excuse an incorrect mutual sign or replace a reference.

For region r, C_r,ij=∫ ε_r E_i·E_j dA, using the actual dielectric basis.
The sum restores C. G=ω Σ_r tanδ_r C_r; air has zero tanδ. This is the
first-order loss tangent perturbation; discarded terms include O(tanδ²) field
redistribution. Homogeneous tanδ gives G=ω C tanδ. Constant parameters apply
only to the explicitly requested band.

Modal z=R+jω(Lexternal+Linternal), y=G+jωC. The passive root
γ=√(zy), Zc=z/γ must satisfy γ/Zc=y and Re γ≥0. Stable propagation uses
q=exp(−γl), interface r=(Zc−Zref)/(Zc+Zref),
S11=r(1−q²)/(1−r²q²), S21=(1−r²)q/(1−r²q²).
This remains finite for an absorbing long line. All matrices use the same
orthonormal even/odd transform; differential/common conventions remain P7a.

The selected screening bounds are δ≤min(signal width, thickness, clear gaps,
each reference thickness)/10, tanδ≤0.05 and modal R/(ωLexternal),G/(ωC)≤0.05,
in addition to P7a quasi-TEM screening. These are conservative eligibility
guards, not an accuracy theorem. Numerical loss convergence remains ≤2%,
separately for mesh and outer domain; P7a tolerances remain unchanged.

Primary method references inspected:

- [COMSOL TEM/quasi-TEM guidance](https://www.comsol.com/support/learning-center/article/modeling-tem-and-quasi-tem-transmission-lines-21971)
  distinguishes PEC, surface impedance and volume conductor models.
- [Rautio and Demir, IEEE TMTT 2003](https://www.sonnetsoftware.com/support/downloads/techdocs/miccondloss_mar03.pdf)
  demonstrates side-current and edge-convergence sensitivity. Its supported
  geometry/data must be checked before treating any graph as an oracle.
- [Qucs single-line references](https://qucs.sourceforge.net/tech/node75.html)
  supply engineering single-line comparisons, not coupled-loss validation.
- [Kitazawa 1993, DOI 10.1109/75.222778](https://doi.org/10.1109/75.222778)
  is a candidate independent coupled stripline reference; reproducible numeric
  inputs/actual values have not yet been obtained.

The original fully specified published both-mode requirement was replaced by
explicit reviewer decision on 2026-10-23: converged two-gap BEM full-power/modes/
surfaces at5%, plus supplementary Johnson published odd-mode R with declared
nominal-thickness uncertainty. It was not met literally. BEM is not called a
published reference; the response records all raw discrepancies and decisions.

Additional reference search: Kuo & Itoh 1997, DOI 10.1109/22.557610,
[primary university PDF](https://ir.lib.nycu.edu.tw/bitstream/11536/735/1/A1997WM23200015.pdf),
was read. Its paired comparison is phase/effective permittivity; its loss graphs
are three/four conductors, so it is not a valid paired-loss oracle for this task.
The candidate 2005 PIER conductance paper is shielded and was blocked by human
verification when fetching the full numerical source. No data were invented
from either abstract or geometry-mismatched graphs.
