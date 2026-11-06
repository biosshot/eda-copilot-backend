# Mode-specific shared-return magnetic representation

The preceding [basis diagnostic](p7-hybrid-rlc-2026-11-06.md) showed that adding
magnetic current modes removed both eligible sections under an all-mode
uniformity condition. This change addresses that representation loss; it does
not establish original-space physical admission or production dispatch.

The private adapter retains each physical group as its original tetrahedra plus
an optional validated prism. A mode uses the prism only when its own density is
uniform within roundoff; nonuniform modes use every original tetrahedron. Adding
a nonuniform mode therefore does not remove another mode's section. Ownership,
slab topology and complete copper coverage reuse existing adapter admission.

For each physical group pair, a fixed 4-by-4 scalar cache reuses geometric
integrals across mode pairs. Its storage does not grow quadratically with the
number of groups. Symmetric group pairs are evaluated once, adding both directed
matrix contributions. Exactly zero density dot products skip integration.

Section/section terms use the finite-axial operator. Mixed section/tetrahedron
terms sum interactions with the section's original three tetrahedra. Thus a
shared bounding box, including adjacent triangular parts of one conductor,
does not require singular overlapping prism quadrature. This is a representation
choice inside validated groups, not extra physical copper. The original generic
Gram's duplicate and conservative section/event overlap refusals remain intact.

Admission accounts for live group currents, geometry and prism string copies,
matrix storage and integration scratch, with checked sizes and deadline checks.
The grouped route remains private and has no automatic board dispatcher.

Final-source release controls passed 10/10 in 5.04 s (5.25 s external wall), under
a 120-second supervisor and a 2048 MiB process-tree cap. Sampled peak tree private
memory was 14,213,120 B, RSS 30,994,432 B (20 ms sampled lower bounds).
Local evidence: `debugging/p7-modal-group-final-control/stdout.txt` and
`supervision.json`; compile log `debugging/p7-modal-group-test-build-final.log`.

Controls include a signal/return all-tetrahedron representation comparison,
signed nonuniform modes, duplicate/budget refusal, original magnetic controls,
the physical loaded R/L/C comparison and the ignored basis diagnostic. The
all-tetrahedron comparison uses shared tetrahedron kernels; it is representation
verification, not independent physical acceptance. On the real assembled control
after enrichment to six current modes, mode 0 retains both sections, whereas
the former all-mode selector reports zero sections. Loaded response error remains
1.6765232e-5; the original-space negative admission controls remain negative.

The final native addon build passed (`BUILD_EXIT=0`, release 6m32s; local log
`debugging/p7-modal-group-native-build-final.log`). No STM32 speedup, complete
physical admission or case-matrix promotion is claimed.

## Enriched physical admission and numerical limits

Three magnetic enrichment rounds retain six voltage and twelve current modes.
The original three uniform modes keep both prism groups; the nine corrections
use original tetrahedra. On this fixture the independent volume model admits
drive 1 at both 1/10 kHz through unchanged original-space checks. The mixed model
admits that drive at 1 kHz. Four 50-ohm loaded comparisons remain within 1e-3.
These are specific positive controls, not acceptance of the other drives.

The independent model's drive 0 refuses passive-island charge: at 1 kHz the
approximately 2.4e-35 C balance residual is compared with approximately 1.0e-34 C
scale. At 10 kHz the direct field evaluation gives residual approximately
5.4e-35 C while the projected energy matrix gives zero. This is cancellation in
an almost zero symmetric passive response. No arbitrary absolute floor or
production gate change has been introduced; robust arithmetic error accounting
remains necessary before treating this case as admitted.

For the enriched mixed model at 10 kHz, drive 1 still refuses KVL. Increasing
the complete grouped Gram order from 12 to 60 reduces the largest inductance
difference against the volume reference from 5.67960e-5 to 5.80186e-7 relative.
Nevertheless KVL changes from 5.22851e-13 to 5.44812e-13 against a limit of
2.50706e-13. Increasing quadrature alone does not resolve admission. Neither the
order-8 volume reference nor the finite-axial quadrature is an exact oracle.

The measured ignored diagnostic completed in 0.953 s under a 120-second external
limit; order-60 model construction took 0.440 s. Local logs are
`debugging/hybrid_enrichment_v3.log` and `debugging/hybrid_enrichment_q60.log`.

## Production integration remains separate

These prisms collapse layer thickness; they do not automatically identify long
longitudinal 2D transmission-line sections. Production mixed extraction follows
`mixed_conductor` → `foil_reduced`, not this private Basis adapter. It still needs
full-space magnetic actions for enrichment and KVL. A fast Gram alone cannot
remove that runtime. The next production work must accelerate those actions,
retain complete original current/return coverage and carry the same error
certificates, before a full STM32 comparison is meaningful.

## Complete trace-field experiment

The existing `DenseField::new` was tested on all six source potential coordinates
of the same corrected-halo mesh. Its matrix has 288 bytes; general storage is
quadratic and this experiment does not justify dense materialization for large
PCBs. The full trace charge/energy discrepancy is 1.296e-15 and directed
reciprocity discrepancy is zero.

Materializing the complete trace does not cure passive-island charge refusal:
both raw and dense field operators refuse drive 0 at 1/10 kHz through the
unchanged gate. Both admit drive 1 with the independent volume magnetic operator.
An intentional +1 microvolt passive-island constant shift produces
2.125005e-19 C and is detected by the charge check. Thus neither a different
storage representation nor a hidden charge floor is a demonstrated solution.

The order-60 mixed magnetic model with the dense field admits 1 kHz drive 1;
the other measured gates still refuse. The ignored diagnostic passed as an
expected negative experiment in 0.687 s; local log
`debugging/hybrid_dense_trace.log`. A subsequent combined external 120-second
control run also passed all eleven hybrid tests, including this diagnostic;
see `debugging/p7-persistent-near-final-controls/stdout.txt`. No production
electric operator, physical gate or case-matrix status changed.

## Bounded island-constant refinement experiment

A test-only experiment builds the complete three-island constant-potential
Jacobian from the original field operator and iteratively corrects only island
constants, preserving current modes and potential differences. The 1 kHz drive-0
charge check stagnates after eight corrections and remains unresolved. At 10 kHz
drive 0, the manual unchanged charge/KCL/KVL checks converge; maximum relative
complex port change is 2.39e-16. The already accepted drive 1 remains converged
at both frequencies. This is partial numerical evidence, not a general fix.

Two preliminary test runs stopped on an incorrectly normalized new projected
field-residual assertion. That quantity is not the assembled coupled solver's
matrix residual. The final diagnostic retains its values for inspection and
does not treat it as an algebraic acceptance test. Existing physical charge,
KCL and KVL limits are unchanged. The final experiment passed under the external
120-second/2048 MiB supervisor in 0.375 s; local output is
`debugging/p7-island-refinement-final-control/stdout.txt`.

No production correction is installed. A full-island Jacobian can require many
electric actions on a PCB, the 1 kHz case still fails, and assembled-system
validation after a successful correction remains necessary before admission.
