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
