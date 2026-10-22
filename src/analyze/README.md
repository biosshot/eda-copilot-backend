# Analysis execution boundary

Internal infrastructure for structural RoutingBoardV2 import, copper geometry and conduction mesh preparation.
There is no separate `/analyze` package export. The root exports the scenario DSL
runner described below; native preparations remain internal.

## Analysis scenarios

`runAnalysisDsl({code, board?, profiles?, models?}, options)` runs ordinary JavaScript
with `await` and the globals documented by `analysisDslDocumentation()` (the
installed `dist/analysis-spec-doc.d.ts`). It does not transpile TypeScript.
`analysis()` is boardless; only `analysis({board})` opts into the snapshot. Catalogs
are explicit host data with no built-in project models or environmental defaults.
Builders snapshot sections, replace repeated sections and call the small internal
`executor.ts` boundary only after structural preparation. All production analysis
executors are currently unavailable; import/geometry/conduction are not user solvers.

Each scenario owns one disposable workerpool process using the existing process
fork options. A parent monotonic deadline includes startup and stops loops before
or after `await`; abort kills only that scenario, with exit awaited before return.
There is no additional compute queue/framework. `node:vm` restricts convenience
globals/code generation but is **not a security boundary for hostile source**.
No process, require, filesystem, network or timers are supplied as DSL globals.
Callbacks stay in JS; only finite JSON tasks reach the executor boundary.

Default limits: 120000 ms, 1000 report entries (across contexts), 4 MiB serialized
output. Options accept positive finite `timeoutMs`, `maxEntries` 1–100000 and
`maxOutputBytes` 4096–64 MiB; 2048 bytes are reserved for a bounded error envelope.
Context headers count against bytes. `filename` is at most 512 UTF-8 bytes with
no control characters. Diagnostics/messages may be explicitly truncated. These
are output/lifecycle limits, not a hard RSS or hostile-code containment guarantee.
Results are `{status:'complete'|'failed', reports, error?}`; errors retain source
location and prior reports. Invalid host requests reject with `INVALID_REQUEST`.
Progress sends append-only report entries; it does not recopy accumulated plots
on every log/check. Context/report payloads remain bounded across the scenario.
Checks never throw for a failed/unknown condition. Report output-limit errors and
program errors are distinct. Plot stores a typed description/data, never a fake file.

See [coverage, tests and limits](../../docs/experimental/analyze/analysis-dsl-verification.md).
`createAnalysisApi` and `executeScenario` are internal: tests explicitly inject a
test executor; the public runner has no executor/module-path override. Physical
result processing/timing and search algorithms are not implemented. Explicit-value
sweep runs its callback on every candidate and records individual failures.

## Internal native preparation

```text
parent TypeScript → bounded FIFO → reusable workerpool process → shared .node
                                                               → pcb-analyze
                  ← unchanged AnalysisImportReport ←───────────────┘
```

Use `runAnalysisProblem({ kind: 'import-board', boardJson }, options)` from
`run-analysis.ts`. The result type comes from `native/analyze/index.d.ts`.
`runAnalysisProblem({ kind: 'prepare-geometry', boardJson, geometry? }, options)`
uses the same queue, deadlines and lifecycle. Its inferred result is an
`AnalysisGeometryReport`; the import operation still returns `AnalysisImportReport`.
The geometry report has one `regions` collection. Contacts, islands and short
findings reference those region IDs. Drill polygons enclose the physical bore;
local `drill_topology_uncertain` / `drill_topology_unresolved` diagnostics flag
ambiguous cuts without retaining additional geometries or a second graph.
Separate islands are not a confirmed open; consult geometry diagnostics.
The optional geometry settings are validated and copied on admission. See
[geometry policy and evidence](../../docs/experimental/analyze/copper-geometry-verification.md).
The built `dist/analysis-runtime.js` is an internal package smoke-test entry.
`runAnalysisProblem({ kind: 'prepare-conduction', boardJson, geometry?, mesh?, profile? }, options)`
uses the same admission/deadline/abort/shutdown path and returns `AnalysisConductionReport`.
Its nested profile/options are validated and snapshotted at admission. There is no
new queue or session. Detailed nodes/elements live once in this report; geometry
snapshots are not embedded. Native compact Rust JSON byte count and stage timers
are reported; fixture runs separately measure actual Node JSON bytes and wall time.
See [conduction preparation and limits](../../docs/experimental/analyze/conduction-mesh-verification.md).
Only the serializable task (JSON string, optional geometry/mesh settings and physical profile)
and job ID cross IPC; callbacks and AbortSignal stay in
the parent. Importing the runtime does not load the addon. The ready process
loads `native/node-addon/index.cjs`, emits `started`, and calls its synchronous
selected import/preparation binding. Internal Rust Board data is neither returned nor
retained between calls. Analyze stays F64 and does not initialize GPU.

