# Analysis execution boundary

Internal infrastructure for RoutingBoardV2 import, copper geometry, conduction mesh and steady DC.
There is no separate `/analyze` package export. The root exports the scenario DSL
runner described below; native preparations remain internal.

## Analysis scenarios

`runAnalysisDsl({code, board?, profiles?, models?}, options)` runs ordinary JavaScript
with `await` and the globals documented by `analysisDslDocumentation()` (the
installed `dist/analysis-spec-doc.d.ts`). It does not transpile TypeScript.
`analysis()` is boardless; only `analysis({board})` opts into the snapshot. Catalogs
are explicit host data with no built-in project models or environmental defaults.
Builders snapshot sections, replace repeated sections and call the small internal
`executor.ts` boundary only after structural preparation. Constant linear PCB DC
and explicit circuit DC/Z(f)/transient are implemented. `geometry()` validates
connectivity/clearance/edge-clearance with source witnesses and explicit coverage.
Import/geometry/conduction preparations remain separate internal operations.

Each scenario owns one disposable workerpool process using the existing process
fork options. A parent monotonic deadline includes startup and stops loops before
or after `await`; abort kills only that scenario, with exit awaited before return.
There is no additional compute queue/framework. `node:vm` restricts convenience
globals/code generation but is **not a security boundary for hostile source**.
No process, require, filesystem, network or timers are supplied as DSL globals.
Callbacks stay in JS; only finite JSON tasks reach the executor boundary.

Default limits: 120000 ms, 1000 report entries (across contexts), 4 MiB serialized
output. Options accept positive finite `timeoutMs`, `maxEntries` 1–100000 and
`maxOutputBytes` 4096–128 MiB; 2048 bytes are reserved for a bounded error envelope.
Context headers count against bytes. `filename` is at most 512 UTF-8 bytes with
no control characters. Diagnostics/messages may be explicitly truncated. These
are output/lifecycle limits, not a hard RSS or hostile-code containment guarantee.
Results are `{status:'complete'|'failed', reports, error?}`; errors retain source
location and prior reports. Invalid host requests reject with `INVALID_REQUEST`.
Progress sends append-only report entries; it does not recopy accumulated plots
on every log/check. Context/report payloads remain bounded across the scenario.
Checks never throw for a failed/unknown condition. Report output-limit errors and
program errors are distinct. Plot stores a typed description/data, never a fake file.

See [coverage, tests and limits](../../docs/analyze/archive/design/analysis-dsl-verification.md).
`createAnalysisApi` and `executeScenario` are internal: tests explicitly inject a
test executor; the public runner has no executor/module-path override. Physical
timing and search algorithms are not implemented. Explicit-value
sweep runs its callback on every candidate and records individual failures.

## Steady DC

Production `dc().run()` automatically validates/imports/prepares supported copper,
assembles CPU/F64 triangle sheet conductance and finite barrel segments, then uses
`faer` sparse QR. `prepare()` is a structural diagnostic, with physicalReady
false until native validation; executorAvailable reports installed analyzer support.
Standalone rectangular conductors use the exact uniform-strip resistance formula.
Ideal constant voltage/current sources and constant current/resistive loads are
supported. In board mode constant power, dynamic operating points, resource models and
ideal inductors are explicitly rejected. Capacitors remain DC opens.

Pad electrodes cover existing finite terminal triangles on the first physical
surface; an unmeshed first surface cannot silently fall back to the bottom.
Barrel ring electrodes remain connected only by finite segment conductance.
No ideal net/GND merging or hidden gmin. Current is positive delivered by a source,
absorbed by a load; pad selectors return explicitly assigned external injection.
Text currents use stable `source-1`/`load-1`/`component:<designator>` element IDs.
Arbitrary net current and voltages across independent gauges are unavailable.

`stackup` accepts scenario-only `platingMaterial` and `diameterConvention` alongside
existing plating thickness. Materials need explicit conductivity/state. A complete
referenceC/temperatureCoefficientPerK/temperatureC law applies linear resistivity;
its unspecified empirical range remains conditional. Missing barrel physics is
never selected from a copper name. V2 remains unchanged.

