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

The final native addon build remains pending. No STM32 speedup, complete
physical admission or case-matrix promotion is claimed.