## Limits and lifecycle

The pool is lazy. Each slot owns one reusable, single-process workerpool pool;
this keeps targeted cancellation and exit confirmation on workerpool's public
API. One parent FIFO owns all waiting tasks; active/terminating slots do not
count against its waiting limit. No placement-pool refactoring is involved.

| Environment | Default | Accepted integer range |
| --- | ---: | --- |
| `PCB_ANALYZE_WORKERS` | 1 | 1 .. Node `availableParallelism()` |
| `PCB_ANALYZE_WORKER_QUEUE_SIZE` | 4 | 0 .. 10000 waiting jobs |
| `PCB_ANALYZE_WORKER_TIMEOUT_MS` | 1800000 | 1 .. 2147483647 ms |
| `PCB_ANALYZE_WORKER_TERMINATE_TIMEOUT_MS` | 1000 | 1 .. 60000 ms |

Missing, empty, noninteger or out-of-range configuration uses the default.
Configuration is read when a lazy generation is created. `timeoutMs` per task
accepts any positive finite number. Its monotonic deadline starts on admission,
includes queue/startup time, and is never reset on dispatch. Long deadlines use
bounded Node timer intervals. Overdue results cannot resolve successfully.

Execution failures reject with `AnalysisExecutionError.code`: `ABORTED`,
`TIMEOUT`, `QUEUE_FULL`, `WORKER_FAILED`, `DISPOSED`, or `INVALID_REQUEST`.
Invalid JSON/boards instead resolve the original `imported: false` report.
`imported: true` certifies structural import only. Missing/limitation diagnostics
do not turn it into an execution failure or prove readiness for physics.

An already-aborted signal creates no process/task. Queued cancellation or timeout
removes that job immediately. Active cancellation/timeout rejects once and stops
only its occupied process via `terminate(true, ...)`. Workerpool requests clean
exit, kills a blocked synchronous process after the configured grace interval,
and confirms exit. The outer shutdown wait is bounded by grace + 5000 ms; a slot
is replaced only after exit confirmation. No automatic job retry or late success.
On an OS-level shutdown failure the slot is retained and dispose rejects, rather
than silently creating additional processes.

Progress is `queued`, worker-observed `started`, then exactly one of `completed`,
`failed`, `cancelled`. `completed` includes `imported: false`; timeout/crash/dispose
use `failed`. Pre-admission rejections have no progress events. Observer throws
and rejected promises are contained; terminal events end delivery. Timers and
signal listeners are removed on settlement.

`terminateAnalysisPool()` is idempotent during shutdown: reject active/waiting
jobs as `DISPOSED`, reject new work, and await process cleanup. A successful
shutdown permits a new lazy generation. `disposeBackend()` attempts both
placement and analysis shutdown even when either fails.

Queue/process limits are not native RAM or RSS limits. No V8 heap limit is
claimed as a Rust memory limit. Global placement/analysis CPU scheduling, hard
RSS control, caching, sessions, meshing and solvers remain outside
this stage.

## Checks

Build TypeScript workers and the existing shared addon before integration tests:

```sh
npm run native:build
npm run typecheck
npm run build
node --import tsx --test --test-concurrency=1 tests/pcb-analysis-runtime.test.ts
node --test scripts/test-pcb-analyze.test.mjs
npm run test:pcb-analyze -- --all --name import
npm run test:package
```

Runtime tests include the real addon, full eight-board report parity, PID/event
loop checks, and separate file-gated synchronous test workers for cancellation,
timeouts, queue limits, crashes, two-process isolation and lifecycle races. Native
integration tests fail with rebuild guidance when the addon is missing/stale.
The installed-package smoke uses compiled JS without repository source or tsx.
See [fixture runs](../../tests/pcb-analyze/README.md) and the
[verification record](../../docs/experimental/analyze/analysis-runtime-verification.md).
