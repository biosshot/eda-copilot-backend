# Electrical network P6a — verification

Review delivery, 2026-10-22/23, Windows x64. Working branch `feat/analyze`, base
`466463a5634b8e8c4f8aaeebea101728a2efbf59`. No commit or next-stage work.
See [physical/numerical semantics](../reference/p5-p6-electrical/electrical-network-semantics.md) and
[the task](../archive/tasks/next-task-electrical-network.md). Remaining P6, PCB extraction P7 and
extracted-board PI P8 are not declared implemented.

## E1–E3 corrections — 2026-10-23

The [review](electrical-network-review-2026-10-23.md) remains pending acceptance;
no commit was created. Only its three corrections, regressions and associated
evidence handling/documentation changed since the previous delivery.

- **E1:** pulse/PWL sampling and event discovery share rounded absolute cycle
  boundaries, including the repeat endpoint. Cycle selection corrects the quotient
  by exact boundary comparisons, without snapping adjacent times by epsilon or
  expanding repeat arrays. Independent Rust 0/1 expectations cover immediate F64
  neighbours, exact starts/falls, delay, several periods, mismatched PWL endpoints,
  linear/hold interpolation, full-duty pulses and repeat counts through 100000.
  Production R=1 Ohm tests check the right value at 0.5 s, continuation starting
  exactly there, nearby output values and the end of repetitions.
- **E2:** admission validates the supplied state against the latest retained
  state/context/model before reading its start time. One bounded output-count
  helper serves admission and emission. No state vector is deserialized for budget
  planning. The 1 → 1.001 s / 1 us continuation completes with **1001 strictly
  increasing samples**, correct endpoints and 1 V throughout. A genuinely large
  new interval, modified/foreign state and stop <= start still reject. The 100000
  sample cap and full matrix/workspace memory estimate remain in force.
- **E3:** the explicit-circuit TS adapter checks a safe integer count and
  frequency × port solution limit before `Array.from`/serialization. A sentinel
  replacing `Array.from` proves early refusal for 1e9 points, unsafe integers and
  50001 × 2 ports, without making giant allocations. Small linear/log/single-Hz
  calculations and recovery pass. Direct Rust frequency/port and memory admission
  regressions remain independent of the TS guard.

Re-executed commands from the backend root (the three CPU environment variables
shown below apply to the Node/corpus runs):

```powershell
$env:PCB_BLOCK_BACKEND='cpu'
$env:PCB_BOARD_BACKEND='cpu'
$env:PCB_POST_PLACE_BACKEND='cpu'
cargo test --release --locked --manifest-path native/analyze/Cargo.toml --test electrical --test dc --test conduction -- --nocapture
npm run native:build
npm run typecheck
npm run build
node --import tsx --test --test-concurrency=1 tests/analysis-dsl.test.ts tests/pcb-analysis-dc.test.ts tests/pcb-analysis-dc-ports.test.ts tests/pcb-analysis-dc-audit.test.ts tests/pcb-analysis-electrical.test.ts tests/pcb-analysis-rlc-audit.test.ts tests/pcb-analysis-conduction.test.ts tests/pcb-analysis-geometry.test.ts tests/pcb-native-addon.test.ts tests/public-api.test.ts
node --test scripts/test-pcb-analyze.test.mjs
npm run test:package
npm run test:pcb-analyze -- STM32G0_CAN_RS485 USB_PD_Trigger_5A ThunderF722 --name rlc
```

Results: **17 electrical + 12 DC + 20 conduction Rust tests; 137 Node tests;
4 launcher tests; native/typecheck/build/package all pass.** Package checks ran
normal, GPU-disabled and unavailable-driver modes; the installed consumer is
`C:/Users/kiril/AppData/Local/Temp/eda-backend-package-0yrpYp`.
[Logs](../../../debugging/analysis-dsl/electrical-e123-node.log),
[Rust](../../../debugging/analysis-dsl/electrical-e123-rust-final.log),
[package](../../../debugging/analysis-dsl/electrical-e123-package.log) retain actual output;
the other commands use the same `electrical-e123-*.log` prefix.

