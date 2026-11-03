# P7 memory experiment, 2026-10-30

Status: final explicit experimental candidate uses airP2/trace4/bubble4 or5. Independent energy and whole-coupon response screens pass8%; real remote symbolic memory falls10.83%, but the subsequent exterior-enrichment budget still cannot fit. No positive remote electrical result. P7 remains open. CPU/F64 direct sparse FEM, unchanged physical geometry/material/contact/current spaces. Residual/KCL/passivity and the existing 2% spatial/domain sensitivity guards remain unchanged. The separate 7.7902% composition delay discrepancy is not consumed as an error allowance for this experiment.

## Profiles and independent control

Remote baseline is `[trace=4,bubble=6,quadrature=16,stars=1]`, with existing fine `[6,8,16,1]`; the historical no-level `[6,8,16,3]` is preserved. The saved remote baseline refusal is `automatic-supervised-1793364627871399200`: 1,411,442 magnetic unknowns, 50,854,985 upper CSC entries, 692,719,663 exact simplicial fill; numeric-stage plan 8,714,661,076 bytes (includes retained/CSC/symbolic/scratch, **not solely factor values**), wall 30.536396 s, tree RSS 2,623,897,600 bytes. No positive electrical result existed.

The independent frozen complete three-tetra source space uses the original `tetra-space-reference.json`, identical tetra vertex bits, full 3x3 magnetic action matrix and whitened relative energy norm. Because the Cartesian scalar operator is isotropic, the same matrix covers each coordinate direction. No DC resistance is used to claim L accuracy. Reference empirical uncertainty is 0.678064%; it is added conservatively to the reported norm for this screening.

| profile | unknowns | input CSC nnz | exact fill | numeric-stage bytes | operator planned bytes | energy error |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| baseline 4/6 | 11220 | 267698 | 3046424 | 45876288 | 50435448 | 2.834247% |
| rejected 3/5 | 10880 | 252818 | 2801722 | 43384576 | 47917656 | 8.529731% |
| rejected 3/4 | 10871 | 252611 | 2801515 | 43373160 | 47902632 | 9.359671% |
| compact coarse 4/4 | 11193 | 266618 | 3045344 | 45823208 | 50378648 | 3.314433% |
| compact fine 4/5 | 11202 | 266951 | 3045677 | 45839568 | 50396760 | 3.137958% |

Initial two trace3 profiles failed the 8% screen. At review, a targeted bubble-only follow-up retained trace4 to isolate that failure: 4/4 and 4/5 pass this small control (3.992497% and 3.816022% including empirical reference uncertainty). This does not prove arbitrary-board accuracy. The initial rejected3/5 worst spectral energy error is 7.466565%; even adding reference uncertainty exceeds8%, so no norm substitution is used to admit it.

## Intermediate bubble-only selection (superseded by the final airP2 choice below)

The intermediate airP3 `compact-v1` binary selected4/4 at spatial level1 and4/5 at level2. Its measurements remain evidence; this is not a second runtime option. The final rebuilt binary uses airP2 for this same named experimental option, with binary hashing separating earlier caches. Unset or `baseline` keeps the original pair. No-level historical extraction stays unchanged. Unknown profile names refuse; OOM never changes profile. Binary/profile identity enters the cache key, extraction provenance records the selected profile and patch level, restored metadata must match. One executor refuses changes to the profile during its lifetime, preventing a memoized-key/native-environment mismatch.

This option remains an experimental candidate pending whole-coupon and real-remote evidence; it does not weaken sensitivity gates or grant complete physical admission. Source controls are retained in the ignored explicit release test `p7_memory_profiles_frozen_energy`; logs are under `debugging/p7-memory-energy.txt`. Generated measurements stay outside user `native/analyze/debugging`.

## Whole-coupon physical observables

`automatic-supervised-1793367628183973900` runs the compact fine profile on the complete unpartitioned coupon, unchanged0.075mm electric reference geometry/current mesh, versus the saved fine baseline `automatic-supervised-1793363430965924500`. The original fixed floors remain S0.01, delay1ps, Cu loss1e-18W. Complex S maximum relative change0.00229887%, delay5.275882%, copper loss0.0000006951%, port complex voltage4.58057e-11%. Phase delay is65.823ps versus69.489ps at10kHz. This passes the requested8% profile comparison; it does not remove or certify the separate slab-composition discrepancy. Frozen magnetic energies above supply the independent L/energy control; port DC-like resistance alone is not used.

Cold supervised wall157.254362s; tree RSS1,226,416,128B, private commit1,229,438,976B, native OS peak1,164,554,240B. Native stage walls: electric6.721999s, magnetic1.741222s, training42.323551s, reduction54.261943s. Electric121500 unknowns remain unchanged; compact magnetic6831 unknowns,206203 input CSC nnz,1594228 exact fill,15840952 factor-value bytes,3420607 scratch bytes,26870381 numeric-stage bytes. Explicit Windows paging mode preflight available commit17,915,756,544B via GetPerformanceInfo; minimum16,673,177,600B. Initial physical available2,382,397,440B. Host activity/paging differ from the historical saved reference wall41.961877s; this is not evidence of a wall-time speedup. Native guard checks in training retain residual~1e-16, KCL≤5.6e-14 and KVL≤4.9e-10.

