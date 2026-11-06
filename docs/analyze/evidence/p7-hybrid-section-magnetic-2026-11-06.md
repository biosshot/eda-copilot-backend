# Uniform sections and local 3D magnetic coupling

The private `hybrid_section_magnetic` module combines uniform triangular extrusion cells with original local tetrahedra. Each physical support appears once; real current-density vectors for all modes share that cell. The operator returns a raw density-weighted Newton Gram. For densities per unit current, multiplying by 1e-7 gives inductance in henries; mean magnetic energy for RMS drives is one half the inductive quadratic form.

Section/section terms use the finite-axial constant-profile primitive. Local tetrahedron terms retain the existing tetrahedral kernels. Mixed terms integrate a strict tetrahedron source potential over the finite section volume, retaining all mutual interactions. Mixed quadratures at orders n and n+4 must agree within 1e-8; tetrahedral source tolerance is 1e-12. Section order is 8..60; local tetrahedron order is capped separately at 16.

Before dense allocation, admission checks matrix dimensions, live input densities/identifiers, geometry storage and 512 KiB of source scratch. Deadlines are checked during validation, support-pair checks, mixed quadrature queries and integral traversal. A single source query owns its bounded internal cubature; this does not provide asynchronous interruption within that query, so external supervision remains necessary for strict process deadlines.

Duplicate identifiers and exact duplicate geometry are refused. Distinct tetrahedra with identical bounding boxes are not mistaken for duplicates. Section/event bounding boxes must prove disjoint interiors (touching boundaries are allowed); unresolved overlap requests a full-volume fallback. This guard is conservative, not a general intersection/coverage proof. No physical region is automatically dropped.

## Controls and measured scope

The test assembles two signal sections, one physical return section shared by both modes and a local tetrahedron. Reciprocity and positive energy are checked. Full-minus-sections-minus-event isolates a nonzero mixed mutual contribution; deleting only this contribution changes energy while retaining each region's self term.

Five release controls pass in 4.79 s (`debugging/p7-hybrid-section-control.log`): mixed/shared return, independent full-volume matrix reference, reverse-direction cross reference, axial splitting, and geometry/resource admission.

Release addon build passes (`debugging/p7-hybrid-section-native-build.log`). No full-board timing run is represented as verification of this private, undispatched operator.

| Numerical comparison | Maximum observed relative difference |
|---|---:|
| Complete Gram versus separate volume reference | 1.288950127883e-4 |
| Two-way axial split, complete Gram | 4.172724794888e-15 |
| Four-way axial split, complete Gram | 3.775322433470e-15 |
| Forward mixed integration, orders 12/16 | 7.738631566524e-15 |
| Reverse mixed integration, orders 12/16 | 2.579543855508e-15 |
| Forward/reverse mixed integration | 3.489971098629e-15 |

These are numerical comparison/refinement estimates, not rigorous global error enclosures. The separate prism reference may use conditionally analytic faces/edges or a positive-source fallback with tolerance 1e-5; branch counts were not recorded. Its role is an independent finite-volume control on this geometry, not an assumed exact oracle.

An initial order-24 split control hit its per-call 30 s deadline; the passing 2/4-way split control uses section order 8 and keeps the mixed convergence check. The matrix reference control uses order 16. No production acceptance gate was relaxed: the new operator is private and is not selected by the existing extractor.

## Remaining integration

The supplied current-density modes are magnetic test data, not a solved KCL-connected channel. Production needs an exact physical-support partition, interface current/charge transfer, a checked map from section recovery and local volume coordinates, electrical R/C assembly, original-equation admission, arbitrary supported geometry and a fallback for unresolved overlaps. All case-matrix families and remote return paths remain required. There is no new full-board performance claim or completed ten-minute extraction in this evidence.