Three newly saved, audited RLC runs contain **24 complete calculations, 36 plots,
6 intentionally failing droop checks and no unknown required measurements**:

| Fixture | Current run | Audit |
| --- | --- | --- |
| STM32G0_CAN_RS485 | [2026-10-23T06-42-35-283Z-rlc-TniloO](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-23T06-42-35-283Z-rlc-TniloO/index.html) | pass |
| USB_PD_Trigger_5A | [2026-10-23T06-42-37-113Z-rlc-K56Dcj](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-23T06-42-37-113Z-rlc-K56Dcj/index.html) | pass |
| ThunderF722 | [2026-10-23T06-42-38-761Z-rlc-QjHJ3m](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-23T06-42-38-761Z-rlc-QjHJ3m/index.html) | pass |

[Comparison](../../../debugging/analysis-dsl/electrical-e123-review/comparison.json)
records exact equality against the previous 24 results for axes, voltages,
currents, impedances, losses, quality, counters and physical final states, excluding
run-specific context IDs and elapsed/RSS metadata. Board/script hashes are equal;
all captured implementation hashes were rechecked. These fixtures use step sources;
the independent periodic-source regressions above cover the newly corrected case.

Current addon SHA256:
`94b1ba59d1eeb0ec58a3a1c08aa8added6e381318a824ee6a37f6c2a5513cf99`;
it exactly matches the installed package binary. Archive SHA256:
`86d20b1ff4e5285e746d3d6de5dbde8a33ceeed420b3a812ea310969f7c0937d`.
All three new report implementation fingerprints:
`2b0d3786139f2875ff7c42b6561075d80176c4e9e20613dd19951f8c73f757a7`.
[Correction diff](../../../debugging/analysis-dsl/electrical-e123-review/corrections.patch)
and [manifest](../../../debugging/analysis-dsl/electrical-e123-review/manifest.json)
identify this delivery separately from the old review artifacts.

The eight full FEM DC boards were **not** rerun: their executed evidence is
retained with its original binary hash. The RLC evidence helper now explicitly
excludes the independent circuit adapter as well as the Rust electrical module
from FEM source-parity comparison. The unchanged dispatch selects that adapter
only for explicit circuits; board FEM never calls it. All **43 remaining captured
shared/FEM files** match the actual DC snapshots. Focused DC tests and installed
package DC checks were rerun on the new addon. No CAD inputs, physical hypotheses
or analytical tolerances changed. No new browser visual pass was performed: the
renderer is unchanged and plot values are audited against the exactly equal data;
the screenshots below remain historical. Linux/macOS and full `npm test` were not
rerun. No work on the next stage was performed.

## Original delivery evidence (historical)

The sections below retain the original delivery's commands, runs, screenshots and
hashes. They are not claims that those old artifacts used the correction binary.

## Delivered path

Existing direct JavaScript DSL → disposable scenario process → shared addon
`AnalysisElectricalSession` → ordinary `pcb-analyze::electrical` CPU/F64 module.
Sparse MNA compilation/stamping, `faer` LU, DC, real-block complex driving-point
Z(f), adaptive BE/step doubling, source events and compatible state continuation.
One bounded compiled model/symbolic/numerical factor/latest state per session.
Observations are passive; there is no board connection, hidden parasitic, shunt,
GPU initialization, second addon, compute runtime or alternative test solver.

Existing builders/schema transport are reused. Local R/C/L mapping is exactly
p/n. Scalar/series units, complex parts/magnitude/phase and checks/plots use the
existing DataValue/report path. Unsupported AC observations and standalone maps
stay unavailable. Explicit board/conductor FEM DC remains on its prior path.
RoutingBoardV2, exporter, saved PCB JSONs, placement/F32 and dependencies are unchanged.

## Commands and actual outcomes

Commands below ran from the backend root. Focused Node/corpus runs selected
`PCB_BLOCK_BACKEND=cpu`, `PCB_BOARD_BACKEND=cpu`, `PCB_POST_PLACE_BACKEND=cpu`.
Package checks independently test their normal/disabled/unavailable-driver modes.

