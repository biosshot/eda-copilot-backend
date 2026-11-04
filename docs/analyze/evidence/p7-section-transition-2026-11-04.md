# P7 W6 internal section transition, 2026-11-04

The internal CPU/F64 cross-section diffusion solver is integrated in `section_transition.rs`, with the conforming polygon mesh handoff in `polygon_section.rs`, a bounded grid and TypeScript singleton handling. The finite transverse Neumann domain is part of the model. Its magnetic result does not certify free-air or exterior-domain convergence. The numeric simplicial LU uses a conservative worst-case LU fill and scratch budget.

The operator reports canonical resistance and magnetic inductance energy Grams. The magnetic Gram is the full inductance of this finite model. Adding a C0-derived or self inductance would double count energy. Optional recovery evaluates axial Jz in the original P1 mesh. The existing PEC and public applicability paths retain their previous scope.

| Control | Result |
| --- | --- |
| Two slabs, skin depth equal to thickness at 4367.292395999 Hz | Z error 0.0167505%, copper loss error 0.000822112%, gauge discrepancy 1.06816e-13 |
| Three conductors | Gauge 4.23575e-15, reciprocity 2.63820e-16, power 1.633e-15, recovered current discrepancy 3.70890e-16 A, owner Gram 3.94901e-16 |
| Polygon mesh and grid | 38 nodes, 56 triangles, 0/1/3 kHz, combined allocation plan 1,285,023 B. Three-conductor output 3,072 B |

Four focused release library tests selected by `transition` passed. Compilation took 3m13s and reported test execution was 0.00 s. Five TypeScript singleton/section regressions passed in 26.866 s. The integrated addon build passed in 10m03s. The PEC `radial_multilayer_coax` integration regression passed (1/1, 0.04 s, compilation 4m41s). Typecheck and package build passed (JS 169 ms, DTS 10.418 s). No whole-channel run was performed.

The checks establish an internal section mechanism and bounded controls. Public endpoint coverage and frequency bands remain unchanged. Local/barrel AC, common-band channel accuracy, independent domain checks and P7 acceptance remain open.
