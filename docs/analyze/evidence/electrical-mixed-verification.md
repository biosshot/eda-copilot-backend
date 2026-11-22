# Mixed delay/RLC verification package

Implementation follows the approved checkpoint in
[next-task-electrical-mixed-networks.md](../archive/tasks/next-task-electrical-mixed-networks.md).
The package awaits parent review; this is not closure of all P6 or P11.
CPU/F64 analysis, ordinary GPU-default shared addon, placement F32 unchanged.

## Saved evidence and reproducibility

Local generated output: `debugging/p6-mixed-verification/` (ignored). Original
`debugging/p6-verification/` evidence remains intact. The shared evidence runner
accepts `--mixed`; it executes the direct authored scenario, records source/script/
board/addon hashes, actual wall time and worker high-water RSS, and independently
audits the saved output. Installed-package verification uses the compiled worker
and packaged ordinary addon, including numeric-contract, cancellation and recovery.

```powershell
$env:CARGO_BUILD_JOBS='1'
$env:CARGO_TARGET_DIR=(Join-Path (Get-Location) 'debugging/p6-target')
$env:P6_MIXED_EVIDENCE=(Join-Path (Get-Location) 'debugging/p6-mixed-verification/native')
cargo test --manifest-path native/analyze/Cargo.toml --test mixed --test coupled -j1 -- --test-threads=1
npm run native:build
npm run typecheck
npm run build
node --import tsx tests/helpers/electrical-core-verification.ts --mixed debugging/p6-mixed-verification/direct-dsl
node tests/helpers/electrical-core-package.mjs --mixed debugging/p6-mixed-verification/installed
python tests/helpers/electrical-core-render.py debugging/p6-mixed-verification/direct-dsl/standalone/result.json debugging/p6-mixed-verification/direct-dsl/STM32G0_CAN_RS485/result.json debugging/p6-mixed-verification/direct-dsl/ThunderF722/result.json
node --import tsx --test --test-concurrency=1 tests/electrical-mixed-audit.test.ts tests/electrical-core-adapter.test.ts
```

Heavy slot was coordinated with Thermal/P7; all native builds/solves are jobs1
and use the P6 target. No unrelated worktree or captured board input is changed.

## Numerical contract and independent references

- `mixed.rs`:13 focused cases, including loaded finite-ramp RC, damped-sine RLC
  convolution, Erlang repeated-pole RC bounce expansion, nonzero steady rail,
  attenuation, mutual even/odd L/C, conditional fit versus explicit RC,
  settled sinusoid, permutations/renaming/unequal zero-delay wave references,
  zero-energy gauge versus physical-charge ambiguity, zero stimulus and an
  unresolved ideal wave-current loop. Source/load/closure/band/port/budget/time
  refusals remain explicit. Each positive native request/result is saved.
- The RC reference has Hdc=1/3 and tau=1ns after2ns. The RLC coupon has two
  matched lines (2/3ns), R=5Ohm, L=50nH, C=20pF; its convolution uses the
  independent second-order damped-sine response. Reflections use GammaS=-1/3,
  GammaL=-1+1.2/(1+s*0.66ns) and exact first-arrival/Erlang expansion.
- Native and saved-result references do not call the production assembly or
  integrator. All four complex S of the connected graphs are compared with
  independent series/shunt ABCD formulas, including both reflection phases.
  Nested connect is exercised directly in the RLC DSL. Native frequency cases include DC; the existing public frequency interval requires a positive start (1MHz). Its rc-input schema requires positive C: the matched DSL coupons declare1aF, whose finite-front perturbation is below the1uV absolute reference floor.
- Fixed limits: original residual<=1e-8, reference error<=1.001mV for1V swing,
  independent global refinement<=0.5mV. The energy norm is fixed before tests:
  `abs(Wsource)+WR+Watten+abs(DeltaUlumped)+abs(DeltaUdelay)+Dbe`.
  `abs(balance)<=1e-4*norm`; exactly zero norm uses1e-20J. Raw balance is also
  reported before subtracting Dbe; signed history error never corrects acceptance.
