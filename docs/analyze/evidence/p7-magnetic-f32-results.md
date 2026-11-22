# P7 magnetic F32 experiment — rejected

Status: rejected on the small control. P7 remains open. Production remains F64; no F32 precision switch, factors, solves, cache metadata, or failed runtime branch is retained.

## Parent review

Verified the saved patch, paired logs, unchanged production sources and allocation figures. This rejects the tested one-shot F32 exterior solve, not mixed precision in general. The exterior construction calls `solve` directly for nine right-hand sides before the existing per-application correction loop can run. Its reciprocity threshold is `1e-9 * max(exterior gram)`. The experiment does not log the offending antisymmetry or original-matrix residual, so neither the magnitude of the failure nor final physical error is established. A possible follow-up is bounded F64-residual correction using the same F32 factor at this construction step, retaining the strict acceptance guard; it has not been tested or authorized by this report.

The measured process peaks and wall times compare a completed F64 run with an early-failed F32 run, so they are not an accepted end-to-end memory/speed reduction. Only same-stage factor storage (50% less) and the numerical-stage allocation estimate (13.73% less on this small fixture) are directly comparable. No unknown reduction was implemented and the small fixture's air-DOF proportions cannot be assumed for STM32.

## Fixed experiment

The unchanged compact-v1 small magnetic-energy fixture uses air degree2, near-copper trace4, copper cell-private degree4, exterior quadrature16 and one copper vertex star. Geometry, copper, current space, RHS construction, ordering and all strict numerical guards are unchanged. The existing independent saved `tetra-space-reference.json` supplies the three-cell energy reference; no reference was regenerated. This is not the real STM32 channel.

An experimental patch used F32 Cholesky factors and triangular solves, with assembly, original sparse coefficients, compensated residual evaluation and bounded correction, exterior/reduced matrices and transport in F64. The capacitance wrapper continued to call the F64 implementation. Only one full numeric factor was retained. F32 conversion admitted an additional CSC with independent usize pointers/indices and four-byte values, while preserving the original F64 CSC. F32 workspace came from the faer scalar-specific scratch queries; solve conversion was covered by the existing conservative per-node reserve. The unchanged exterior Schur construction still enforces its original directed reciprocity tolerance. No new correction scheme or relaxed guard was introduced.

Reproduction patch and logs are in ignored `debugging/p7-magnetic-f32/`. The patch is evidence, not production code. Test executable SHA256: `EF35F50E69046D1C2CA110502F6B96A987FE86FBD70596054ADD7254078C1F82`. Run the existing `volume_galerkin` ignored test `p7_memory_profiles_frozen_energy` with `P7_ENERGY_PROFILE=4:4`, `P7_ENERGY_AIR=2`, and experimental `P7_MAGNETIC_PRECISION=f64` or `f32`. The matrix bit-pattern audit, including column pointers, row indices and F64 values in little-endian FNV1a64, is identical: `6ef47441cf426f93` (an audit fingerprint, not a cryptographic digest).

## Result

| Quantity | F64 | F32 experiment |
| --- | ---: | ---: |
| Free potential coordinates | 3644 | 3644 |
| Upper CSC nnz | 60303 | 60303 |
| Exact simplicial fill | 387209 | 387209 |
| Stored supernodal values | 457070 | 457070 |
| Numeric factor bytes | 3656560 | 1828280 |
| Queried numeric/solve workspace bound bytes | 1027327 | 786111 |
| Numeric-stage bound bytes | 9592000 | 8275300 |
| Measured peak process RSS bytes | 15441920 | 13000704 |
| Measured peak process private commit bytes | 14487552 | 12521472 |
| Supervised process wall seconds | 22.6089953 | 15.5041652 |
| Test exit | 0 | 101 |

These are sampled process metrics at10ms, separately from conservative native allocation bounds; host scheduling affected wall times. They do not certify speedup. F32 numeric-factor time was0.022374s, versus F640.102019s in the preceding uninstrumented new-binary baseline, with different host scheduling; this is not a controlled timing comparison.

F64 energy matrix is bit-identical to the previous saved compact control, with whitened energy error4.488681472101003% plus independent reference uncertainty0.678063644006451% =5.166745116107454%, passing8%. F32 factorization succeeds, but exterior enrichment fails **`LOCAL_MAGNETIC_EXTERIOR: unresolved directed exterior Schur reciprocity`** before producing an admitted operator, energy matrix, response or reduced model. This is a failure, not an accuracy result. Existing per-application F64 residual refinement cannot rescue a failure constructing the operator's exterior Schur matrix. No guard was relaxed and no additional iteration algorithm was added.

The first direct runner accidentally selected the old F64 executable while compilation was still running. Its nominal F32 output is explicitly archived as `spoiled-stale-exe-f32.*` and excluded. Once compilation completed, the measured pair above used the same new executable, verified by four-byte factor storage and identical original matrix fingerprint. The unchanged F64 baseline was recorded before the paired experiment as well.

## Unknown ownership audit

Disjoint measured categories in this fixture are air trace3037, near-copper trace546 and cell-private61. Near-copper trace includes free non-private coordinates belonging to any cell touching an original copper vertex; shared-interface coordinates are counted once in this category. Private modes vanish on every cell face. Air includes remaining non-private coordinates. Column-owned upper CSC nnz are41962 /16694 /1647, corresponding to F64 index/value payload671392 /267104 /26352 bytes. The shared column pointer array is29160 bytes. These are ownership counts, not physically independent blocks: cross-category interactions exist. Factor memory cannot be additively assigned to physical categories after coupled elimination; only the measured total factor is reported.

The smallest subsequent conforming approximation would reduce one far-air cell patch while retaining the original shared edge/face polynomial trace at patch boundaries, copper traces and private copper basis. Existing minimum shared-entity degree constraints must be honored; changing one cell independently can otherwise reduce its neighbor's common trace. A local interior-only basis reduction is conforming when those functions vanish on all faces, but compact airP2 has no cell-interior Bernstein modes to remove. Thus a trace reduction needs an explicitly conforming patch/interface constraint treatment and independent energy/reference screening. No coarsening was implemented alongside precision.

## Verification and limits

The existing sparse-potential F64 test passed across simplicial and supernodal layouts. The compiled experimental F64 frozen energy test passed and reproduced the unchanged reference comparison; the same-coefficient F32 test failed at the existing reciprocity guard as recorded above. After evidence capture both modified Rust files were restored to their original tracked content, including the F64-only shared helper and magnetic operator. No retained Rust/TS runtime changes require a native rebuild. Existing user `native/analyze/debugging/` content was preserved.

Because the mandatory small control failed, no full small-channel DSL, real STM32 symbolic/numeric run or cold/warm/restart channel attempt was admitted. The existing OS-paging supervisor was not needed and no pagefile setting changed. <=4GiB / <=120s, complete-channel delay/loss/S accuracy, linear residual acceptance and original P7 bandwidth/physical obligations remain unproven. Retrying the unchanged failing candidate on the large channel would provide no admissible result. No commit or push was made.
