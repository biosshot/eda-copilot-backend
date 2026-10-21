# Analysis execution boundary

Internal infrastructure for the existing structural RoutingBoardV2 importer.
There is no public `/analyze` package export or analysis DSL at this stage.

```text
parent TypeScript → bounded FIFO → reusable workerpool process → shared .node
                                                               → pcb-analyze
                  ← unchanged AnalysisImportReport ←───────────────┘
```

Use `runAnalysisProblem({ kind: 'import-board', boardJson }, options)` from
`run-analysis.ts`. The result type comes from `native/analyze/index.d.ts`.
The built `dist/analysis-runtime.js` is an internal package smoke-test entry.
Only the string task and job ID cross IPC; callbacks and AbortSignal stay in
the parent. Importing the runtime does not load the addon. The ready process
loads `native/node-addon/index.cjs`, emits `started`, and calls its synchronous
`importAnalysisBoard` binding. Internal Rust Board data is neither returned nor
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
RSS control, caching, sessions, geometry preparation and solvers remain outside
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
