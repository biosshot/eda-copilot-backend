# P7 implementation and D1 continuation, 2026-11-04

## D1 slot internal convergence

Under the current serial resource policy (1800 s, 4096 MiB process-tree private commit, 3072 MiB native, one thread, 1 GiB host commit reserve), the first resume stopped after 76.89 s with `SECTION_MESH_LIMIT`. The single retry raised the section cap to 200,000 nodes. The relevant section used 57,158 nodes and 113,302 triangles. The retry reused 35 cache entries from the earlier resume and completed in 393.406 s. Supervision recorded process-tree peak RSS of 807,067,648 B and peak private commit of 748,253,184 B. These are resumed-run measurements. The artifact directories are `debugging/p7-d1-cross-coupling/slot-graded-electric-resume1800-20261104/` and `debugging/p7-d1-cross-coupling/slot-graded-electric-section-budget1800-20261104/`.

| D1 delay check | Change | Result |
| --- | ---: | --- |
| Domain 1 → 2 | 5.467882% | Fail |
| Domain 2 → 4 | 1.291154% | Pass |
| Spatial refinement at domain 4 | 0.3207689% | Pass |
| Boundary shift at domain 4 | 1.1317817% | Pass |

The DSL completed and returned domain 4. All returned cache keys were confirmed against the selected domain. This establishes internal numerical convergence of this D1 slot result over 1–10 kHz. The historical resume packet recorded `pending-comparison` in `comparison.json`. The subsequent matched comparison and remaining D1 gates are reported below.

## Implementation and focused checks

- Return planning includes finite-width returns and a remote physical bypass. The corresponding geometry checks pass. The STM32 multicorridor case still refuses the unsupported interface.
- Blind and buried via span geometry passes three focused checks. Six scoped via DC cases pass. Full physical via acceptance remains open. The separate coax reference and volume skin-transition diagnostic do not validate blind-via EM.
- Explicit component R/C/L attachments and pin requests are wired through the adapter. Public DSL `.components` and planner integration resolve exact captured pins and bounded physical paths of up to eight graph edges, including intermediate islands. Board-level component accuracy is pending.
- The independent low-band audit runs on the final local candidate. It checks endpoints and full-versus-reduced responses and reports coverage gaps. Segmented common-band adaptation remains open.
- Nine board preparation tests and 27 TypeScript tests pass, including 14 return planner, four component, five convergence and four section budget tests. TypeScript typecheck passes. All five native frequency audit tests passed. The native build passed in 4m55s. Addon SHA-256 is `e0816d1e451dbfa019e53a170a587a09532f8f9faec5af66abb0ed05d1d8e9e6`. The later TypeScript batch passed 36 focused tests, typecheck and package build. The separate baseline native coax volume skin-transition run ended after 87.266 s at 12,697,600 B private memory with a GMRES failure at 240 iterations. The diagnostic retry with 16 restarts completed; its accuracy outcome is recorded below.

P7 remains open. The numerical pass above is scoped to internal convergence of the resumed D1 slot, and the geometry and adapter checks retain their individual scopes.
## Local W6 physical verification and D1 independent comparison

The fresh-cache event 5 attempt at `debugging/p7-d1-cross-coupling/slot-w6-event5-20261104/` validated intrinsic local coverage at 1–10 kHz in 125.235 s. Peak tree private commit was 304,087,040 B and peak tree RSS was 336,031,744 B. Independent replay was true. The recorded loss and power comparison values were 2.03e-15 and 4.52e-15, with 12 adaptive intervals. This evidence covers the intrinsic local operator. The segmented W6 and owner-loss changes are integrated. Their 23 targeted TypeScript tests and typecheck passed. The final native build passed in 6m27s, with addon SHA-256 `5458b8730a2a85dbde61753b2a02e94bbadb28cd29c2e73f4a2fe4b0621ae782`. Package build and eight actual-native boundary/component tests passed. The local event 5 measurement does not establish physical segmented common-band acceptance.

