# Geometry validation verification

Implementation worktree: `codex/analyze-geometry-validation`, base `f648956`.
CPU/F64 only. See [semantics](geometry-validation-semantics.md). Review pending;
no commit or merge is made by the implementation agent.

## Implementation boundary

The native `validation` module reuses the existing copper preparation and reliable/
uncertain contact graph. An optional internal preparation observer keeps only
outside/cutout fragments after the same scoped drilling. The ordinary preparation
serialization is unchanged, checked by the observer/no-observer regression.
There is no second connectivity solver, global geometry envelope, mesh, refill,
DC/placement alteration or new V2 field. All source polygons are retained once in
the native report; overlays reference their stable validation IDs.

R-tree object broad phase limits same-layer pair candidates. Per-polygon segment
indexes produce exact supported shortest-distance witnesses; analytic closed-region
containment/intersections produce zero distance. Existing approximation and boolean
allowances remain explicit. Outside findings carry a separate interior evidence
point beyond the error allowance of both board and source contours. Unknown nets,
uncertain contacts/fills/drills, omitted geometry and open intent cannot yield a
complete no-defect result. `violations.count()` is unavailable for every partial
coverage result, including positive confirmed findings. Confirmed items and the
explicitly labeled confirmed-report count remain available.

Allocation admission uses input/generated-vertex/index/contact/report plans;
request/candidate/segment/measurement/vertex/serialized-byte caps precede bounded
work. These estimates do not impose an OS RSS cap. Native time checks surround
preparation and occur within validation loops. Existing preparation retains its
own budgets and the disposable process provides the hard deadline/cancel boundary.
Native error reports reject explicitly without an empty complete result.

## Independent checks

Analytical native coupons cover known rectangular gaps, threshold bands, touching/
overlap, diagonal and interior point-to-segment minima, hole boundaries, outside/
cutout/concave board geometry, drill exclusion from board edges, layers/nets/unknown
nets, same-net islands, reliable barrel evidence, scoped neighbours and duplicate/
reordered identity. Negative cases cover missing outline/fill, stale fill, unsupported
rules/selectors, malformed limits and time/memory/candidate/segment/report limits
with subsequent successful jobs. Node exercises the actual DSL, shared addon and
process recovery, including aggregate unknown/fail behavior and continued execution.

The saved-result audit independently reconstructs polygon boundaries, uses winding
containment and a separate BVH segment-distance oracle, verifies witness/source/
net/layer identity, units, thresholds and error bands, source areas, graph evidence,
coverage/counts, original rules and report plots. Versioned snapshots guard complete
finding/geometry/diagnostic/coverage/graph arrays. Negative mutations remove/duplicate
findings, alter IDs/layers/distances/thresholds/polygons, lose diagnostics, truncate
arrays or promote incomplete coverage to pass. Expected values are analytical for
coupons; real-board snapshots retain actual defects and are not clean-board claims.

## Commands and saved evidence

All commands run from this worktree. Rust uses `CARGO_BUILD_JOBS=1` after the initial
focused check and separate outputs. Full native/package commands use a parent-
coordinated slot. A copied main target cache is read-only provenance; Cargo rebuilds
this worktree's sources in its own target directory.

- Focused native library and validation tests: `debugging/geometry-validation-rust.log`.
- Native addon build: `debugging/geometry-validation-native-build.log`.
- Early independent oracle/variant regressions: `debugging/geometry-validation-prebuild-node.log`.

Final evidence is recorded below. Generated output remains ignored under `debugging/`.

## Visual review boundary

Interactive HTML includes rule/layer/status filters, pagination, source highlighting,
board/finding views, shortest segments, actual/required/source tables and coverage.
Browser control execution is unverified: a sibling task received an explicit CUA
security refusal for these local report pages. The parent prohibited alternate
browser/localhost workarounds. Static `overview.png` and `finding.png` are instead
rendered directly from audited saved numeric arrays without executing HTML/scripts,
then inspected using `view_image`. They are static QA images, not browser screenshots.
This limitation is not reported as a successful interactive UI test.