The reusable `scripts/audit-p7-memory-profile.ts` compares the real saved model responses and writes `profile-comparison.json`; it returns nonzero beyond8% and does not change admission in the production executor.

## Actual remote STM32

One real compact coarse remote attempt, `automatic-supervised-1793367808884100700`, used explicit native8192MiB / tree9216MiB private-commit cap / reserve512MiB /600s with OS paging. Windows available commit at preflight17,759,735,808B, minimum16,071,524,352B; original physical available2,714,009,600B, minimum1,219,072,000B. Existing physical geometry/electric preparation completed:99215 electric unknowns. Magnetic symbolic preparation yielded:

| quantity | saved remote baseline4/6 | compact4/4 | change |
| --- | ---: | ---: | ---: |
| magnetic unknowns | 1411442 | 1113875 | -21.08% |
| upper CSC nnz | 50854985 | 38952305 | -23.40% |
| exact simplicial fill | 692719663 | 680816983 | -1.72% |
| numeric-stage plan bytes | 8714661076 | 8136190172 | -6.64% |

Compact numeric-stage breakdown: retained base194,971,877B; input CSC623,236,880B; symbolic221,742,160B; node reserve285,152,000B; actual supernodal factor values6,169,957,272B; scratch641,129,983B. Their sum is8,136,190,172B. Available magnetic budget after retained live operators was7,788,285,892B, so allocation correctly refused before numeric factor. Bubble unknown reduction leaves the shared trace fill dominant. Merely reducing unknown count does not proportionally reduce fill or solve memory.

Wall132.162570s, tree RSS2,400,202,752B, tree private commit2,620,940,288B, largest child RSS2,328,219,648B. This is a successful bounded measurement and an unsuccessful electrical solve, with no OS kill, deadline or outer memory guard. No real remote ports/L/delay can be claimed. No identical retry is justified, and pagefile settings were not changed. Historical baseline and current walls are not comparable as speedups because host contention, paging and explicit budgets differ.

## Validation and limits

Native addon build PASS12m43s with the final sparse-plan diagnostics and streaming reduced Gram implementation. TypeScript typecheck/build and7 cache tests PASS, including explicit profile cache separation, invalid name rejection and same-executor profile-change refusal. Full whole-coupon computation passes original native numerical guards; response comparison to a saved independent whole reference passes8%. The final airP2 profile remains explicit/experimental and default remains baseline. Entire P7, inter-slab coupling, real remote electrical acceptance and arbitrary RF/transient admission remain open.

Independent per-profile subprocess measurements (same final control test executable, one process/profile,10ms OS RSS sampler) are saved in `debugging/p7-memory-controls`. Repeated controls were necessary to isolate actual RSS from symbolic plans after the initial all-profile energy screen.

| profile | process peak RSS bytes | supervised process wall s |
| --- | ---: | ---: |
| baseline4/6 | 51171328 | 22.688615 |
| rejected3/5 | 48676864 | 14.004023 |
| rejected3/4 | 48726016 | 17.242545 |
| compact4/4 | 51081216 | 20.133453 |
| compact4/5 | 51122176 | 7.537986 |

All five exited0 and reproduced the energy matrices. The small control's air trace space dominates, so bubble-only RAM savings are tiny. Walls vary substantially with unrelated host activity and are observations, not a reliable speedup claim. The ignored test additionally asserts accepted/rejected energy-screen boundaries; final focused results are recorded below. `git diff --check` PASS. No commits created.

## Symbolic ordering control

The bounded developer-only example reconstructs the **same** real remote matrix and compares full AMD with private-first AMD, stopping before numeric allocation. `debugging/p7-memory-ordering` retains the exact extracted request, logs and supervisor evidence. Full AMD gives686,606,056 exact fill and8,212,694,988B numeric-stage bound versus private-first680,816,983 and8,136,190,172B. Thus full AMD is worse by0.94% in numeric-stage memory; the existing private-first ordering remains. Wall116.811419s, RSS2,192,052,224B/private commit2,484,711,424B; initial available Windows commit21,052,280,832B, no outer refusal. No numerical8GB allocation was attempted. Temporary diagnostic access, public module exposure and the example environment hook were removed after measurement; no production ordering toggle is retained.


## Final choice: lower far-air degree, retained near-copper traces

The final single opt-in is `P7_MAGNETIC_PROFILE=compact-v1`: air degree2, near-copper trace4, copper interior bubble4 (spatial level1) or5 (level2), exterior quadrature16, one copper vertex star. Unset/`baseline` retains air3/trace4/bubble6 level1 and air3/trace6/bubble8 level2; no-level historical extraction retains air3/trace6/bubble8/stars3. CPU/F64, direct sparse factorization, physical mesh/contact/material/current spaces and all numerical/sensitivity guards are unchanged. Internal air-degree validation permits only2 or3; no new arbitrary DSL numerical parameters exist.

