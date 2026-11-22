# Analysis DSL — coverage and verification

Implementation contract, 2026-10-22. Physical executors are outside this stage.
The authoritative typed signatures live in `src/analyze/analysis-dsl/spec-doc.d.ts`.

| Public name | Signature / category | Current execution | Future executor |
| --- | --- | --- | --- |
| runAnalysisDsl | ({code,board?,profiles?,models?}, {signal?,timeoutMs?,filename?,maxEntries?,maxOutputBytes?}) → Promise scenario report | Isolated JavaScript, await, diagnostics | None |
| analysis | ({board?}?) → context | Snapshot and report | None |
| pad, padId, component, net, region | explicit selector constructors | Strict descriptions / board reference validation | None |
| stackup; ic, resistor, capacitor, inductor, device | typed physical/component descriptors | Strict descriptions; no CAD mutation | Physical interpretation |
| profiles.environment, profiles.sample | environment description / time sample | Explicit profiles and waveform evaluation | None |
| a.port, channel, differential | contact pair / path / pair descriptions | Strict references and context ownership | Extraction |
| a.conductor, line, circuit, thermalNetwork | standalone model descriptions | Shape / units / graph validation | Physical evaluation |
| a.model, scene, antenna, network, connect | model and network descriptions | Explicit resource / port validation | Physical evaluation |
| a.variant | ({changes}) → context | Records explicit derived snapshot intent; rebuild limitations | Geometry rebuild/refill |
| a.geometry | scope, rules → builder | Structural prepare; run rejects ANALYZER_NOT_IMPLEMENTED | Full user geometry / DRC |
| a.dc | board sources/loads OR circuit OR conductor + currentA/voltageV | Structural prepare; run rejects | DC |
| a.pi | board/circuit; domain frequency/time, frequency OR time/initial | Structural prepare; run rejects | AC / transient PI |
| a.thermal | board heat/losses/cooling OR network; environment, initial, time/steady | Structural prepare; run rejects | Thermal |
| a.electrothermal | electrical, thermal, couplings, common time | Structural prepare; run rejects | Coupled solver |
| a.impedance, tdr | target, explicit frequency or time / termination | Structural prepare; run rejects | Line extraction / TDR |
| a.si | channels OR network, drivers, receivers, time | Structural prepare; run rejects | SI |
| a.eye | signal, bitRateBps, discardS, phaseS, thresholdV | Structural prepare; run rejects | Eye processing |
| a.emi | risk/conducted/coupling; explicit excitations / measurements | Structural prepare; run rejects | EMI |
| a.em | target, method, frequency, space, outputs / excitation | Structural prepare; run rejects | EM |
| a.networkParameters | network, frequency, representation, referenceOhm | Structural prepare; run rejects | S/Z/Y evaluation |
| builder.prepare, run | () → Promise preparation / typed result | prepare distinguishes structural validity from physical readiness; run dispatches | Executor |
| result accessors, timing; min/max/abs/at/real/imaginary/magnitude/phaseDeg | typed data / sampling or propagation timing | Data/validity transport; unavailable for unimplemented operations | Numerical processing |
| a.check, log, plot, report | report operations | pass/fail/unknown, bounded serializable records, no fabricated files | None |
| a.sweep | explicit values / cartesian or zip + JS evaluate callback | Enumerates values; preserves failed attempts | None |
| a.solve, optimize | parameters, evaluate, targets/constraints/budget | Typed, explicitly unavailable; callback is not invoked | Search algorithm |

Excluded legacy forms: assign, converters, solver(openems), SI channel,
transmitter/receiver/stimulus/aggressors setters, separate timing/DDR solver.

## Exact builder vocabulary

For every entry `K.P` below, the signature is
`P(value: NonNullable<Configurations[K][P]>): Builder<K>`. `Configurations`, all
description/result types and global signatures are in the standalone reference.
Each builder also has `prepare(): Promise<Preparation>` and
`run(): Promise<Results[K]>`, with no `then` method. Shared setters are `name`,
`numerics`, `budget`. The [machine inventory](../../../../debugging/analysis-dsl/api-coverage.json)
contains all **144 setter signatures across 12 builders**, extracted from the
actual TypeScript declarations; tests compare the runtime method sets to schemas.

