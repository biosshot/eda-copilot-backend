# Experimental mixed magnetic Gram → R/L/C bridge

The private adapter connects `hybrid_section_magnetic` to the existing
`volume_reduction::Basis` electrical assembly. It is not production dispatch.
The <=600-second complete STM32 extraction target remains unmet.

## Representation and safeguards

- Every physical copper cell must appear exactly once. Proposed section groups
  currently cover one axial slab, three tetrahedra and six prism vertices, with
  matching source ownership and conforming internal/boundary faces.
- A group becomes one section only when every retained mode has uniform physical
  current density within roundoff. Otherwise all original tetrahedra remain.
  Ungrouped cells remain tetrahedra. Invalid/duplicate ownership is refused.
- Section/event mutual terms and the physical shared return remain in one Gram.
  The raw Newton kernel is converted to henries by `1e-7` exactly once.
- `Basis::model_with_inductance` shares the existing resistance, incidence,
  capacitance, contact and island assembly. It validates finite dimensions and
  symmetry, and bypasses full magnetic actions for inductance projection.
- Adapter reserves include live basis vectors, full gradients, modes and matrix
  storage; deadlines are checked during mapping and integration. This restricted
  adapter is not yet a general CAD section producer.

## Physical control

Three separate 1 mm × 1 mm × 1 mm copper conductors (two signals and one return),
18 copper tetrahedra, six end-face contacts, sigma 58e6 S/m, are surrounded by
dielectric padding and an outer electric reference boundary. The first signal
contains section groups; the other conductors remain local tetrahedra.

Reference and candidate use the same harmonic Basis, electric operator and
charge-conditioned coupled solve. The reference magnetic operator is the
open-Newton volume-integral operator, not a finite-box magnetic surrogate.
Resistance, capacitance, incidence and conserved-potential modes match exactly.
The summed constant-mode capacitance is positive: approximately 4.958345175e-13 F.
Maximum inductance-entry difference, normalized by the largest reference entry,
is 5.679599e-5.

Four successful loaded comparisons use 50-ohm port admittances, two independent
balanced signal/return drives and frequencies 1/10 kHz. Maximum complex port
error normalized by the largest reference port magnitude per drive is
1.676523e-5 (unchanged comparison threshold 1e-3). This is not a per-tiny-port
relative error. Algebraic residuals satisfy 1e-8.

Crucially, all four unloaded candidate drives are still refused by the original
space KVL admission gate. KCL residuals are at most approximately 2.22e-16;
absolute KVL residuals are approximately 4.244e-8/2.227e-8 at 1 kHz and
4.244e-7/2.227e-7 at 10 kHz. No physical gate is relaxed. Same-basis agreement
does not prove that the basis is sufficient or that the hybrid operator alone
causes the refusal; independent-reference admission must be checked next.

An earlier fixture without dielectric padding put all mesh vertices on copper
and did not provide the intended external electric reference. Its large loaded
errors are superseded, not evidence against the coupled solver.

## Validation and limits

The three focused release tests passed in 40.34 s under a 120-second external
limit; the Gram phase is limited to 60 s and each solve to 30 s. Controls cover
units, uniform/fallback coverage, duplicate ownership, invalid inductance input
and the coupled comparisons/refusals. `volume_projection` integration tests
passed 2/2. Native release addon build is recorded separately in the local
`debugging/p7-hybrid-rlc-native-build.log` (completed release build and addon copy).

No STM32 rerun is justified by this private undispatched bridge. No runtime
speedup, frequency-band extension, automatic shared-return routing or physical
case-matrix acceptance is established. All case-matrix families remain pending;
remote returns and unsupported group geometries must retain physical coverage.
Next: identify intrinsic-basis versus operator contribution to KVL and enrich
the representation without weakening original-space admission.

## Independent-basis diagnostic

The ignored release control `physical_basis_enrichment_diagnostic` subsequently
ran in 0.187 s under a 120-second external limit. Local output is
`debugging/hybrid_basis_enrichment.log`. It uses the independent original
volume-integral magnetic operator, not the mixed Gram.

With the original three current modes, the adapter keeps two Sections and twelve
local tetrahedra. Original-space admission still refuses: at 1 kHz the two KVL
residuals are 4.2437623e-8 and 2.2267506e-8, essentially the same as the mixed
model. At 10 kHz drive 0 refuses island charge, while drive 1 refuses KVL at
2.2267506e-7. Thus the original basis itself is insufficient; the mixed operator
is not the sole cause of refusal.

One existing magnetic enrichment round adds three current modes. For drive 1,
KVL drops to 1.8258273e-11 at 1 kHz (scale 1.9486853e-5), and 1.8075257e-9 at
10 kHz; both remain above the unchanged relative gate. Drive 0 refuses island
charge at both frequencies. More importantly, all-mode uniformity makes both
section groups fall back: zero Sections, eighteen local tetrahedra. The control
stops there rather than declaring an all-volume enrichment a hybrid success.

Next implementation: select representation per mode, retaining Sections for
uniform modes and exact original tetrahedra for nonuniform corrections. Physical
ownership and all mutual terms must remain intact. This diagnosis does not
establish complete admission, speedup or case-matrix acceptance.

## Compensated island representation experiment

The ignored `compensated_island_refinement_diagnostic` retains the complete
source Field and independent volume magnetic operator. It evaluates charge as
the linear composition of the original field response and full-space island
indicator responses. Constant-potential corrections and their charge sums retain
compensated low parts instead of rounding the corrected potential back to one
f64. The same three-island fixture uses two drives at 1/10 kHz, at most eight
corrections and the unchanged 1e-8 component-relative charge gate, without an
absolute charge floor. Convergence is mandatory; port ownership is checked
against retained electrode DOFs and physical islands.

The initial diagnostic passed all four cases. The previously failing passive
island ratios fell from 0.235 to 5.41e-17 at 1 kHz and from 5.98e-4 to 4.40e-19
at 10 kHz in one correction. Original volume KCL/KVL maxima were 2.23e-16 and
7.46e-19, respectively. The maximum relative port correction was 3.19e-16;
this is computed from the compensated island corrections, not unchanged rounded
port values. A deliberate 1 microvolt island shift is detected (2.125e-19 C).

This establishes only the compensated linear-composition representation. The
rounded direct Field response differs by up to 1.93e-34 C; it is not asserted to
pass the same charge gate. This does not admit the q60 hybrid magnetic model,
provide a rigorous field error bound, or connect a production shared-return
route. The indicator actions and low-part state need budgeted transport before
production use; all case-matrix acceptance remains pending.

The final mandatory-convergence/ownership control passes under a 120 s / 2048 MiB
external supervisor in 0.594 s (`debugging/p7-compensated-island-final/`), with
sampled peak private/RSS 12,632,064 / 29,462,528 B (20 ms lower bounds).