Independent frozen3-cell energy: air2/trace4/bubble4 norm4.488681% plus0.678064% inherited reference uncertainty =5.166745%, below8%. Unknowns3644 versus11193 intermediate air3/trace4/bubble4; exact fill387209 versus3045344; numeric-stage bound9,592,000B versus45,823,208B. This large small-control improvement does **not** transfer to the dense real-board trace space.

Complete unpartitioned air2/trace4/bubble5 coupon (`debugging/p7-memory-air2`, native example, unchanged0.075mm electric refinement): maximum delay change5.625475%, complexS0.002451196%, copper loss0.0000007341% versus the saved whole reference. It passes8% with S0.01/delay1ps/loss1e-18W floors; explicit fixed-current magnetic energy above verifies L rather than using DC R. Phase delay at10kHz65.580ps versus69.489ps. No direct port-voltage comparison was available in this example-mode packet; S compares all two-port responses. Wall59.497817s, RSS1,070,620,672B, private commit1,122,906,112B; no outer refusal. These timings use a different host load and orchestration from the earlier worker packet and are not a speedup claim.

Real remote air2 symbolic comparison (`debugging/p7-memory-air2-remote`) uses the exact same extracted request and magnetic graph construction. Private-first AMD:1,097,762 unknowns,38,421,979 upper CSC nnz,645,577,888 exact fill; numeric-stage bound7,771,103,332B. Relative to the original remote baseline: unknowns-22.22%, inputnnz-24.45%, fill-6.81%, numeric-stage memory-10.83%. Relative to intermediate bubble-only air3, numeric-stage memory improves4.49%. Full AMD remains worse:651,575,696 fill and7,824,542,644B bound. This is a modest reduction, not a multiple-fold real-board improvement.

Remote symbolic wall59.913591s, RSS2,145,325,056B/private commit2,439,733,248B; initial Windows available commit22,801,915,904B; no outer refusal. Both orders stop before numeric allocation. The private-first numeric plan barely fits example magnetic budget7,796,674,500B, but `enrich()` must additionally reserve at least free*9*24 =237,116,592B above the operator plan. Thus exterior enrichment has a strict lower plan8,008,219,924B **before** source integrals and solve scratch, already beyond the available budget. A costly numerical factor cannot produce a complete remote result under this8GiB native cap; it was deliberately not allocated. No successful remote model, ports, L or delay is claimed.

One final targeted alternative retains trace3 but increases interior bubbles6 (air3) to isolate trace/private tradeoff. Independent norm plus reference uncertainty8.703834% fails8%; unknowns10898, exact fill2802217, numeric-stage bound43,411,216B. It is rejected without remote execution. No further candidate search or hidden OOM fallback is present.

Reproduction evidence includes the exact request and temporary diagnostic/air2 patch in `debugging/p7-memory-ordering/diagnostic-and-air2.patch`, the supervised example runners/logs, frozen energy logs, and the final shared native implementation. Apply the temporary patch only to its recorded pre-final source for diagnostic replay; it is not a production ordering setting. Generated evidence stays outside user `native/analyze/debugging`.

## Final binary and focused verification

Final addon SHA256 `28B7AF88580A72C4BFCD0E9A4AD40D2C7ED29BC1778E440CB69B67380FFDCB84`; `npm run native:build` PASS7m28s. No diagnostic ordering hooks or environment air-degree overrides remain in the native example/runtime. The one fixed compact profile is selected through the existing explicit experimental name.

- `cargo test --release --manifest-path native/analyze/Cargo.toml --test board_response --test volume_projection`:7 response tests and1 fixed coupled-energy/reciprocity projection test PASS.
- `cargo test --release --manifest-path native/analyze/Cargo.toml --lib sparse_potential -- --nocapture`:1 pinned simplicial/supernodal owned-storage test PASS.
- Final frozen energy test, air2 with explicit4/4 and4/5 filters:both PASS. Fine4/5 norm4.294509% plus0.678064% inherited reference uncertainty =4.972573%; coarse total5.166745%. The test rejects an unknown filter rather than silently running no profile.
- `npm run typecheck`, `npm run build`,7 `analyze-local-cache.test.ts` tests and `git diff --check`:PASS.
- Tiny final-addon compact level1 smoke on the original216-current/28-potential coupon, explicit512MiB/60s:PASS in2.292388s, native process peak85,921,792B. Its real magnetic factor uses4941 unknowns,151333 CSC nnz,930921 exact fill; positive magnetic diagonal, response solve and original numerical guards pass. This is a small addon mapping/smoke check, not a repeated real remote measurement or accuracy certificate. Packet `debugging/p7-memory-addon-smoke.json` plus log.

All real remote and independent whole-reference measurements described above remain unchanged; no further heavy real-board retry was made after the final build. Tests/evidence do not close P7 or turn the existing slab-composition error into a complete-channel accuracy claim. No commit was created by this worker.