| Builder | Additional setters |
| --- | --- |
| geometry | scope, rules |
| dc | components, stackup, materials, sources, loads, temperatureC, circuit, conductor, currentA, voltageV |
| pi | components, stackup, materials, sources, loads, temperatureC, circuit, domain, frequency, frequencyHz, time, initial |
| thermal | components, stackup, materials, network, heat, losses, cooling, environment, initialC, initial, domain, atS, time |
| electrothermal | stackup, materials, electrical, thermal, couplings, time |
| impedance | components, stackup, materials, target, method, frequency, frequencyHz, temperatureC |
| tdr | components, stackup, materials, target, referenceOhm, riseS, termination, time, temperatureC |
| si | components, stackup, materials, channels, network, drivers, receivers, time, temperatureC |
| eye | signal, bitRateBps, discardS, phaseS, thresholdV |
| emi | components, stackup, materials, sources, loads, temperatureC, circuit, kind, signals, rules, excitations, measurement, time, frequency, frequencyHz, spectrum |
| em | components, stackup, materials, target, method, frequency, frequencyHz, space, outputs, excitation, excitations, observation, temperatureC |
| networkParameters | network, frequency, frequencyHz, representation, referenceOhm |

Description signatures are explicit rather than catch-all dispatch:

| Name | Signature / type in reference |
| --- | --- |
| analysis | `({board:RoutingBoardV2}?) → AnalysisContext`; omitted means boardless |
| pad / padId / component / net | `(designator,number)` / `(id)` / `(designator)` / `(name)` → discriminated Selector |
| region | `({layers,polygon:PolygonMm}) → region Selector` |
| stackup | `(StackupInput) → StackupDescription` (existing RoutingStackup adapter) |
| ic / resistor / capacitor / inductor / device | `ComponentBase` / `ResistorInput` / `CapacitorInput` / `InductorInput` / `ComponentBase & {model}` → ComponentDescription |
| profiles.environment / sample | `(Environment) → Environment` / `({environment,atS}) → number` |
| a.port | `({positive:PadSelector|PadSelector[],negative:...}) → Port` |
| a.channel / differential | `({from,to,via?,scope?}) → Channel` / `({positive,negative}) → Differential` |
| a.conductor / line | `({lengthMm,widthMm,thicknessMm,material}) → Conductor` / `(LineInput) → Line` |
| a.circuit / thermalNetwork | `(CircuitInput) → Circuit` / `(ThermalNetworkInput) → ThermalNetwork` |
| a.model | `(LocalModel) → LocalModel` (strict kind schemas, including cooling tables) |
| a.scene / antenna | `({model,parameters?}) → Scene` / `({structure,feed,models?}) → Antenna` |
| a.network / connect | `({model,ports}) → Network` / `({networks,connections,ports}) → Network` |
| object.port | `(name:string) → Port`; explicit declared names only |
| a.variant | `({changes:VariantChange[]}) → AnalysisContext`; intent only, rebuild unavailable |
| a.check / log / plot / report | `(CheckInput) → CheckRecord` / `({level,message,data?}) → void` / `(PlotInput) → void` / `() → Report` |
| a.sweep | single `parameter` + callback, or `parameters` + cartesian/zip + callback → Promise SweepResult |
| a.solve / optimize | `(SolveOptions)` / `(OptimizeOptions)` → Promise SearchResult; reject without invoking evaluate |
| result accessors | Per-result interfaces, with explicit selectors/ports/names; `DataValue` operations preserve unit/validity |
| si.timing | `(TimingOptions sampling|propagation) → TimingResult`; validated, unavailable processing |

## Actual behavior and boundaries

- `runAnalysisDsl` and `analysisDslDocumentation` are root library exports; there
  is one runner, no `/analyze` package, executor override, MCP wrapper or CAD call.
  `dist/analysis-spec-doc.d.ts` imports the existing public `/types` transport;
  PCB Layout's `dist/spec-doc.d.ts` remains byte-identical to its source.