Strict CAD fill freshness remains default. `.inputPolicy({cadFill:'snapshot'})`
permits only available polygons with unknown freshness, records original freshness
and board hash, and makes result/check/map conditional. It does not waive missing
copper/drills, stale fills, topology, unbuilt mesh, contacts or unspecified hole spans.

`.mesh({maxAreaMm2,...})` controls existing discretization. DC uses bounded boundary
subdivision `max_edge_mm=sqrt(maxAreaMm2/4)` while keeping constraint edges fixed
during Spade refinement. General diagnostic mesh defaults remain unchanged.
Default DC caps: 100000 mesh nodes, 200000 triangles, 100000 reduced unknowns,
1024 MiB conservative planning, 120 seconds. Defaults/explicit limits and actual
counts/timings are recorded. Planning covers preparation, sparse assembly and symbolic QR factor/workspace sizes
and maps before large allocations; RSS is measured at native call boundaries,
not as a hard allocator ceiling or peak. Parent deadline/abort covers all stages.

One scenario owns at most one native preparation/factorization; changed context,
geometry/profile/contact/element matrix or limits invalidates it. New RHS reuses
the factorization. Independent internal DC problems clear their session after the
call. Native solve runs in the disposable DSL process, with no nested child to
orphan on cancellation. There is no global DC cache or competing public runner.

`result.solution` retains computed-domain mesh, real fields, geometry/bores, quality,
physical profile, requests and provenance. Omitted-domain summaries retain original
counts/readiness; omitted arrays do not mean absent copper. Repeated geometry uses
a solution-ID reference in the same report. Maps carry this identity and actual
values. J is A/mm², triangle loss W; barrel loss is separate. Real scalar transforms
and min/max reducers preserve units/validity; partial maps stay conditional and
potentials from independent references cannot be reduced together.

See [DC verification](../../docs/analyze/evidence/dc-analysis-verification.md):
all eight selected real PDNs solve at two loads/two meshes with convergence below 2%,
after re-exporting three fixtures and preserving small F64 refinement corrections. Coupon physics and real-board eligibility are separate acceptance criteria.

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
[geometry policy and evidence](../../docs/analyze/evidence/copper-geometry-verification.md).
The built `dist/analysis-runtime.js` is an internal package smoke-test entry.
`runAnalysisProblem({ kind: 'prepare-conduction', boardJson, geometry?, mesh?, profile? }, options)`
uses the same admission/deadline/abort/shutdown path and returns `AnalysisConductionReport`.
Its nested profile/options are validated and snapshotted at admission. There is no
new queue or session. Detailed nodes/elements live once in this report; geometry
snapshots are not embedded. Native compact Rust JSON byte count and stage timers
are reported; fixture runs separately measure actual Node JSON bytes and wall time.
See [conduction preparation and limits](../../docs/analyze/evidence/conduction-mesh-verification.md).
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
[verification record](../../docs/analyze/evidence/analysis-runtime-verification.md).

## DC observations and saved examples

`a.port` only describes selectors. Board DC activates finite-area pad electrodes
from the current builder's sources, loads, resolved IC assignments and explicit
resistive elements. An unused ordinary/aggregated/unready port does not change
assembly; standalone conductor ignores board-port descriptors.

`dc.voltage(port)` uses active electrode potential where present; otherwise each
single-pad side integrates the immutable piecewise-linear FEM field over all
prepared terminal triangles on the first physical surface (area-weighted mean,
bores excluded). It adds no `join_all`, conductivity, domain or solve. Incomplete,
uncomputed or incompatible-reference coverage and aggregate observations return
unavailable with a reason; there is no first-node or other-layer fallback. A port
may be declared after solve. Current observations still require explicit elements.

Native preparation/factorization reuse is separate from report geometry reuse.
A compact mesh reference is emitted only if its target covers all computed domains
in the same context/board/profile/mesh; otherwise the current result retains its
mesh and becomes the next eligible geometry owner. Earlier results remain frozen.

Real-board `dc.js` files are directly authored DSL. Thin named launchers preserve
exact text/filename and hashes, run the production runner, save evidence and invoke
`tests/helpers/pcb-analysis-dc-audit.ts`. The audit and its negative tests are saved
in the repository and do not invoke FEM. See the fixture README and DC verification.