| Command | Exit / result | Local log |
| --- | --- | --- |
| `npm run native:build` | 0; production shared addon | [native](../../../debugging/analysis-dsl/electrical-native-production-final.log) |
| `cargo test --release --locked --manifest-path native/analyze/Cargo.toml --test electrical --test dc --test conduction -- --nocapture` | 0; 15 electrical + 12 DC + 20 conduction = **47** | [Rust](../../../debugging/analysis-dsl/electrical-rust-final.log) |
| Focused Node command below | 0; **134 pass**, no failures/skips | [Node](../../../debugging/analysis-dsl/electrical-node-final.log) |
| `node --test scripts/test-pcb-analyze.test.mjs` | 0; **4 pass** | [launcher](../../../debugging/analysis-dsl/electrical-launcher.log) |
| `npm run test:pcb-analyze -- --list` | 0; dc/geometry/import/mesh plus rlc for the three selected fixtures | [list](../../../debugging/analysis-dsl/electrical-list.log) |
| `npm run typecheck` | 0 | [typecheck](../../../debugging/analysis-dsl/electrical-typecheck-final.log) |
| `npm run build` | 0 | [build](../../../debugging/analysis-dsl/electrical-build-final.log) |
| `npm run test:package` | 0; isolated installed archive, all three smoke modes | [package](../../../debugging/analysis-dsl/electrical-package-final.log) |
| `npm run test:pcb-analyze -- --all --name dc` | 0; **8 boards / 32 native complete**, all saved audits pass | [DC corpus](../../../debugging/analysis-dsl/electrical-dc-corpus.log) |
| `npm run test:pcb-analyze -- STM32G0_CAN_RS485 USB_PD_Trigger_5A ThunderF722 --name rlc` | 0; **3 boards / 24 native complete**, all saved audits pass | [RLC corpus](../../../debugging/analysis-dsl/electrical-rlc-final.log) |

```powershell
node --import tsx --test --test-concurrency=1 `
  tests/analysis-dsl.test.ts tests/pcb-analysis-dc.test.ts `
  tests/pcb-analysis-dc-ports.test.ts tests/pcb-analysis-dc-audit.test.ts `
  tests/pcb-analysis-electrical.test.ts tests/pcb-analysis-rlc-audit.test.ts `
  tests/pcb-analysis-conduction.test.ts tests/pcb-analysis-geometry.test.ts `
  tests/pcb-native-addon.test.ts tests/public-api.test.ts