## Corrected measurements and refused runs

The independent audit rejected initial ESPower/NanoScope/NanoSpec/Thunder literals
read from PowerShell's rounded display (`0.15` instead of the captured `0.152`).
The authored JavaScript was corrected to the exact original rule values; board
JSON and selected geometry were not edited. Those spoiled run directories remain
available. The interrupted TPS attempt is also not final evidence.

The initial distance implementation correctly refused NanoScope, NanoSpec and
ThunderF722 after reaching the 5-million exact-check cap. The final distance
kernel chooses the shorter boundary for iteration, queries the larger segment
R-tree, and seeds pruning with exact point-to-segment upper bounds. This is only
an efficiency change: every potentially shorter segment remains tested, the cap
and all original thresholds stay unchanged, and witness orientation is restored.
A large-polygon/rectangle analytic coupon covers bounded distance extraction.

Standalone release probes of the final kernel completed the three refused boards:
NanoScope 4,602,634 exact checks / 29.36 s; NanoSpec 713,564 / 15.31 s;
ThunderF722 592,729 / 12.11 s. These are native CLI probes, not DSL acceptance runs.
The executable/source hashes and numerical JSON are saved in
`debugging/geometry-validation-probes.log` and `debugging/geometry-validation-probes/`.
Exact-check statistics include point-to-segment seed checks and segment-pair checks.
The final named DSL corpus is recorded separately below.


## Final commands (2026-10-23)

