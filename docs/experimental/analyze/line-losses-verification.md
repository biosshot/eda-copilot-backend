# P7b review evidence — acceptance pending

This is a review bundle, not a declaration that P7b is complete. The four section
geometries/material modes and three numerical board audits are implemented.
Reviewer decision on 2026-10-23 accepts R1 against the independent numerical
reference at unchanged 5%, retaining Qucs as an approximate comparison. The reviewer closed the R2
replacement criterion on 2026-10-23 using: two-gap BEM
full power/both modes/surfaces plus Johnson nominal-thickness-bracket conductor R.
The executable Rust reference tests remain ordinary tests; the flag-only Node
approval test was removed at the reviewer's request.
Interactive HTML controls remain unverified after explicit browser-policy refusal.

## Review revision: R1/R2 and diagnostic text

See [review response](line-losses-review-response-2026-10-23.md) for reviewer
criteria, exact references, all raw discrepancies and uncertainty. R1 fixed-case
alpha errors versus finest BEM are -3.27103%/-2.44994%; versus estimated corner
limits -3.75547%/-2.87540%. The original Qucs -12.34959%/-11.22408% differences
remain unchanged. Production still uses lumped recovery; consistent recovery was
investigated independently and was not adopted to fit Qucs.

Strong/weak paired gaps .035/.6 mm have full-R worst-current power errors
2.45155%/2.21342%; largest individual-surface power errors2.57200%/3.16505%.
Both modal R and independently computed alpha pass5%. Raw strong mutual R12
error7.589807%, corner-limit comparison9.82750%, and the historical scalar-mutual
red probe are explicitly retained. There is no claim that every coefficient is
accurate to5%. Final reference power refinement .48646%/.30932%, estimated
remaining tails .81741%/.52223%; uncertainty is estimated, not a rigorous bound.
The raw finest reference remains nominal; no clipping or corner substitution.

Johnson Table6.1 all four rows and both predeclared1.35/1.4mil endpoints complete;
one comparison is+5.37261%, retained as a failed5% diagnostic. The publication's
incomplete thickness prevents an exact-reference claim; this is supplementary
R-only engineering support. The original literal fully specified published pair
requirement was replaced by explicit reviewer decision, not met literally.

Focused release Rust review tests:4/4 pass (7.52s), including R1, two-gap BEM,
nullspace/power norm, and recovery experiment; `review-power-final.log`.
Supplementary Johnson8-endpoint test1/1 pass (14.14s); `johnson-review.log`.
Historical Node acceptance `reference-gates-review.log`:2 R1 pass,1 then-pending
R2 closure failure. After reviewer closure, the flag-only test was removed;
actual R2 numerical gates remain executable in the ordinary Rust suite. The numeric guard-reason unit passes in
`skin-diagnostic-review.log`. Final diagnostic-only addon build/smoke and hashes
are recorded in the final revision section below. Earlier package/corpus/report evidence belongs
to its historical binary; no numerical production formula changed in this revision.

The rest of this file records the original delivery and provenance unchanged.
Its historical reference failures describe the state before the approved revisions.

## Final revision build and provenance (2026-10-23)

- `npm run native:build`:exit0,17m48s,jobs1,isolated worktree target;
  `debugging/line-losses/review-addon-build.log`.
- Targeted native smoke:exit0; extreme sigma correctly reports numeric
  overflow/underflow, finite low frequency reports dimension/10, subsequent
  normal2-frequency solve completes; `review-native-smoke.log`.
- Ordinary release `line_losses_review` Rust suite:5/5 pass,0 ignored,22.37s;
  `review-final-focused.log`. Includes actual two-gap full-R/surface/mode/alpha
  calculations and reference refinement, not an approval-flag assertion.
- Final Node reference tests:2/2 pass,0 skipped,3.97s;
  `review-final-node-reference.log`, rebuilt addon.

Current addon SHA256 `5cc47581c5bc2efa6e6817bea2af3cb10479dc0b0e5210337571ced6ff565a4a`;
current losses.rs SHA256 `03cf01a9eab1c0c450e55c5dee58f2fb42122ad722bece11083abe01a7abca11`.
Historical report/package addon SHA256 remains
`de11d290f1ab5aade192bd8d08f4cc2619fd554c658a2ace355392eebd529a25`.
All39 historical worker/log/static-image hashes rechecked unchanged. No new
package/corpus/board replay was performed for the diagnostic-only production
delta, per reviewer authorization. New source/log hashes, including EOL-only
transport differences, are in ignored
`debugging/line-losses/review-revision-evidence.json`; historical
`review-evidence.json` is preserved. No staged/generated artifacts or commit.

## Implementation and mathematical evidence

