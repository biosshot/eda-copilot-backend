# P7-D1 magnetic closure — working evidence

2026-11-01. Continuation after `c2fef93`; D/P7 remain open.

## Independent localization before the next native change

The fixed slot joint model was regenerated after conservative-current correction in `debugging/p7-d1-cross-coupling/joint-current-fixed-2/`. Patch1→2 remains 10.2046289%, domain remains 0.1116198%, and the old hybrid comparison remains 11.0418663%. Thus the loaded-current repair does not change the magnetic defect.

A new diagnostic contracts the independent constant-density Newton volume integral with the actual saved loaded AC currents at 1 kHz. Its quadrature order 8→12 changes the two balanced integral energies by at most 3.6e-6 relative. Against this independent integral, patch1 FEM self energy is low by 19.20%/20.59%, patch2 by 7.04%/8.98%, and the existing historical `[6,8,16,3]` configuration by 5.27%/4.51%. Current geometry and direct-integral energies are unchanged across those profiles. This localizes the error to the magnetic potential space, not the port observable or the recovered current.

The next bounded diagnostic is one source-resolving degree elevation, trace8/body8 with the existing exterior order16 and three vertex stars, evaluated against the same independent integral. It is not selected by joint/hybrid agreement and no tolerance is changed. Existing production patch levels remain unchanged during this diagnostic. A higher-degree candidate is not accepted unless its independent self-energy error and subsequent spatial/domain gates meet the preassigned 2% criterion.

After a converged joint reference exists, derive only cross-block magnetostatic mutual terms from disjoint physical current supports. Do not add another plane self-energy or calibrate against the hybrid result. Section distributed loop currents must include both the signal prism and its physical reference return; local currents use the saved per-tetra basis. Cross terms must be symmetric, passive in the assembled matrix, and tested for reciprocity, translation, distant decay and zero double counting.
