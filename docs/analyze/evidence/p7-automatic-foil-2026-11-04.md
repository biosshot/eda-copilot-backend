# P7: automatic finite-plane extraction, 2026-11-04

Status: implementation integrated; whole-case physical acceptance remains open.
This extends [the internal foil operator](p7-foil-reduction-2026-11-04.md).

## Production contract

The ordinary channel planner can select one joint finite-copper foil domain when
captured return copper continues laterally beyond its section corridors. Selection
uses geometry and physical islands, without fixture names or electrical net-name
joins. The candidate closes over complete intersecting copper islands and keeps
the original finite electrodes. Native ownership checks reject clipped copper.
The original section path retains its domain, spatial, cut and energy checks.

An alternating stack of 2–8 copper layers is structurally admitted. This does not
establish physical accuracy for every layer count. Vias, plated holes and package
attachments still need the mixed-volume route and are not silently flattened.
The single-sided substrate boundary is not implemented.

The native endpoint prepares one shared electric/magnetic operator and enriches
one common basis across all requested matched Norton columns. It checks the final
basis against every column's unreduced KCL/KVL (1e-8) and wave/copper power balance
(1e-12 W absolute plus 1e-8 times the summed power magnitudes). Reference shunts
are excluded from intrinsic copper loss. A maximum of 16 total enrichment steps,
16 physical ports and 256 frequency/port columns bounds this endpoint.

Its current applicability is 1–10 kHz. It returns exact S samples with the original
paired contacts and references, not an admitted continuous-band or temporal
model. Standalone P6 replay is allowed only at those points and references;
interpolation, reference changes, composition and SI consumption are rejected.
The resource retains the physical pair convention and SHA-256 bindings to the
exact board and geometry. Replay provenance is bounded by hashes.

Results are `partial`, with conditional validity and explicit `unverified`
spatial/domain/band coverage. Residual success alone does not close a case-matrix
row. Native memory estimates are separate from measured process memory.

## Focused verification

- Native two-port shared-grid test: all four columns at 1 and 10 kHz passed;
  maximum KCL 1.11045e-16, KVL 3.24684e-10, wave/copper discrepancy 3.38878e-15 W.
  Three enrichment corrections; 14 prisms; planned storage 12,618,071 B.
- The live release suite passed the three existing harmonic/enrichment/grid
  tests. The added four-layer test first exposed a test provenance error:
  actual filled-polygon paths must be checked, not their zone outline paths.
  After that test-only fix, a debug run exhausted 300 s during enrichment;
  it is not a passing validation. The focused release rerun passed in 66.02 s:
  four-layer provenance, five separate physical components, positive component
  capacitance, reciprocity, the analytic DC loop and unreduced AC KCL/KVL.
- The automatic-planner boundary test passed with a substituted native report
  and actual P6 replay. It rejects malformed residual/power evidence and verifies
  exact coordinates, original contacts, producer hashes and replay restrictions.
- TypeScript typecheck passed after integration.

The release addon build passed in 6m21s, SHA-256
`e851a2a8ac1c639997677c5e886e6a4a4b1b2305e3a849ea49f9e1a89b72609d`.
The first actual-addon automatic bend attempt refused at the unchanged thin-foil
cell-altitude screen. It took 1.891 s, with sampled process-tree private peak
215,609,344 B and RSS 180,432,896 B. It used a 1800 s / 3072 MiB native budget
and 4096 MiB tree limit, one thread. Artifacts are in
`debugging/p7-auto-foil-20261104/native-first/`.
The authored bend has constrained 19.509 micrometre chords, below four times its
5 micrometre copper thickness. The source geometry and guard were not modified.
This requires retaining locally volumetric cells in the mixed operator; it is
not numerical acceptance of the foil approximation.

The separate manufactured `finite-foil-witness` automatic control passed. It
reuses the native foil coupon's physical copper and electrode contours, with an
authored track fully inside existing copper to exercise route selection. A
geometry-only comparison confirmed unchanged physical unions and pad contours.
It is not a replacement or correction of the refused bend above.

The real DSL/native producer and exact 1 kHz P6 replay completed together in
162.219 s supervised wall (161.260 s runner). The 1/3 kHz, two-port producer had
46 prisms, planned storage 13,343,893 B, maximum KCL 1.66542e-16,
KVL 8.93594e-10 and wave/copper discrepancy 5.61682e-15 W. Sampled process-tree
private peak was 216,391,680 B and RSS 180,887,552 B. The supervisor samples every
20 ms, so peaks are sampled lower bounds. The preceding native budget/one-thread
settings were retained. Artifacts: `debugging/p7-auto-foil-20261104/native-witness/`.
The positive control returns `partial/unverified` extraction and complete
exact-point replay; independent spatial and broadband accuracy are not inferred.

Typecheck and package build passed with the joint group dispatch integrated.

Reproduction entry point: `scripts/verify-p7-auto-foil.ts`, with
`P7_AUTO_FOIL_COUPON=finite-foil-witness`, `RAYON_NUM_THREADS=1`, and a new output
directory. Run it through `scripts/verify-p7-closure-supervised.py --memory-mib 4096`
using `P7_SUPERVISOR_TEST_COMMAND=["node","--import","tsx","scripts/verify-p7-auto-foil.ts"]`
and `P7_SUPERVISOR_TEST_TIMEOUT=1810`. The default fixture remains the refused
finite bend; both inputs are preserved separately. These are developer controls,
not an automatic expensive unit-test suite.

No real-board, RF, spatial convergence or full P7 acceptance is asserted here.
