# Explicit packages in the checked joint extractor

The automatic joint candidate retains original package pads and all their
physical copper islands. Explicit two-terminal R/C/L equivalents, including
authored series loss, are attached through the existing P6 two-contact weld.
External physical port order remains unchanged; private package ports are not
exposed as external excitations. Arbitrary imported one-port models are outside
this bounded path.

Native composition uses the actual recovery map to reconstruct original copper
and package coordinates. Each package supplies its complex terminal current to
the full copper residual. Internal package KCL/KVL, copper terminal/Joule power,
package terminal/Joule power and aggregate matched wave power are checked
separately. A pure capacitor has a floating difference-only stamp, with no
invented exterior ground. Geometry, full package resources and native identity
participate in the exact-grid cache key.

Review found and corrected a native/TypeScript property-order mismatch in
attachment metadata comparison. Cache revalidation also checks each package's
reported loss against its authored resistance and complex current; lossless
packages must report zero loss.

The isolated release analytic test passed for series RLC and pure C at 1 kHz,
including out-of-band and grounded-capacitance rejection. Six focused live
TypeScript tests passed for automatic geometry dispatch, private package pads,
distinct R-dependent cache keys, report validation and cache restoration.
Typecheck passed. The integrated native build passed in 10m12s. Addon SHA-256: `5967213e65a6b535112c9e6aa57d89550898fec3ae2fabda4e384753e59d919b`.
The exact integrated release analytic unit also passed. The actual-addon
board/package smoke passed in 3.125 s total, with 2.100 s
in native execution and 189,882,368 B peak private memory. The strict package
loss check passed. Artifacts are under `debugging/p7-joint-packages-20261104/native/`.
Physical case-matrix acceptance and coverage beyond exact 1–10 kHz samples remain open.