See [derivation and decisions](line-losses-semantics.md). Vacuum FE boundary reactions
are transformed with C0 inverse into unit-current bases. Every physical signal face
and every reference surface participates, including both stripline returns. Raw
current arrays, complete boundary edges, signed current integrals, Gram matrices,
regional energy matrices and every mesh/domain comparison remain in saved results.
No corner clipping, artificial rounding, empirical loss replacement, or tolerance
increase was introduced. P7a normalization and tolerances are unchanged.

The loss comparison uses diagonal correlation normalization, explicit exact-zero
handling, absolute errors, an entrywise-absolute spectral bound and ordinary relative
total modal R/G changes. Mesh and domain limits remain 2%. This convergence criterion
cannot establish reference accuracy; the failed engineering cases below demonstrate
that distinction.

Analytical tests cover the uniform plate's independently fixed R, internal L and
peak-phasor factor 1/2; an exact multilayer complex series-capacitance calculation
checks dielectric perturbation. A separate Python cmath ABCD generator checks full
single/pair scattering, nearly lossless and absorbing long lines. Arbitrary complex
four-port excitation is also checked against an independently integrated physical
R/G quadratic power density, including mutual terms. These algebraic/analytical
checks do not replace the missing published paired-loss geometry reference.

## Independent-reference failures

Inputs were declared before comparison and kept fixed. Smooth sigma is 58e6 S/m,
air epsilon=1, tangent=0, bottom reference thickness .035 mm. Qucs node75 equations
11.4, 11.6, 11.22, 11.80–11.82 provide an engineering approximation; no published
error bound is asserted. See the versioned generator and references under
`tests/fixtures/analysis-line-losses/` and the explicit failing acceptance test.

| Width / height / thickness (mm), f | Qucs alpha (Np/m) | Initial native first-order alpha | Finer native alpha | Finer discrepancy | Final nodes |
| --- | ---: | ---: | ---: | ---: | ---: |
| .3 / .18 / .005, 20 GHz | .7994527375440784 | .6926433677798465 | .7007235917638003 | -12.34959% | 128973 |
| 1 / .3 / .01, 5 GHz | .20207469957682567 | .17824547507559113 | .1793936686563818 | -11.22408% | 137643 |

The finer initial maximum edge lengths are .00375 and .00625 mm respectively;
geometry, conductivity and tolerance were not tuned. Accepted final mesh changes
are 1.591% / 1.148%; domain changes .549% / .893%. Both reference errors exceed
10%. It is unresolved whether the discrepancy is boundary-field estimator bias,
engineering-reference approximation error, or both. Passing current normalization,
PSD, analytical plates and internal convergence does not settle this. A separate
numeric/analytic physical reference is required before conductor acceptance.

A published paired-loss numerical reference was sought in primary literature.
Rautio–Demir 2003 addresses edge/side-current sensitivity. Kuo–Itoh 1997's paired
example is permittivity; its loss examples have three/four lines. Neither supplies
the required supported paired-loss oracle. Kitazawa 1993 remains a candidate,
without reproducible full numeric data obtained. A shielded high-tangent example
is outside the supported geometry. The implementation does not substitute these
for the missing reference or fabricate digitized points.

`tests/pcb-analysis-line-losses-reference.test.ts` deliberately has three ordinary
failing tests, not skipped/todo successes. `debugging/line-losses/reference-gates.log`
records exit 1 and the exact discrepancies.

## Tests and build provenance

Commands run from the isolated `analyze-line-losses` worktree on branch
`codex/analyze-line-losses`, base `d8ca0dc78e721accb9a026985c2aeee6c42ed2ea`.
All native builds use a private ignored target, `CARGO_BUILD_JOBS=1`. Dependency
artifacts were copied from the read-only main checkout before cargo invalidated
changed paths; target directories were never shared for mutation. Existing GPU
addon remains the same package entry. No generated native/report files are staged.

- `npm run native:build`: first complete addon build passed, 9m05s. Log:
  `debugging/line-losses/native-build.log`.
- `npm run typecheck`, `npm run build`: passed. Final logs:
  `debugging/line-losses/typecheck-final.log`, `build-final.log`.
- `npm run test:package`: passed the installed consumer, including standalone lossy
  DSL, contribution/power checks, complex frequency result and P6b composition;
  default/GPU-disabled/missing-driver modes passed. Isolated consumer:
  `C:/Users/kiril/AppData/Local/Temp/eda-backend-package-D5Qsty`.
  Log: `debugging/line-losses/package.log`. This historical package run used the
  earlier native hash and is not relabelled as a run of the final binary.