- Direct native audit currently covers16 captured time requests plus two complete
  frequency requests. Maximum main-coupon reference error0.242742mV;
  maximum normalized energy balance across captured cases5.92621e-5.

## Verification results

| Gate | Result | Saved evidence |
| --- | --- | --- |
| Final mixed native incl frequency/admission/finite guards |13/13 PASS,30.04s | `native-admission-final.log`, `native/`, `native-audit.json` |
| Touched pure coupled regression |9/9 PASS | `native-all-final.log` |
| Touched delayed regression after history fixes |4/4 PASS | `delayed-admission-final.log` |
| Finite source/load TS adapter regression | PASS | `adapter-tests.log` |
| Final ordinary GPU-default addon | PASS,6m20s; SHA256 d83c6ad7...9b0d2ca | `native-build-admission-final.log`, `manifest.json` |
| Typecheck/build | PASS | `typecheck-final.log`, `build-final.log` |
| Direct standalone / STM32 / Thunder |9 calculations each; saved independent audit PASS | `direct-dsl/*/{result,audit,implementation}.json` |
| Installed compiled worker/shared addon |9 calculations + independent audit PASS; f32-rte-ftz-v1 | `installed/manifest.json`, `installed/audit.json` |
| Timeout / pre-abort / active abort / recovery | PASS | `installed.log` |
| Saved-result corruption audit | PASS,12 mutations | `audit-negatives.log` |
| Static PNG render + actual visual inspection |6/6 inspected; no interactive HTML claim | `direct-dsl/*/electrical-mixed*.png`, `render.log` |

| Context | Actual scenario wall time | Worker high-water RSS | Maximum native plan |
| --- | --- | --- | --- |
| Standalone |807.16ms |88.29MiB |131.71MiB |
| STM32G0_CAN_RS485 |842.03ms |97.22MiB |131.71MiB |
| ThunderF722 |978.72ms |129.76MiB |131.71MiB |

Worker RSS is OS high-water memory including Node and previous scenario calls;
the native plan is conservative admission, not a measured allocation peak.
Actual board-context terminals are STM32 U3.48 to U4.1 (CAN_TX), and Thunder U2.58
to TX1.1 (UART1_TX), with a recorded real GND pad as explicit common return. The
authored coupons do not extract those traces or infer the devices' own models.

Worst direct reference error is0.242743mV (limit1.001mV); maximum independent
coarse/refined change is0.121278mV (limit0.5mV). Maximum physical energy residual
is5.92621e-5 of the fixed norm (limit1e-4). All four complex S match independent
ABCD within1e-8. Raw balance is visible before Dbe and decreases under refinement:

| Coupon | Raw balance coarse / refined [J] | Dbe coarse / refined [J] |
| --- | --- | --- |
| RC |1.47253e-14 /7.39261e-15 |1.67598e-15 /8.40019e-16 |
| RLC |1.18579e-14 /5.92671e-15 |3.56085e-15 /1.78051e-15 |
| Reflections |2.73337e-14 /1.37934e-14 |5.85826e-15 /2.92423e-15 |

Earlier failed measurements remain visible in `journal.md` and failed logs.
Expanded cases exposed source-front/history quadrature accuracy and a terminal
history-rounding defect; these were fixed without changing the pre-agreed norm
or tolerances. Unequal-reference invariance uses the task's0.5mV global numeric
bound, not bit identity between different adaptive wave coordinates.

## Remaining scope

Grounded canonical delay/RLC/relaxation mixed assembly is the implemented bound.
Infinity/noncanonical joins still require an explicit physical terminal map;
physical nonunique steady charge and arbitrary incoming history/continuation
are refused. General resonant/active fitting, arbitrary inferred delays, extracted
PCB channels, SI/TDR/eye/timing and P8/P11 product closure remain outside this task.
Finite producer bands and conditional fit assumptions survive in result provenance
and derived series. Static PNGs are inspected; interactive HTML is not claimed.
