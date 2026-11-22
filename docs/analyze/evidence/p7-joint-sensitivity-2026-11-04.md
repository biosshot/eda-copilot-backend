# Optional joint-domain sensitivity

`P7_AUTO_FOIL_AUDIT=1` enables two bounded comparisons of an automatic checked
response. Normal extraction performs no extra solves. One comparison halves
the maximum XY triangle area; the other doubles the exterior air margin.
Each starts from the same baseline, preserving physical copper ownership,
barrels, finite electrodes, materials, package models and exact frequency grid.
Changing mesh can legitimately change the foil/volume classification.

All work shares the original deadline and memory cap. Exact-grid cache entries
remain independently keyed and validated. Refused or expired comparisons are
recorded as unverified and are not retried. The baseline response is retained.

The sensitivity metric compares each complex S50 column using its variant L2
norm with a 0.01 floor, and each copper/package loss using the variant loss
with a 1e-18 W floor. Evidence records absolute changes and denominators; small
outputs must not be interpreted as independent relative-accuracy proofs.
Changes at most 2% pass this sensitivity check; above 2% through 3.5% require
review; larger changes remain unverified. These thresholds never relax native
KCL/KVL or power gates.

The focused mock test passed, including total budget accounting, classification
changes, review/refusal states and propagation into result diagnostics. No
physical refinement was run for this change. This is not full 3D mesh
convergence, independent accuracy or continuous-band verification. The joint
closed-copper model has no internal partition cuts to shift. Spatial, domain,
energy and frequency coverage remain unverified even if both comparisons pass.