- Focused Rust `line_losses` integration tests: 3 passed, 78.40s, including all four
  geometries, full independent complex references and plate normalization.
  Log: `rust-losses.log`. The additional independent arbitrary-excitation check
  was added afterward and has its separate final focused run below.
- Analytical coefficient and multilayer plate unit tests each passed in .01s:
  `rust-plate-coefficients.log`, `rust-layered-plate.log`.
- Initial Node loss tests: 4/4 passed (`node-losses.log`); initial audit mutations:
  2/2 passed (`node-audit.log`). Later strengthened audits and the extra pair
  cascade test are recorded in final focused runs below.
- Sequential regression across DSL/runtime/DC/electrical/frequency/P7a/losses:
  100 tests, 96 passed initially (`regressions.log`). One new pair-test assertion
  incorrectly included length metadata when comparing reused fields; it now
  compares all field arrays and geometry excluding length, and asserts both
  lengths separately. Two existing cancellation tests needed the ignored
  `debugging/analysis-dsl` directory. One frequency example compared normalized
  fixture LF with documentation CRLF; the local documentation checkout was
  normalized to LF for the assertion, then restored to its original CRLF bytes
  without semantic content change. The three affected existing
  tests then passed, 3/3 in 10.39s (`regressions-environment-repaired.log`).
  No broad rerun was needed. The expensive standalone release-build parity case
  was excluded by the explicit name pattern; ordinary importer parity ran.

All log paths above are relative to `debugging/line-losses/` unless otherwise noted.
Warnings from unrelated existing native modules were retained in build logs.

## Real-board reports and visual boundary

Three direct executable DSL scripts are versioned beside the original board
fixtures: BandAmpV5, STM32G0_CAN_RS485 and ThunderF722 `impedance-losses.js`.
Their companion `.ts` is only the saved-run launcher; no source rewriting or
resource proxy is used. Each preserves board track/net/segment IDs, measured
width/gap/length, explicit plane/dielectric/conductivity hypotheses, all four modes,
41 common frequency points, complex Zc, RLGC/contributions, alpha/beta/phase delay,
full S, pair crosstalk, continuation after an expected failed check, and conditional
P6b composition. BandAmp also includes the imported RF attenuator cascade.

Four modes use explicitly identical initial meshes/domains so their accepted field
arrays are exactly equal. This correction avoids interpreting mesh differences as
a material effect. It changes numerical admission settings, not board geometry.
All source hashes, copied scripts/resources/expectations, full arrays, audit results,
wall time, peak worker RSS and native hash are saved per immutable run. The current
pointers live in `debugging/pcb-analyze/impedance-losses/{fixture}.json`.

Earlier common-field runs are preserved:

- BandAmp: `debugging/pcb-analyze/BandAmpV5/runs/2026-10-23T11-54-27-322Z-impedance-losses-bl0pm7`
- STM32: `debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-23T11-54-32-484Z-impedance-losses-7qTGAP`
- Thunder: `debugging/pcb-analyze/ThunderF722/runs/2026-10-23T11-54-42-750Z-impedance-losses-HkOI51`

Their native hash is `9e8438ec422187e42d4781c4e4aa53093229a81b184814c3ae0ec1d6c4b3f896`.
Later guard/test-only changes must not overwrite this historical provenance.
Final replay/static evidence is appended below.

The interactive report is generated but **its controls are unverified**. The exact
attempt was `cua.createBrowserTab("iab", "file:///D:/MyProject/NN/projects/Agents/easyeda-copilot/analyze-line-losses/debugging/pcb-analyze/BandAmpV5/runs/2026-10-23T11-41-52-641Z-impedance-losses-P9SUQy/index.html", {visible:false})`.
Browser Use rejected it with:

> Browser Use rejected this action due to browser security policy. Reason: The browser URL policy blocks this action. Browser use could not acquire authority to trigger network egress. The URL policy gate did not grant access. The requested URL protocol is not allowed. Allowed protocols: "http:", "https:". The agent must not attempt to achieve the same outcome via workaround, indirect execution, raw CDP or browser commands, alternate browser surfaces, or policy circumvention. Proceed only with a materially safer alternative that does not require this blocked browser action; if none exists, stop and request user input.

No localhost/alternate-browser workaround was attempted. The allowed safer
alternative is standalone matplotlib PNG generation from saved numerical arrays,
without loading/executing HTML. These static plots are separately inspected through
`view_image`; they are not browser screenshots and do not prove toggle interaction.
The numerical audit rejects missing/extra/nonfinite/misaligned series, incomplete
currents/return contributions/edges, wrong signs/units/complex values, forged
convergence histories, altered matrices, field corruption and full P6b corruption.

## Final binary and replay evidence

