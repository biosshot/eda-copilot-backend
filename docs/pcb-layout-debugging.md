# PCB layout debug output

Run from the backend root:

```text
npm run debug:pcb-layout -- capture <fixture>
npm run debug:pcb-layout -- replay <native-capture-dir-or-meta.json> [repeats]
```

`capture` runs the complete fixture once. The console shows its status, duration, native request count, and the path to `summary.md`. Detailed progress, profiles, and errors go to `run.log`.

All generated files are local and Git-ignored under `debugging/pcb-layout/`:

| Path | Contents |
| --- | --- |
| `runs/<fixture>/<timestamp>/summary.md` | First file to read: status, elapsed time, request counts, stage timings, and links to output. |
| `runs/<fixture>/<timestamp>/summary.json` | The same run in machine-readable form, including native capture metadata and hashes. |
| `runs/<fixture>/<timestamp>/run.log` | Complete console output from the fixture, including errors and native profiling. |
| `runs/<fixture>/<timestamp>/source/` | Copy of the fixture's JS, JSON, and TypeScript runner at capture time. |
| `runs/<fixture>/<timestamp>/stages.json` | End-to-end stage durations and `placementOk`, if placement reached completion. |
| `runs/<fixture>/<timestamp>/placement/` | Full-board SVG and JSON, block/stage previews, resolved input, and `board.assemble.json`, if generated. |
| `runs/<fixture>/<timestamp>/native/<block\|board>/<process>/<request>/` | One exact Rust solver call: `problem.json` input, `meta.json` identity/timing, and `solution.json` when it completed. |
| `replays/<block\|board>/<timestamp>-<label>/` | Result of rerunning **one saved native call**, not a new full-board run. Contains `summary.md`, `summary.json` with timings and baseline match, and the new `solution.json`. |

To investigate one slow block, locate its request directory under a run's `native/block/` using the block label or `summary.json`, then pass that directory to `replay`. The replay uses the captured `problem.json` exactly; it does not parse the DSL, rebuild the graph, or create board/block SVGs. `replays/` is therefore separate from `runs/` and is not an alternative board placement.

A process exit of zero is not enough to establish a successful placement. Check `stages.json` (`placementOk`), the placement report, and hard violations. If a capture fails early, some output files are absent; `run.log` and `summary.json` explain what happened.
