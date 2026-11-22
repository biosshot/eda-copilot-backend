# Mixed prism/tetrahedron shared-return admission

The private grouped magnetic Gram now expands same-group and overlapping or
touching-AABB section pairs into their original tetrahedra. Strictly separated
groups retain the axial prism kernel. AABB overlap is conservative local
classification, not proof of physical contact. The original per-mode uniform
section choice and all source ownership/coverage checks remain in force.
Actual cache misses are counted separately for local section expansion,
separated section pairs and mixed section/tetrahedron expansion. The fixed
4-by-4 per-group cache and its admission bound remain.

The physical control contains two signals and one shared return, all three
owners represented by six proven slab prisms. Each original current mode retains
all six section supports, while nonuniform enrichment remains tetrahedral per
mode. Actual misses are 12 far section pairs, 81 local tetrahedron pairs and
108 mixed tetrahedron pairs: this is not an all-tetrahedron test with section
labels. R, C, incidence, physical ports and island constants match the original
volume model. Compensated island responses retain the original charge gate.

Both independent drives at 1 and 10 kHz pass the unchanged original-space
charge/KCL/KVL gates against the order-16 full-volume magnetic operator.
Maximum KCL is 2.23e-16, maximum KVL 6.425e-18, and maximum normalized complex
port difference is 2.715e-13. The deliberate 1 microvolt island perturbation is
still detected. No tolerance, residual floor or regularization is introduced.

Reference sensitivity is explicit. The projected full-volume order-8/16
inductances differ by 8.3202e-8 normalized by the largest order-16 entry
(absolute maximum 9.0835e-13 H). The same candidate passes the order-8 KVL check
at 1 kHz, but is refused at 10 kHz with approximately 5.50e-13 residual against
2.51e-13 to 2.62e-13 limits. This is not accepted as passing order 8, nor is
order 16 declared an exact physical solution. The initial order-8 control's
refusal is preserved in `debugging/p7-lazy-direct-local-far-initial-controls/`.

The final external release supervisor passes 37 controls under 120 s / 2048 MiB in
11.094 s (`debugging/p7-lazy-direct-local-far-final-controls/`), sampled peak
private/RSS 25,935,872 / 38,506,496 B, 20 ms lower bounds. The smaller grouped
control also compares the full tetrahedron Gram, reciprocity, positive energy
and per-mode representation.

The native addon build passes in
`debugging/p7-lazy-direct-local-far-native-build.log` (`BUILD_EXIT=0`).

This validates a small finite mixed prism/tetrahedron model at two frequencies.
It is not an automatic distributed 2D transmission-line/shared-return route,
a bandwidth extension, STM32 result or acceptance of every case-matrix family.
Remote return vias, cuts/bridges, multilayer and high-frequency admission remain
mandatory pending work; no source clipping is authorized by this result.