The matched full joint D1 reference completed in 65.594 s with peak private commit of 125,411,328 B. The matched hybrid completed in 680.031 s, with peak private commit of 796,983,296 B and RSS of 825,208,832 B. Raw packets and the baseline binary are saved under `debugging/p7-d1-matched-20261104/`. The copied packet omits the cache.

| Check | Result |
| --- | --- |
| Matched comparison, four-port | 1.25208% |
| Matched comparison, open response | 1.45596% |
| Four-port domain 2 → 4 | 0.726184% |
| Four-port spatial | 0.356493% |
| Four-port cut shift | 1.363572% |
| Strict loaded replay | 4.93e-7, fails 1e-8 |

Strict replay failed on residual evaluation with rounded I+Z/50. The integrated fix evaluates the residual from the original Z. The staged regression failed before the fix and seven focused tests passed afterward. Stock Rust checks passed: seven board preparation, four matched-wave, four via tests (including six DC subcases), and one capacitive test. The final native build passed. The existing full-rank local basis remains in use. A proposed basis change was left out because its need was unproven.

The joint spatial fine run completed in 99.078 s with 223,588,352 B peak private commit. Main S, loss and delay comparisons passed, including 0.55% delay, while one loaded-reactance entry failed at 21.8164% (2.18 pH shift). Joint domain ×2 completed with maximum reported relative sensitivity 1.21474e-5 (0.00121474%) and all reported checks passed. Magnetic comparison data are missing for that domain control. D1 physical acceptance remains open.

The major remaining implementation gap is distributed return planes. The STM32 enclosing ROI is 59.3918 × 39.3918 mm. Fine electric meshing has a 28,513 MiB (about 27.845 GiB) lower bound there. The experimental foil operator supports 64 support nodes. The first positive staged distributed-plane control failed magnetic reciprocity. The updated staged plate passes the raw magnetic gate at 9.63e-10. Six prototype tests pass. Fine convergence required refinement from 1,428 to 6,092 prisms, with a dense memory floor of 12.15 GiB already at 1,428. The adapter remains unintegrated. The staged DC helper is unwired. Missing operators and physical checks both keep P7 open.
## Strict D1 refresh and next implementation work

The refreshed joint packet completed in 73.406 s at 124,223,488 B peak tree private commit. Full Z and unit states match the original packet bitwise. The refreshed loaded state passed strict replay on the captured slot: relative port error 8.60705e-17, normwise current error 8.32475e-17, true physical Norton residual 3.04929e-26 A. The hybrid is reused explicitly from its original producer. Refresh evidence is under `debugging/p7-d1-matched-20261104/slot-reference-refresh/`. Reference admission remains `unverified` because the joint spatial loaded-reactance gate failed and domain magnetic evidence is incomplete.

Continue implementation with the scalable distributed-plane operator, DC path integration and local skin/HF and physical common-band coverage. For each affected class, establish the required physical operator and frozen matched input/reference first, then run one targeted acceptance control. The previously started coax retry is complete; no further refinement series was launched. The current work returns to fast development without new research sweeps.


## Completed coax diagnostic

The single restart-16 retry completed both lengths and all three frequencies in 204.266 s, with peak private memory 19,091,456 B and RSS 23,379,968 B. Its strict solver convergence checks passed. On this coarse mesh the comparison against the independent coax reference remains **unverified / exceeds-review-range**: the largest error is 27.8051% in regional loss at 400 kHz, above the 5% target and 6.5% review ceiling. Series-impedance errors at 25/100/400 kHz are 9.3740% / 1.6080% / 12.4793%. This is not skin-transition or via acceptance. No additional refinement campaign was started. Raw report and progressive samples: `debugging/p7-coax-transition/base-restart16-20261104/`.

The matched D1 harness used its recorded 4096 MiB native/tree caps and 512 MiB host commit reserve. The local W6 and coax harnesses used 3072 MiB native, 4096 MiB tree and a 1 GiB reserve. Actual measurements remain attached to each supervisor ledger.
