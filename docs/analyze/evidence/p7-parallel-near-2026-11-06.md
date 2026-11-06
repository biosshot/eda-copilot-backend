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
`debugging/p7-parallel-near-native-build.log`). The bounded four-worker STM32
attempt remains pending. The planned run retains the same board/channel, all source geometry, 10 kHz,
maxIterations=4, native 590-second deadline, external 600-second cap, native
12 GiB/tree 14 GiB budgets. Trace/capture/scalar cache and other experimental
switches must be off to exercise parallel near work. No complete extraction or
<=600-second result is claimed before that evidence exists.
