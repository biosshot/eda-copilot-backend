# Repository workflow

This file applies to the entire `eda-copilot-backend` repository. Keep production placement independent of diagnostics: capture and profiling must be opt-in and must not change scores, candidate order, cache keys, or placement policy. Preserve user changes in fixtures and DSL files.

## Build and fast checks

Run commands from this repository root. After TypeScript edits, run `npm run typecheck`. After native edits, run `npm run native:build` and focused Rust and TypeScript tests. Do not run all PCB layout fixtures or a large board for a small change. Use exact captured native inputs for quick iteration, then a few representative blocks, then full boards when needed.

Before editing repository code, read `CHANGELOG.md`; before changing placement behavior, also read the relevant recent experiment reports. Update the `Unreleased` changelog section for each meaningful behavior, performance, workflow, or compatibility change; mention regressions and limitations plainly. Keep historical release entries intact. A focused experiment report contains the measurements and verdict; the changelog records what actually changed for users and developers.

## PCB layout speed and parallelism

Treat PCB layout wall time as a required quality measure alongside placement validity and electrical quality. Before a performance-sensitive change, save the current timing for the same exact captured block or board problem. Afterward, compare cold runs in fresh processes with the same native build configuration, input, machine, thread settings, cache setting, and profiling state; record both binary hashes. Use repeated runs and compare medians for short solves. Separate fixture preparation and footprint loading from block search, board packing, and refinement. Record candidate and evaluation counts when available. Treat a reproducible slowdown of roughly 15% or more on a representative case as a regression: investigate, fix or revert it before declaring the change successful, and document any explicitly accepted tradeoff. After promising local results, check representative full-board wall time before declaring a PCB optimization complete. Do not use a warm native replay as evidence for end-to-end speed.

Parallelize independent PCB work where it reduces wall time, but first inspect the existing pools and thread budgets. Reuse `src/pcb-layout/run-pcb-layout-pool.ts` for independent board runs and `src/pcb-layout/pcb-auto-place-v2/tree-subtree-pool.ts` for subtree work; use the existing native batch/board/refinement parallelism for native work. Do not create an ad hoc pool or spawn one process per candidate. Coordinate `PCB_LAYOUT_WORKERS`, `PCB_LAYOUT_SUBTREE_WORKERS`, `PCB_BOARD_PACKER_THREADS`, and `PCB_POST_PLACE_THREADS` so nested levels share the available CPU budget rather than multiplying it. Keep queues bounded, preserve cancellation and timeout behavior, and do not load the native addon into multiple worker threads in one process. Measure serial and bounded-parallel modes on the same inputs: more threads are useful only if elapsed time improves without destabilizing placement or exhausting memory.

## Debugging output

All generated test and experiment files belong under `debugging/`, which is gitignored. Use domain subdirectories:

- `debugging/pcb-layout/` for PCB fixture results, captured native problems and replays.
- `debugging/circuit-layout/gallery/` and `debugging/circuit-layout/cache/` for circuit layout.
- `debugging/circuit-layout/patterns/` for schematic pattern comparisons.

Keep each run in its own named or timestamped directory. Use JSON for exact inputs, outputs and metrics; Markdown for human-readable conclusions; SVG or HTML for visual comparisons. Do not put generated outputs in `docs/experiments/`. Commit only selected comparison artifacts there when the experiment needs a durable record. Avoid overwriting a prior run that is being used as a baseline.

## PCB layout capture and replay

List examples with `npm run test:pcb-layout -- --list`. An ordinary fixture run is `npm run test:pcb-layout -- PortableScope`; its generated files are under `debugging/pcb-layout/PortableScope/`. This can be slow, especially PortableScope.

Capture a full fixture once with `npm run debug:pcb-layout -- capture PortableScope`. This enables native profiling in that child process and writes `debugging/pcb-layout/runs/PortableScope/<timestamp>/`. Inspect `summary.md`, `summary.json`, `stages.json`, `run.log`, `placement/placement.svg`, `placement/board.assemble.json`, and `native/{block,board}/process-*/<capture>/`. A capture contains exact encoded `problem.json`, `solution.json` and `meta.json` at the current TypeScript-to-Rust boundary, including staged block hypotheses. The capture is diagnostic; a fixture can exit successfully while its placement report says `placementOk: false`, so check both.

Replay one captured native problem with `npm run debug:pcb-layout -- replay <path-to-capture-directory> [repeats]`. The result is saved under `debugging/pcb-layout/replays/`. Inputs are written before Rust starts, so a crashed or stalled solve can be replayed even when its `solution.json` is absent; in that case `exactBaselineMatch` is null. Compare ranks, geometry and the rendered placement before accepting a change. Replay uses the current native binary and bypasses circuit parsing, footprint resolution and the rest of board placement. For a cold timing measurement use a fresh process; repeated solves in one replay process can reflect warm caches. Capture timing is wall time for the whole native batch; do not sum it across its hypotheses. Rust detail and candidate counts are in `run.log` when profiling is enabled. Parallel thread times are not the board wall time.

To capture another PCB entry point programmatically, set `PCB_LAYOUT_DEBUG_DIR` to a unique absolute output directory before calling `runPcbLayout`. Without this environment variable, native input capture and stage timing are disabled. `PCB_BLOCK_SOLVER_PROFILE=1`, `PCB_BLOCK_SOLVER_DETAIL=1`, and `PCB_BOARD_PACKER_PROFILE=1` enable Rust diagnostics independently. Never enable them for production benchmarking without recording the setting.

## Circuit layout and other tests

`npm run test:schematics` writes its gallery and cache under `debugging/circuit-layout/`; run `npm run test:schematics -- --help` for filters, offline replay and workers. Pattern tests write `debugging/circuit-layout/patterns/`. These paths are local generated artifacts, not source data.

## Experiment record required

For every placement experiment, add or update a short record under `docs/experiments/<topic>/README.md` before declaring it finished. Include the code revision and exact command/input; the hypothesis and changed parameters; measured wall time and, where available, stage times and candidate counts; validity, hard violations, electrical and geometry metrics; links to saved before/after board and block visuals; and a clear verdict (`successful`, `regression`, or `inconclusive`) with the reason. Distinguish a native replay from a full-board result. Keep a successful implementation in a commit, and name its commit in the experiment record. Do not claim a speedup or quality improvement from a single warm replay or a visually attractive but invalid placement.
