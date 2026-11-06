# Bounded parallel magnetic near kernels

The production streamed magnetic action has an opt-in CPU path selected by
`P7_MAGNETIC_NEAR_THREADS=2..32`; the default remains one worker. It preserves
every original tetrahedron, current coordinate, return path and physical gate.

## Execution and resources

The adaptive traversal buffers at most 128 near pairs in original order. A
dedicated Rayon pool evaluates pure geometric kernels concurrently. Each task
has a bounded local geometry/cubature cache; immutable tetrahedra are shared by
Arc. Results are replayed through the original outward interval additions in
original traversal order, including original density multiplication order.
Far interactions, covers, goals and physical certificates retain their logic.
All workers join before an ordered error is returned; no detached work remains.

Before creating the pool, admission reserves per worker ten 192 KiB cache/scratch
slots and a 2 MiB thread stack, plus 8 KiB window storage. It includes these bytes
in the streamed/hierarchy budget. A persistent pool is reused, while local
caches are recreated for each window. Invalid worker counts and oversized
windows are refused. The standard one-worker path does not buffer near work.

`P7_MAGNETIC_TRACE=1`, slow-pair capture, or an enabled scalar pair cache cause an
explicit serial fallback so their existing accounting remains valid. The
`P7_MAGNETIC_PARALLEL_PLAN` line reports requested/actual workers and extra bytes.
Numeric response-cache identity is unchanged because summation and values are
preserved; timing fields naturally differ.

## Focused evidence

Final release controls were rerun under one external 120-second supervisor,
2048 MiB tree cap, 1 GiB host reserve. All fourteen tests passed in 6.375 seconds
external wall. Sampled peak tree private memory was 14,286,848 B, RSS 31,182,848 B
(20 ms sampled lower bounds). Logs:
`debugging/p7-parallel-near-final-controls/stdout.txt` and `supervision.json`.

- Pair-window kernels compare bitwise at 1/2/4 workers, with resource and
  deadline refusal controls.
- A signed two-column full action contains more than 128 near pairs and a far
  cell. Actions, bounds and all certificate fields except three timing fields
  match bitwise at 2/4 workers, covering full and trailing window replay.
- The existing signed-action/full-KVL enclosure control passes.
- All ten mixed section/R/L/C controls, including the enriched admission
  diagnostic, pass. Their specific physical refusals remain documented.

The captured benchmark rebuilds the sixteen saved slow-pair geometries and
repeats them eight times, producing 128 jobs with cold eight-slot caches and no
scalar memoization. It compares current sequential/parallel result bits, not old
saved order-10 values. The saved target/source orientation is retained, but each
record gets separate geometry indices: this is kernel replay, not the actual
board traversal or its cache history.

| Workers | Seconds | Relative to one worker |
|---:|---:|---:|
| 1 | 0.283469 | 1.00x |
| 2 | 0.151856 | 1.87x |
| 4 | 0.089053 | 3.18x |

The earlier agent executions lacked an external supervisor; the measurements
above use the subsequent supervised validation. This microbenchmark does not
establish full-board speedup or physical case-matrix acceptance.

## Full-path validation

Native addon build passed (`BUILD_EXIT=0`, release 5m29s; local log
`debugging/p7-parallel-near-native-build.log`). The four-worker STM32 run ended
with `LOCAL_DEADLINE: streamed tetra magnetic action`, external wall
593.672 s. The native deadline fired before the 600-second supervisor cap; this
was not an external kill or memory refusal. No channel result was completed.

The run used commit `e1d36bc`, RS485_B U5.7/U5.5 → J5.2/J5.3, 10 kHz,
maxIterations=4, native deadline 590 s, native 12 GiB/tree 14 GiB budgets and a
cold isolated cache. All 844 copper regions, 65 islands and 89 barrels remain;
the full space still has 65,206 potentials and 464,697 current coordinates
(154,899 copper tetrahedra). Trace/capture/scalar cache, cover cache, section
hints and near-bound omission were off; RHS chunks remained one column.

The native log confirms `nearWorkers=4 requestedNearWorkers=4`, with
16,261,120 extra planned bytes. Global `RAYON_NUM_THREADS=1` keeps other stages
comparable; the supervisor's `threads: "1"` field reports this global setting,
not the dedicated near pool. Sampled peak private memory was 1,871,863,808 B,
RSS 1,117,921,280 B (20 ms sampled lower bounds). Actual peak is not proven by
sampling.

Two observations of the same Node PID show CPU increasing from 429.34375 to
758.03125 s over approximately 171.399 wall seconds, about 1.92 cores on average
over that interval. This does not isolate the cause: serial traversal, work
imbalance and available host CPU may all matter. It is not a full-run speedup.
Trace is incompatible with the current parallel path, so no completed-pair or
completed-action count is available; do not claim first-action completion or
infer a board-level acceleration factor from two deadline exits.

Local evidence is under `debugging/p7-parallel-near-stm32-600s/`: `stdout.txt`,
`comparison.json`, `result.json`, `supervision.json`, `identity.json` and the two
`cpu-sample-*.json` observations. The native core identity is
`6cfa6fb540eb53ec49e5d7aceb23c262a9adaa582a6dfe72701ff8bb88708702`.

The <=600-second complete-extraction target remains unmet. No accuracy gate,
physical geometry or case-matrix acceptance changed. The opt-in implementation
is retained for controlled use; it is not enabled by default.

Before another heavy run, add parallel-compatible progress/timing measurements
and test persistent worker caches (currently recreated each window) with finer
job scheduling. Compare before enabling any policy. Structural work reduction
and a production shared-return route remain required; another uninstrumented
deadline run would not resolve the remaining uncertainty.