- The runner accepts ordinary JS with `await`, never TS annotations or a custom
  AST language. Each scenario owns one disposable workerpool **process**, using
  the existing fork options/resource paths. The parent deadline includes startup;
  timeout/abort terminates that process and awaits exit, including loops after
  `await`. A later scenario can run. No general compute runtime/queue is added.
- Default limits: 120000 ms, 1000 entries across contexts, 4 MiB output;
  `maxOutputBytes` accepts 4096–64 MiB, with 2048 bytes reserved for errors.
  Context headers count against bytes. Errors are bounded and retain source
  filename/line/column plus previously accumulated reports. Append-only progress
  sends each new entry once instead of resending old plots/reports over IPC.
- `node:vm` is **not** a security boundary for hostile JavaScript. process,
  require, file/network access and timers are not supplied as convenience globals.
  Limits bound lifecycle/report output, not total V8/native RSS or concurrent
  public scenario admissions. Host input/limit errors reject; script/solver errors
  return `status:'failed'` with partial reports. Checks do not control this status.
- All builders snapshot inputs and replace complete sections. `.prepare()` is
  recorded even if a later user line fails. It distinguishes `structurallyValid`
  from `physicalReady:false` / `executorAvailable:false`. Production `.run()`
  validates and rejects with `ANALYZER_NOT_IMPLEMENTED`; no preparatory native
  operation is presented as a user analysis or DC solver.
- Contexts use frozen V2 snapshots, context IDs and board SHA-256 identity. Strict
  schemas validate units/field types, exclusive modes, supplied graph/port/model
  references, material/layer names and order. Explicit component groups are not
  extended. Constant component loads resolve independent supply/return residuals
  with exact GND/equal defaults; current-sum tolerance is
  `16 * Number.EPSILON * max(1,total,sum)`. Source/load waveform initial values
  must match the explicit operating point.
- Dynamic pin-current consistency, supported component recognition/default load
  groups and physical/model applicability remain explicitly **unverified**.
  No material properties are guessed. The stackup adapter uses 0.03479 mm/oz,
  excludes mask from board thickness, and rejects inconsistent sums (1e-6 mm).
  The existing conduction profile receives only explicit compatible fields:
  layer dimensions, conductivity with a supplied material state, and plating
  thickness. No barrel material or bore convention is invented.
- Host model catalogs may have project namespaces. They and anonymous local
  descriptions are snapshotted; supplied resource/table payloads are not loaded
  from files or interpreted as physics. Local cooling tables are only validated
  descriptions. Automatic report entries reference provided model IDs/hashes,
  rather than duplicating complete model payloads. Executor tasks retain them.
- Variants retain typed changes and stable handles with
  `VARIANT_REBUILD_REQUIRED`; no geometry/refill/material change is silently
  applied or solved against old copper. Old result/final-state data cannot be
  transferred to an incompatible snapshot.
- Tests inject an explicitly named test executor into the internal scenario
  kernel; production defaults never use one. It demonstrates dependent results,
  extracted-network handoff, callbacks in JS and complete finite task envelopes.
  Missing accessors return unavailable; only real scalar min/max identities are
  computed. Series math, complex transforms, timing/eye and S/Z/Y processing are
  unavailable. No numerical optimizer or one-callback search substitute exists.
- `check` keeps pass/fail/unknown, inclusive bounds, units and validity. Missing,
  nonfinite, incompatible or invalid conditions remain unknown. `plot` stores an
  explicit/inferred typed description with supplied data, `rendered:false`, and
  no fabricated image/file. Explicit-value sweep visits every candidate, records
  callback errors and continues; cancellation terminates the scenario.

## Reproducible checks and artifacts