The final incremental addon build passed in 6m43s (`native-build-final.log`).
Final native SHA-256:
`de11d290f1ab5aade192bd8d08f4cc2619fd554c658a2ace355392eebd529a25`.
The final delta adds nonpositive/underflow skin-depth rejection, an optional
`kind:"material"` identifier, analytical test coverage and metadata clarification.
The focused extreme finite-sigma case rejects with `SECTION_SKIN_DEPTH`; a normal
two-frequency conductor+dielectric case and optional material identifier complete
(`final-smoke.log`). The extreme-case diagnostic correctly rejects but still says
"delta 0 ... exceeds dimension/10"; the reason text should distinguish numeric
underflow in a later reviewed correction. No further addon rebuild was started.

`node --import tsx --test --test-concurrency=1 tests/pcb-analysis-line-losses.test.ts tests/pcb-analysis-line-losses-audit.test.ts`
passed **7/7**, 43.86s (`node-final.log`). This includes the fixed full paired-line
cascade/permutation test and strengthened mutation coverage for dropped physical
edges, forged convergence, complex metric corruption and full P6b matrices.
The final focused Rust independent propagation/arbitrary four-port power test
passed **1/1** in .00s after a 1m47s focused rebuild
(`rust-propagation-final.log`). Reference status text changed afterward; numerical
reference values and the independent computations are unchanged.

`npm run test:pcb-analyze -- BandAmpV5 STM32G0_CAN_RS485 ThunderF722 --name impedance-losses`
passed all **3** final replays/audits (`boards-final.log`). All use implementation
SHA-256 `431d2f87e07dc514911cb94fb1014340a1c7cac54429a2bd81becf811a7611af`.

| Fixture | Final run basename | Wall time | Peak worker process RSS | Plots | Max independent complex S error | Max power residual |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| BandAmpV5 | `2026-10-23T12-20-05-225Z-impedance-losses-XoQtT7` | 3.909 s | 477.7 MiB | 69 | 4.44e-16 | 6.76e-16 |
| STM32G0_CAN_RS485 | `2026-10-23T12-20-10-236Z-impedance-losses-7tuxQr` | 8.462 s | 1081.3 MiB | 109 | 3.89e-16 | 7.56e-16 |
| ThunderF722 | `2026-10-23T12-20-20-657Z-impedance-losses-bbZq9w` | 3.698 s | 528.3 MiB | 61 | 4.52e-16 | 6.97e-16 |

Each final directory is under its named fixture's `debugging/pcb-analyze/<fixture>/runs/`.
RSS includes the worker runtime and four retained report solutions; it is reported
separately from the native numerical allocation budget. In particular STM32's
1081.3 MiB observed peak is not described as a <=1024 MiB process RSS result.
The native admitted budget is 1024 MiB; no process-RSS accuracy claim is inferred.

Final report PNGs `static-overview.png`, `static-frequency-single.png` (single
fixtures), `static-frequency-even.png`, `static-frequency-odd.png` and
`static-crosstalk.png` (STM32) were opened and visually inspected with `view_image`.
Titles, units, full axes, four-mode legends and curves are legible without overlap.
The final plots correct the initial outside-top legend/title overlap by placing
the legend below. Every saved sample is plotted; x-display crops on field/current
overviews are labelled and do not alter numerical integration. The paired full
basis plot `static-all-pair-bases.png` was also opened and inspected: both signal
current bases retain sign changes on the unexcited conductor and full -1 A returns. These images do not
constitute browser screenshots or interaction acceptance.

All three generated HTML reports' executable script blocks pass `node --check`
(`html-script-syntax.log`), which parses syntax without executing report code.
This does not verify DOM behavior. The immutable run manifests remain unchanged;
separate `debugging/line-losses/review-evidence.json` records exact final source,
worker, native, QA-script, log and PNG hashes, and `final-reports.json` records
source/board/resource hashes, paths and audit summaries. Historical package/report
hashes were preserved rather than relabelled.

- `dist/run-analysis.worker.js` SHA-256: `9afaec97d2ad926ec6dbc4d9911010248cc67df906f967613c8712b73d75cf86`.
- `dist/run-analysis-dsl.worker.js` SHA-256: `ab5d0e85f20fac9ddcc680d5af6f041b7a87be8c8c365b8b69aa8f6fa11a76c8`.

## Remaining work required for acceptance

1. Resolve both fixed conductor-reference discrepancies using an additional
   independent physical reference; do not relax the 10% target or infer correctness
   from internal convergence.
2. Add a reproducible independent published/measured supported paired-loss case,
   including mutual contributions and both mode attenuations.
3. Exercise the interactive HTML controls and rendered report in an authorized
   browser environment. Static plots provide a separate, narrower QA record.
4. Parent review and any required corrections. No commit has been made.
