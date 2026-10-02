# PortableScope incomplete capture review — 2026-10-02

Reviewed existing `debugging/pcb-layout/runs/PortableScope/2026-10-02T07-57-08-534Z`.
No fixture/capture/replay was launched. There are 194 native requests; the board
request `00194-board-c3c65eb88b` remains `started`, without a solution. Its SHA256
is `c3c65eb88b9fa1e304f52be0f63a38aa7d3fad5083f4a5c4ad330920b7f511fe`.
The last recorded board phase is beam, 369.442 s, six CPU workers, hard count 9.
No final local-improve/repair/board/refiner/router result exists in this capture.
Process 20360 was absent at inspection. The capture does not establish why the
process ended; no fatal error is recorded in its log. Hard=9 is intermediate,
not a final placement verdict. The old capture lacks launch-time addon provenance;
do not retrospectively assign the current addon hash to it.

## Why Board Packager chose CPU

The log explicitly says `requested=auto`, actual `cpu`, reason
`outside measured board GPU workload/thread threshold`. Capture inherits the
parent environment and only enables diagnostic flags; it never sets a backend.
This was policy selection before GPU execution, not a GPU error or capture
switching backend. No GPU fallback lines were found anywhere in the run.

The saved board has 28 primitives, 23 unlocked, 246 components, 1166 relations,
search width 32. `solver/cubecl.rs::auto_profitable` requires at least 34 primitives,
24 unlocked, 154 components, 535 relations and width 32. The first two conditions
fail. Six host workers are not the deciding failure: `solver.rs` checks auto
profitability with `threads.min(4)` and caps an accepted GPU call at four workers.
Explicit `PCB_BOARD_BACKEND=cubecl` requests the GPU path, with normal device/input
guards and complete CPU recovery. This review does not measure its speed on this
board and does not silently lower the measured crossover threshold.

## Optimization priorities supported by this partial run

1. **Board Packager**: 369.442 s beam already exceeds individual block jobs.
   The saved exact problem is available for one future GPU replay and geometry
   comparison. Evaluate a workload criterion using components/connectivity and
   candidate cost, not just primitive count: this board has fewer primitives
   but substantially more components/relations than the original Telemetry floor.
   Detailed inner board timings were not enabled, so this capture cannot prove
   which board score contribution dominates. Keep local-improve and repair in
   any full-cycle timing and inspect the nine intermediate hard violations.
2. **Auto block selection for composites**: grouped FPGA has only five primitives
   but 19 components and 384 connection points. It stayed CPU because block auto
   requires `primitives.len() >= 6`, even though the substantial-work flag is true.
   It took 42.775 s (beam 28.467 s, singles 14.306 s); full/bounded scoring consumed
   29.238 accumulated seconds, including 24.676 s direct pad crossing. Grouped
   MCU similarly has five primitives/11 components/120 points: 16.905 s initial
   CPU work and 6.580 s pair continuation. New GPU support for composites does
   not automatically change the existing profitability gate. Review this gate
   using actual candidate/score work; tiny CPU blocks should not all pay GPU startup.
3. **MCU cold GPU work**: released MCU initial job takes 82.751 s, almost all beam;
   343 batch spans account for 76.702 s and frontier 5.455 s. The first GPU beam
   reports only 0.487 ms cumulative queue mutex wait, so contention does not explain
   that initial delay. Cold shader compilation is a strong hypothesis, mixed with
   kernel execution in completion waits; compilation is not separately timed and
   cannot be asserted to consume the whole 76.7 s. Reuse pipelines and instrument
   compilation before rewriting scoring arithmetic.
4. **DDR GPU execution/scheduling**: grouped initial 40.591 s; released initial
   83.473 s. The batch of both hypotheses takes 83.495 s (do not sum job durations).
   Released GPU work: 2653 score batches, 7,226,013 score candidates; 496 frontier
   batches and 15,826,368 frontier candidates. Score batch spans 69.788 s, completion
   waits 51.135 s, frontier 5.416 s, shortlist geometric route checks 5.446 s.
   Shared runtime cumulative mutex wait reaches 59.329 s across concurrent jobs;
   it overlaps work and is not additive to wall time. Improve scoring passes,
   invariant reuse and job scheduling. `gpu_readback` includes GPU execution/wait,
   not just buffer transfer. Historical `block_micro_route_penalty` is geometric
   in these blocks; it does not prove Micro-A* dominates.

The 88 completed local post-place calls total only 2.222 accumulated seconds.
60 logged block calls use CPU and 45 use CubeCL; many CPU jobs are trivial
single primitives/passive islands. Full pipeline routing and final refinement
were not reached. Total board/pipeline duration and final quality remain unknown.

## Capture diagnostic fixes and validation