| Check | Result |
| --- | --- |
| Typecheck / package build | Passed on final sources |
| Focused DSL suite | **28 passed**, no skips/failures; includes eight structural examples, standalone declaration typecheck, dependent test execution, before/after-await termination, recovery, byte limits and incremental report progress |
| Existing PCB Layout/native/runtime/public API regressions | **58 passed**; `tests.log` contains 82 passing tests (these 58 plus the earlier 24 DSL tests; the final 28-test DSL run supersedes that part) |
| Installed archive | **3/3 modes passed**: normal environment, GPU disabled, GPU drivers unavailable; both documents, real DSL worker, existing PCB worker and eight import fixtures |
| Public runner evidence | Structure-only line succeeds with physicalReady=false; DC fails ANALYZER_NOT_IMPLEMENTED and retains fail/unknown checks; supplied-table report succeeds without rendering |
| API/document inventory | 12 builders, 144 setters; runtime vocabulary checked; original PCB documentation byte-identical |
| Diff/patch integrity | `git diff --check` and reverse `git apply --check` passed; no trailing whitespace in task files |

Final installed consumer:
`C:\Users\kiril\AppData\Local\Temp\eda-backend-package-qUi2vl`.
The archive is produced by the existing package checker under `.artifacts/`;
no package is published.

Commands run from the backend root on Windows x64 / Node v26.5.0:

```powershell
npm run typecheck
npm run build
node --import tsx --test --test-concurrency=1 tests/analysis-dsl.test.ts

$env:PCB_BLOCK_BACKEND='cpu'
$env:PCB_BOARD_BACKEND='cpu'
$env:PCB_POST_PLACE_BACKEND='cpu'
node --import tsx --test --test-concurrency=1 tests/analysis-dsl.test.ts tests/pcb-board-hole.test.ts tests/pcb-constraint-region.test.ts tests/pcb-local-layout.test.ts tests/pcb-signal-path.test.ts tests/pcb-placement-validation.test.ts tests/pcb-native-addon.test.ts tests/pcb-analysis-runtime.test.ts tests/public-api.test.ts

npm run test:package
node debugging/analysis-dsl/evidence.mjs
git diff --check
```

The focused test also invokes `tsc --noEmit --strict --target ES2022 --module
NodeNext --moduleResolution NodeNext --allowImportingTsExtensions` on
`tests/analysis-dsl/examples.ts`, without skipLibCheck. This checks the standalone
installed declarations and negative compile-time vocabulary. Eight complete
USB, CAN, MCU/DC-DC/display, RF amplifier, DDR, external EMI, heatsink/thermal and
antenna examples are structurally exercised; synthetic test results are clearly
labelled and are not physical benchmarks.
Source-location checks also cover a user filename matching the worker basename
and a syntax error at the end of user code, without attributing it to the wrapper.

Detailed logs and review artifacts live under ignored
[`debugging/analysis-dsl/`](../../../../debugging/analysis-dsl/):

- `typecheck.log`, `build.log`, `dsl-final-tests.log`, `tests.log`, `package-final.log`;
- `api-coverage.json`: complete generated setter signatures;
- `prepare-line.js/.json`: successful structure with explicit physical limitations;
- `missing-solver.js/.json`: real production executor error, retained fail/unknown checks;
- `report-plot.js/.json`: serializable supplied table/check/log, no rendered artifact;
- `evidence.mjs/.log`: reproduction of those public-runner results;
- `changes.patch`: tracked edits plus new task files, excluding generated outputs.

Native sources, addon exports, F32 placement algorithms, V2/exporters and saved
boards are unchanged. This TS/documentation stage does not rebuild Rust or rerun
the full copper/mesh corpus. Linux/macOS execution, remote CI and the complete
`npm test` suite were not run. No publication or DC implementation in this stage.

Review correction M1 (2026-10-22): wrapped `equals` now obeys the same unit gate
as range/target predicates. A production-runner regression proves mismatched
units remain unknown and execution continues, dimensionless comparisons still
pass/fail, and unavailable/range behavior is retained. Typecheck/build and the
updated focused suite passed (29/29); logs: `m1-typecheck.log`, `m1-build.log`,
`m1-tests.log` under the artifact directory above.