Every command below exited **0**. All native work was CPU/F64, one Cargo job;
all Node and fixture execution was sequential. The full addon and installed-package
slot was released to the parent after completion.

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR="$PWD/native/node-addon/target"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS='-C target-feature=+crt-static'
cargo test --release --locked --manifest-path native/analyze/Cargo.toml --target x86_64-pc-windows-msvc --test validation --test geometry --test dc --lib -- --test-threads=1
npm run native:build
npm run typecheck
npm run build
npm run test:package
$env:PCB_BLOCK_BACKEND='cpu'
$env:PCB_BOARD_BACKEND='cpu'
$env:PCB_POST_PLACE_BACKEND='cpu'
node --import tsx --test --test-concurrency=1 tests/analysis-dsl.test.ts tests/pcb-analysis-validation.test.ts tests/pcb-analysis-validation-audit.test.ts tests/pcb-analysis-geometry.test.ts tests/pcb-analysis-dc.test.ts tests/pcb-analysis-dc-ports.test.ts tests/pcb-analysis-runtime.test.ts tests/pcb-native-addon.test.ts tests/public-api.test.ts
npm run test:pcb-analyze -- --all --name validation
node --import tsx scripts/check-pcb-analysis-validation.ts
git diff --check
```

Rust: **50 passed** (4 library, 13 validation, 21 geometry including all eight
captured boards, 12 DC). Node: **85 passed**, no skips/failures, 52.41 s for the
final regression run. Native build, TypeScript check, build and installed package
smokes passed. Installed checks exercise native export, shared process path,
actual DSL/typed result plus the existing GPU-disabled/absent-driver variants.
Eight final authored DSL runs completed; each audit and each sealed-snapshot
re-audit passed. Each scenario executes the expected `unknown`, deliberate `fail`,
and continued `pass` checks. An unknown aggregate is never counted as a pass.

The earlier broad Node run had two harness failures: its legacy DC test expected
unimplemented geometry and its ignored cancel-log folder was absent in this fresh
worktree. The assertion now expects structural `INVALID_CONFIGURATION` for empty
rules, and the test creates its log directory. Production DC code is unchanged.
The earlier native diagnostic adapter also exposed provenance fields to the strict
DSL diagnostic schema; the adapter now projects the supported diagnostic fields
while retaining complete provenance in the native solution. Initial logs remain
saved; final logs listed here supersede them.

Final addon SHA-256:
`3b2fa4f7d7e95457aed03d98c07f530aeb2a3785676f5a1631fd846aca428859`.
Native validation source SHA-256:
`038ebe031df2606d0e4728c5f008ba920053cfab3e02a37e1c3dbd6456427867`.

## Final corpus

All eight retain the original full board, captured source provenance and exact
board-default clearance/edge thresholds. All report partial coverage (including
unavailable open intent). Zero confirmed reports is **not** a clean-board claim.
Wall time covers DSL/process/native return; it excludes independent audit and
PNG rendering. Native time is measured inside Rust. RSS is the OS process high-water
value at return, including Node/earlier work in that worker; it excludes later
report rendering and is not an enforced memory guarantee.

| Fixture | Wall / native s | Peak MiB | Candidates | Exact checks | Regions | Confirmed / uncertain |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| BandAmpV5 | 1.54 / 0.49 | 90.1 | 714 | 93,255 | 319 | 8 / 1 |
| ESPower | 3.82 / 2.72 | 102.1 | 2,237 | 110,220 | 668 | 24 / 33 |
| NanoScope | 35.97 / 33.36 | 230.6 | 13,921 | 4,602,634 | 3,019 | 0 / 198 |
| NanoSpec | 22.19 / 19.19 | 181.7 | 9,982 | 713,564 | 2,509 | 229 / 32 |
| STM32G0_CAN_RS485 | 2.21 / 0.96 | 103.1 | 2,556 | 90,549 | 844 | 2 / 63 |
| TPS63060DSCR | 1.15 / 0.17 | 90.5 | 644 | 17,343 | 172 | 2 / 0 |
| ThunderF722 | 17.53 / 14.92 | 168.9 | 5,837 | 592,729 | 1,895 | 0 / 8 |
| USB_PD_Trigger_5A | 1.69 / 0.77 | 95.1 | 1,344 | 108,698 | 388 | 0 / 9 |

Independent BVH/winding distance recomputation disagreed by at most
`6.495e-15 mm` across the final corpus (audit tolerance `2e-7 mm`). The independent
sweep also checked that every potentially violating clearance pair had a report,
and all source regions had an edge measurement. Versioned snapshots seal complete
arrays/graphs/diagnostics, not only counters. Original exact scripts, seed
expectations, sealed audited expectations and implementation maps remain distinct.

All **16 final PNGs** (overview and selected-finding detail per board) were opened
with `view_image`. Board shape, copper/holes, source highlights, shortest witness,
units/error/threshold, readable source coordinates and explicit partial coverage
were checked. Static coordinate text clipping found during review was corrected
before final regeneration/review. NanoScope shows an uncertain threshold-band gap;
Thunder and USB show explicitly uncertain contacts; the other examples show
confirmed clearance/edge findings. HTML controls remain unverified as explained
above; no HTML/browser scripts were executed for static QA.

Final report directories (relative to this worktree; each contains `validation.json`,
`audit.json`, `index.html`, `overview.png`, `finding.png`, exact script and hash maps):

- [BandAmpV5 report](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-23T12-42-45-907Z-validation-PsnDHJ/index.html) · [overview](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-23T12-42-45-907Z-validation-PsnDHJ/overview.png) · [finding](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-23T12-42-45-907Z-validation-PsnDHJ/finding.png)
- [ESPower report](../../../debugging/pcb-analyze/ESPower/runs/2026-10-23T12-42-49-937Z-validation-2XqdQF/index.html) · [overview](../../../debugging/pcb-analyze/ESPower/runs/2026-10-23T12-42-49-937Z-validation-2XqdQF/overview.png) · [finding](../../../debugging/pcb-analyze/ESPower/runs/2026-10-23T12-42-49-937Z-validation-2XqdQF/finding.png)
- [NanoScope report](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-23T12-42-56-107Z-validation-W5TYhB/index.html) · [overview](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-23T12-42-56-107Z-validation-W5TYhB/overview.png) · [finding](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-23T12-42-56-107Z-validation-W5TYhB/finding.png)
- [NanoSpec report](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-23T12-43-42-953Z-validation-IkiYsS/index.html) · [overview](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-23T12-43-42-953Z-validation-IkiYsS/overview.png) · [finding](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-23T12-43-42-953Z-validation-IkiYsS/finding.png)
- [STM32G0_CAN_RS485 report](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-23T12-44-11-710Z-validation-5XPD0S/index.html) · [overview](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-23T12-44-11-710Z-validation-5XPD0S/overview.png) · [finding](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-23T12-44-11-710Z-validation-5XPD0S/finding.png)
- [TPS63060DSCR report](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-23T12-44-16-676Z-validation-eN1OHa/index.html) · [overview](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-23T12-44-16-676Z-validation-eN1OHa/overview.png) · [finding](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-23T12-44-16-676Z-validation-eN1OHa/finding.png)
- [ThunderF722 report](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-23T12-44-20-132Z-validation-rGPHHp/index.html) · [overview](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-23T12-44-20-132Z-validation-rGPHHp/overview.png) · [finding](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-23T12-44-20-132Z-validation-rGPHHp/finding.png)
- [USB_PD_Trigger_5A report](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-23T12-44-42-246Z-validation-OaLdwt/index.html) · [overview](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-23T12-44-42-246Z-validation-OaLdwt/overview.png) · [finding](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-23T12-44-42-246Z-validation-OaLdwt/finding.png)

## Evidence hashes

The complete artifact and visual-review manifest is
[`debugging/geometry-validation-evidence.json`](../../../debugging/geometry-validation-evidence.json).
It records SHA-256 for each exact input, numerical report, audit, HTML and PNG.
Per-run metadata records board/CAD/native/normalized implementation hashes, base
commit, wall time and worker RSS. The re-auditor checks captured hashes, current
native binary and current sealed fixture expectations before numerical inspection.
Current audit/render source hashes are retained separately from execution source
hashes, so rendering corrections do not masquerade as a new physical run.

| Saved log under `debugging/` | Exit | SHA-256 |
| --- | ---: | --- |
| `geometry-validation-rust-final.log` | 0 | `dab5c54b4176e63504e5959346e5a8bd9f608d44158334d6ed869893147f6703` |
| `geometry-validation-node-regressions.log` | 0 | `608bdc6d89e2bdf0b17c33f9529f056f3c2678b5e004d5672039d7fd752bdd46` |
| `geometry-validation-native-build.log` | 0 | `1d4714a35ba2944abcce8cd6d503d2c7db50e1cfbeeea9e48d35b91c2b098d20` |
| `geometry-validation-typecheck.log` | 0 | `c8e1d473194acf6c35e29cc844bbbfe27b2ab8af927fe40bf2d1316d9da2c5ee` |
| `geometry-validation-build.log` | 0 | `8255e63db728848c78b8a3aa81428d206af154f6d090bf61bb8884c7844a58c4` |
| `geometry-validation-package.log` | 0 | `cce768453878ecb904ffab0324943c08c7e830a94804b0e6ad4fecaff3cbd29c` |
| `geometry-validation-corpus-final.log` | 0 | `9d2641f1794b12af4aa55af2183606d3e2486e1a3047f1017313586562521630` |
| `geometry-validation-reaudit.log` | 0 | `3f312588d5b3687b724acc79b5cd5eb15b44a6228805f4611cc6f72dfff850ef` |
| `geometry-validation-probes.log` | 0 | `96594976b0a93edcf3e787ea71dd5ea3a4f5805b5dfb8ab40d52962275c28785` |

No commit/stage was performed. Native binaries, `target/`, temporary scripts,
package archives and generated reports stay ignored. Parent review and requested
corrections remain the acceptance step.