- Freeze addon hash, source hashes, revision/dirty state and requested backends in
  `run-manifest.json` before child launch; no dependence on successful completion.
- Record requested backend/thread settings in each native metadata file before
  entering Rust. Distinguish request from actual execution.
- Keep native backend decisions/fallback reasons and board detail in summaries.
- Enable `PCB_BOARD_PACKER_DETAIL` in capture, alongside existing profiles.
  Detailed timings introduce diagnostic overhead and represent nested worker work.
- Preserve inherited backend policy. No solver threshold or algorithm change here.

Validation: TypeScript typecheck, three focused capture tests (request preservation,
actual-decision/detail parsing, opt-in exact-input/started metadata) and script
syntax check passed. Existing placement workloads were not rerun.


## Later completed capture: 2026-10-02T08-30-42-388Z

This supersedes the incomplete run above for timing analysis, without modifying its evidence.
The existing capture completed in **824.728 s**, auto-place **820.978 s**,
with addon SHA256 `349f526507fb80185b5c9bae7ce59d738bd736929e1752b976557224090ed6ac`.
Both board calls actually selected CubeCL under auto (the user temporarily made
`auto_profitable` return true). This is before the relaxed-threshold build.

- Ordinary board call: beam 139.273 s, local improve 17.518 s, repair 0.389 s;
  native total **157.180 s**.
- Aligned board call: beam 148.679 s, local improve 26.229 s, repair 0.260 s;
  native total **175.168 s**. The two sequential calls total **332.348 s**,
  approximately 40.3% of complete capture time; their beam stages total 287.952 s.
- Board GPU operation spans total about 92 s and 97 s. Readback includes execution
  and synchronization; it is not a pure PCIe transfer measurement.
- CPU route evaluation accumulates 253.808 s and 298.087 s across workers.
  Engine lock waiting accumulates 201.935 s and 196.030 s. These overlap parallel
  work and must not be added to wall time. CPU routing and serialized GPU access
  remain important alongside GPU kernel execution; lower admission floors do not
  optimize these operations.
- Block initial batch wall times: DDR **82.447 s**, MCU **51.689 s**, FPGA
  **48.393 s**. Each batch is counted once, not once per hypothesis.
- Final native refiner: **27.762 s** (capture wrapper 27.912 s), CPU, six workers.
  Its existing worker/readiness rule excludes six workers independently of the
  workload floors. Lowering size floors will not change this specific choice.
- First board capture begins 297.908 s after capture launch. There are also about
  87.0 s between the end of the second captured board call and final refiner
  capture, and about 79.2 s after the refiner until capture completion. The saved
  coarse stage timings do not attribute these intervals precisely; do not call
  them GPU execution or assume the native calls explain all 824.728 s.

Quality: `placementOk: false`, with **11 fatal overlap errors**, despite exit 0.
The log already reports hard=9 at both board beam checkpoints. Backend admission
changes do not repair this geometry. No full placement rerun was performed.

## Requested admission-only change

Divide minimum workload thresholds by eight, round down and clamp to one:

| Consumer | Previous minima | New minima |
| --- | --- | --- |
| Board primitives / components / search width / relations / movable | 34 / 154 / 32 / 535 / 24 | 4 / 19 / 4 / 66 / 3 |
| Block cold primitives OR connection points | 10 / 240 | 1 / 30 |
| Block auto / explicit primitive floor | 6 / 3 | 1 / 1 |
| Refiner components / pads / net edges / timeout ms | 154 / 660 / 235 / 2000 | 19 / 82 / 29 / 250 |

At a one-primitive floor the block cold-size condition admits every nonempty
block; the retained point-count/readiness alternatives are consequently redundant.
This is the requested numeric change, not a restructuring of backend selection.
Keep worker caps/readiness, positive iteration checks, input/device/memory guards
and original-input CPU recovery. Reduced floors are a heuristic, not evidence of
speedup for tiny jobs. The completed capture above predates these changes.


Validation of the relaxed thresholds: 81 Rust tests passed (4 ignored), release
addon built, TypeScript typecheck passed, five focused board fallback/capture
checks passed. An ephemeral synthetic board at the new 4/19/4/66/3 minima selected
actual GPU, did not fall back, and exactly matched its single CPU pass.
One replay each of saved local FPGA refiner `00168` and keys block `00002` matched
their saved outputs exactly. Refiner performed 2 GPU batches / 198 candidates;
the keys input selected CubeCL but needed no GPU score batches. Replay wrapper
times were 534.971 ms and 391.354 ms, versus saved CPU capture times 67.259 ms and
3.907 ms. These cold-start correctness checks ran alongside other focused checks,
so they are not isolated performance comparisons. They demonstrate that relaxed
admission is not itself a speed guarantee, particularly for tiny jobs. No full
PortableScope capture or repeated median benchmarking was started.