```

This includes D1–D3/M1, old physical guards, strict addon export/declaration/version
checks, actual production DSL and Node transport. Package smoke executes the task's
DC/Z/time/continuation example through the installed public API and checks prior
PCB/schematic/workers. Installed consumer:
`C:/Users/kiril/AppData/Local/Temp/eda-backend-package-tEhJtG`.
No full `npm test`, Linux/macOS execution or remote CI run is claimed.

## Independent numerical references

Saved Rust tests use divider/branched-network equations, complex RC/RL and
series/parallel RLC impedances (both components, including the series resonance
imaginary zero with an absolute Ohm floor), RC/RL exponentials and the closed-form
underdamped RLC step response. Finite ramps use the analytic step convolution
(200 midpoint samples for the short RL/RLC ramp); RC ramp/discharge uses its
closed-form expression. The new solver's first output is not the oracle.

| Measurement | Observed | Criterion |
| --- | --- | --- |
| DC divider/branches, multiple sources, source-current sign, DC C/L/ESR/ESL/DCR | passed, analytical absolute checks 1e-12 on these nonzero scales | ≤1e-8 relative or physical zero floor |
| Explicit/local-model R/C/L equivalence | matching complex results through production DSL | same mapped model |
| RC/RL/series/parallel RLC complex Z | both real/imaginary checks passed; absolute floor at resonance | ≤1e-6 relative away from zeros/poles |
| RC/RL/transient and regular ideal V step through R into C | passed pointwise physical reference checks | ≤0.5% characteristic scale |
| Damped RLC maximum trace error, rtol 1e-7 → 1e-8 | **0.00331461 → 0.00129119 V**, i.e. **0.33146% → 0.12912%** of a 1 V step | ≤0.5%, improves with refinement |
| Damped RLC energy error | **1.44727e-4 → 5.49760e-5** of exchanged energy | ≤0.005, improves with refinement |
| RL maximum current error | **2.58026e-5 → 8.28491e-6 A** | ≤0.5% of 0.5 A scale |
| AC / steady sine transient agreement, including phase | maximum pointwise error **1.59572e-5 V** for \|Z\|=9.980319 Ohm and 1 A excitation | ≤0.5% amplitude scale |
| RC output cadence 10 us → 1 ms | identical internal counters and final values; energy error **1.18433e-4** | output cadence must not set dynamics |
| Long vs split transient, event after continuation boundary | maximum voltage difference about **5.55e-16 V** in the saved pulse case | same global time/events |
| Fast 1 ns RC with 1 ms output cadence | dissipated energy **4.99855e-10 J**, stored **5e-10 J**, relative energy error **9.66754e-5** | resolve internal dynamics, ≤0.5% energy |

BE numerical damping is measured against the same physical RLC equation and
decreases with tighter tolerances; it is not labelled ESR. Residuals/KCL/DC/AC
power and signed time work/loss/stored-energy discrepancies remain in reports.
Numerics meanings, separate V/A scales, fixed guards and output interpolation are
specified in the semantics document.

Negative tests cover floating/dependent/inconsistent sources and ideal constraints,
unknown mappings/models, missing observations, complex extrema/zero phase,
changed/foreign/unknown state, invalid absolute stop, budgets/output/front resolution,
and recovery. Real synchronous native abort/timeout records killed worker PIDs,
awaits exit and succeeds on the next public runner; see
[cancellation evidence](../../../debugging/analysis-dsl/electrical-cancel.log).

## Real-board scenarios and visual reports

All coefficients are explicit test hypotheses, not extraction or a validated BOM
model. Verified bindings: STM32 U2.5/U2.2 → U3.4/U3.7, U4.5/U4.8, U5.8/U5.5 on
+3V3; USB J1.A9/J1.A12 → J2.2/J2.1 through the VBUS_IN/VOUT context; Thunder
U9.5/U9.2 → U2.64/U2.63, U4.8/U4.4 on +3.3V. Source bindings and all returns are
retained in the direct JS/log/report, with validation against actual fixture pads.

Each report has 8 calculations, 12 plots and 6 checks: **two intentionally strict
droop failures**, each followed by a passing loss check and further plots. These
are expected `check:fail`, not rejected calculations. No unknown required measurement.

| Fixture / final report | Wall time¹ | Peak worker RSS² | Z reference relative error | Time refinement relative error³ | Maximum relative energy error |
| --- | ---: | ---: | ---: | ---: | ---: |
| [STM32G0_CAN_RS485](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-22T20-53-05-122Z-rlc-YC5giu/index.html) | 1.267 s | 98.4 MiB | 4.64e-16 | 3.38e-5 | 5.92e-7 |
| [USB_PD_Trigger_5A](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-22T20-53-07-968Z-rlc-OkECZu/index.html) | 1.178 s | 94.6 MiB | 4.64e-16 | 6.03e-5 | 1.02e-6 |
| [ThunderF722](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-22T20-53-10-306Z-rlc-OaE4Da/index.html) | 1.489 s | 125.7 MiB | 4.64e-16 | 3.11e-5 | 5.42e-7 |

¹ Runner wall time, excludes copying the already computed board DC / rendering;
other independent verification processes ran concurrently, not a controlled benchmark.
² OS process high-water RSS through native calls, includes Node/host board/earlier
calculations; excludes final serialization after the last native call. Not a hard
RSS ceiling or per-factor allocation measurement.
³ Pointwise coarse/fine voltage difference divided by actual droop scale, floor
1 mV, not by the large rail voltage; guard 0.005.

[Overview](../../../debugging/pcb-analyze/rlc/index.html),
[full matrix/quality/inputs/hashes](../../../debugging/analysis-dsl/electrical-matrix.json).
Each run saves exact scenario.js, normalized resolved requests in rlc.json,
implementation.json, audit.json, wall/RSS, standalone HTML/SVG and copied
board-dc.json/HTML/source. Missing current-run data is not fetched from old successes.

The versioned launcher calls `auditRlcReport`. Its positive baseline must pass
before mutation tests run. Saved negative tests corrupt case/result identities,
inputs, measurements/axes/units, Z imaginary data, quality, expected failures and
following checks/plots, refinement, final state and continuation; every corruption
must fail the audit. Plot arrays are compared to the actual solution's transformed
data, not merely checked for nonempty SVG.

Visual QA used the in-app browser on a task-owned localhost server: all three page
headers/statuses/bindings/DC summaries, magnitude and time voltage curves; additionally
STM32 phase, Thunder inductor current and USB fail→pass table. Shared axes, SI units,
legends, explicit hypotheses and continuation at 500 us were visible. No AC PCB maps.
[Thirteen screenshots](../../../debugging/pcb-analyze/rlc/screenshots/README.md)
include page/frequency/time views of each final run. Temporary tab/server closed.

## Old DC preservation and binary provenance

[DC comparison](../../../debugging/analysis-dsl/electrical-dc-comparison.json)
compares all 32 calculations against the previously saved accepted baseline:
measured voltages, complete physical field/current/loss/quality payloads are
**exactly equal**. Saved PCB files and authored dc.js inputs are unchanged.

The eight-board pass used addon SHA256
`4c2ff2d39f417c008bd2118af26276a0b8edfa1c080ad00a32d063208e41da81`.
Subsequent fixes affected only the independent electrical Rust module; the final
RLC/package addon is
`fe31a0ce8399fa267866216602d8503cf046e4a04c4b10b69b601a6e487ffcfb`.
**The eight full DC boards were not rerun with that later binary.** Instead every
DC snapshot's **44 shared/FEM source files** were checked byte-normalized against
current source, excluding only `native/analyze/src/electrical/`, which DC never
calls. Final-node tests/package rechecked the actual DC path and strict old exports.
Both binary identities remain in the copied evidence; no hashes/results are rewritten
to imply a replay. This avoids cyclic identical full-board numerical runs.

All three final RLC runs have implementation fingerprint
`5c4e9079edd44f52ea17174d4b518abfbd5bb16526cb62c6aa8696fe01c28af9`.
Every captured implementation file and exact script bytes were verified after
execution. Installed archive native hash equals the final source-tree binary;
archive SHA256 `ce58e9d16a662601ed28c6d2040ae228d4041364f50c891a8ea50440560df356`.
The [review manifest](../../../debugging/analysis-dsl/electrical-review/manifest.json)
and [uncommitted diff](../../../debugging/analysis-dsl/electrical-review/changes.patch)
identify the complete change set, including new files.

## Corrections during verification and limits

- Fixed duplicate stop output from F64 ceil rounding on continuation. Output axes
  are strictly increasing and end once at stopS.
- Fixed state equality across native/Node JSON representation of `20.0` vs `20`;
  equal F64 values compare equal while structure/identity/time/actual values stay strict.
- Nonimpulsive ideal V steps through R into C work. An ideal V jump directly across
  an ideal capacitor rejects rather than silently changing its stored charge.
- Retained the fast RC counterexample: the former large-step heuristic completed
  with **50% energy error** ([reproduction](../../../debugging/analysis-dsl/electrical-fast-scale-before.log)).
  Physical RC/LR/LC step seeding plus the time-energy guard prevents that false
  success. The 1 ns case meets its independent energy reference. The original
  **1 ps / 1 kA at t=10 ms** case explicitly returns `NETWORK_STEP_UNDERFLOW`
  under the requested F64 current/time tolerances. It is **not** claimed solved;
  the original parameters remain saved in both Rust and production Node tests.

The session supports its latest known compatible final state, not an unlimited
archive or arbitrary cross-model/context state transfer. Sampled extrema may miss
internal peaks. Singular/pole/nonfinite and exhausted budget/resolution cases reject.
No nonlinear device/converter/feedback, full multiport matrices, physical extraction,
skin/proximity, FFT/fitting, GPU or other next-stage analyzer is implemented.
