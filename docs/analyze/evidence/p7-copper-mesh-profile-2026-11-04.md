# P7 copper through-thickness mesh profile

Implementation scope: audited local-volume preparation, CPU/F64. The profile
uses the nonmagnetic conductor skin depth divided by three as a vertical mesh
screen. Activated copper layers share the same vertical planes in electric and
current meshes. Dielectric and air spacing follow the existing rules. Ordinary
preparation without a frequency audit retains its prior mesh.

The effective upper frequency is the maximum of the requested audit endpoint
and actual native training frequencies. Training still uses 10 and 100 kHz;
therefore a 10 kHz request can require copper resolution for 100 kHz. Requested
and effective upper frequencies are reported separately. The training list is
shared with holdout exclusion rather than duplicated in the mesh planner.

Actual planar node/triangle counts and planned vertical slabs bound extrusion
nodes, tetrahedra and bytes before new planes are appended. Existing detailed
field/factor budgets remain authoritative. The model identity distinguishes
audited mesh policy/version/requested upper frequency; native core identity
also changes with implementation. Independent audits remain cold request-scoped
evidence, not reusable validated-band cache entries.

This is a mesh screen, not skin-effect or high-frequency acceptance. Lateral
edge/proximity and barrel radial resolution remain unverified. Applicability
has not been widened. No new coax refinement campaign was run.

The isolated actual Rust crate compiled in release and passed its focused mesh
test (material/frequency scaling, copper versus dielectric, generated planes and
pre-allocation refusal). A focused TypeScript identity test passed. The shared
training-frequency correction compiled in the integrated release addon build
(5m09s), along with the mixed conductor changes. Addon SHA-256:
`2ef413381454d868c01649d68360cd24ad371c2a63544c06914409001fc246a7`.
This build is not a physical skin-transition comparison.

Subsequent profile v3 also subdivides round barrel metal radially. It uses
interpolated constrained rings and spokes between the existing canonical inner
and outer contours, with target spacing delta/3 at the same effective upper
frequency. Material and contact ownership still use the original contours.
Constraint work is bounded before insertion; ordinary CDT/extrusion memory
checks remain in force. The profile records shell counts and maximum spoke
spacing separately for current and electric meshes. Non-audited requests are
unchanged. One geometry-only test passed for two shells, preserved contours,
frequency scaling and budget refusal. The integrated native build passed in 10m12s. Addon SHA-256: `5967213e65a6b535112c9e6aa57d89550898fec3ae2fabda4e384753e59d919b`.
Both focused profile tests passed in the actual release crate (0.00 s test
runtime after 2m54s compilation).
Tangential/proximity resolution and physical barrel AC accuracy remain unverified.