## Explicit circuit networks (P6a)

The existing dc/pi builders now accept explicitly authored R/C/L circuits for DC,
complex driving-point Z(f), and adaptive transient/state continuation. CPU/F64
Rust sparse MNA uses the same shared addon and disposable process. Board FEM DC
remains independent. See [semantics](../../docs/analyze/reference/p5-p6-electrical/electrical-network-semantics.md)
and [verification](../../docs/analyze/evidence/electrical-network-verification.md)
for signs, p/n model mapping, numerics, state/resource limits and direct RLC examples.
No PCB frequency extraction or spatial circuit maps are produced.

## Frequency multiports (P6b)

`a.model` additionally describes immutable network Resources. Factory/direct/catalog
inputs share early validation; `a.network` creates mapped instances and `a.connect`
pairs their ports. `networkParameters` dispatches through the existing process and
shared addon to Rust, returning S/Z/Y complex DataValues and reusable result.network.
See [semantics](../../docs/analyze/reference/p5-p6-electrical/frequency-network-semantics.md) and
[verification](../../docs/analyze/evidence/frequency-networks-verification.md).
No PCB frequency extraction or imported-network transient is implemented.


P7a cross sections: explicit `a.line()` geometries use dielectric P1 FEM (not the
DC copper mesh), C/C0/external L and lossless modal distributed propagation in
`native/analyze/src/extraction/`. `z.network` reuses P6b, the existing process and
single addon; no GPU or additional dependencies. See
[semantics](../../docs/analyze/reference/p7-extraction/line-cross-sections-semantics.md).
Explicit PEC/lossless inputs retain their result forms. Optional `conductors`
provides fixed-temperature smooth signal conductivity and complete reference
thickness/materials; explicit 0..0.05 dielectric tangents use regional energy
participation. Native skin-depth, weak-loss and independent 2% loss convergence
guards reject ineligible cases. Mask/roughness/dispersion/temperature/other targets
reject. Field reports retain mesh/phi/E, quality, independent mesh/domain
convergence and exact-frequency matrices. Large reports require an explicit
bounded report budget. Real-board examples are conditional geometry/stackup
hypotheses, not measured/extracted complete PCB channels.
Lossy RLGC additionally retains per-frequency full matrices, all physical
surface/region contributions, unit-current surface fields and complex modal
alpha/beta/Zc/phase delay. Stable full 2/4-port S and a longitudinal dissipation
balance feed existing P6b. See [loss semantics](../../docs/analyze/reference/p7-extraction/line-losses-semantics.md)
and [verification and limitations](../../docs/analyze/evidence/line-losses-verification.md).


## Geometry validation

`analysis({board}).geometry().rules([{kind:'connectivity'},
{kind:'clearance',minMm:0.2},{kind:'edge-clearance',minMm:0.3}]).run()`
checks the captured copper without refill. Scope selectors select complete regions
against relevant neighbours, including copper outside the scope. Distance witnesses
and source polygons are retained in the solution; `measurements` is a table in mm.

Connectivity reuses conservative shorts/contacts. Islands alone do not establish
an open; terminal-group intent is unavailable. Same known net clearance is exempt;
unknown nets remain conservative uncertain observations. Board edges include real
cutouts and exclude drill holes. Missing/stale/uncertain input keeps coverage
partial. Whenever requested coverage is incomplete, `violations.count()` is
unavailable even when confirmed items exist; inspect `violations.items` and the
explicitly labeled confirmed-report count in measurements. Generic checks keep
running with unknown/fail results. Channel length/skew/reference rules reject.

Native defaults: 120 s, 512 MiB allocation planning, 1 million broad-phase
candidates, 5 million segment checks, 100000 measurements, 2 million report
vertices and 96 MiB report JSON. The existing worker deadline is the hard execution
boundary; allocation estimates are not an OS RSS guarantee. Scope/rule processing
and report construction are bounded. Base preparation retains its existing limits.
See [semantics](../../docs/analyze/reference/p4-geometry/geometry-validation-semantics.md)
and [verification](../../docs/analyze/evidence/geometry-validation-verification.md).
Run all eight direct scenarios with `npm run test:pcb-analyze -- --all --name validation`.
