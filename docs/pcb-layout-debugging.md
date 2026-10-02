# PCB layout debug output

Run from the backend root:

```text
npm run debug:pcb-layout -- capture <fixture>
npm run debug:pcb-layout -- replay <native-capture-dir-or-meta.json> [repeats]
```

`capture` runs the complete fixture once. The console shows its status, duration, native request count, and the path to `summary.md`. Detailed progress, profiles, and errors go to `run.log`.

Capture inherits `PCB_BLOCK_BACKEND`, `PCB_BOARD_BACKEND` and `PCB_POST_PLACE_BACKEND`;
it does not change solver selection. Unset means the native default `auto`, which
can deliberately select CPU for a workload below its measured GPU threshold.
An explicit GPU request for board packing in PowerShell is
`$env:PCB_BOARD_BACKEND='cubecl'` before the capture command. Missing/unsupported
GPU and runtime failures still recover on CPU. Clear that variable afterward
with `Remove-Item Env:PCB_BOARD_BACKEND` if subsequent runs should use `auto`.

`run-manifest.json` is written **before** launching the fixture and contains
requested backends, revision/dirty state, source hashes and the addon hash at
launch. It survives an interrupted run. Each native `meta.json` also records
`backendRequest` before solving. These fields describe the **request**, not the
actual execution backend. Native decisions/fallback reasons are in `run.log`
and, on completion, `summary.json.profile.backendDecisions`. Capture enables
board detail profiling as well as block detail; `profile.boardDetails` stores
nested accumulated worker timings, which must not be added to wall time.

All generated files are local and Git-ignored under `debugging/pcb-layout/`:

| Path | Contents |
| --- | --- |
| `runs/<fixture>/<timestamp>/summary.md` | First file to read: status, elapsed time, request counts, stage timings, and links to output. |
| `runs/<fixture>/<timestamp>/summary.json` | The same run in machine-readable form, including native capture metadata and hashes. |
| `runs/<fixture>/<timestamp>/run.log` | Complete console output from the fixture, including errors and native profiling. |
| `runs/<fixture>/<timestamp>/source/` | Copy of the fixture's JS, JSON, and TypeScript runner at capture time. |
| `runs/<fixture>/<timestamp>/stages.json` | End-to-end stage durations and `placementOk`, if placement reached completion. |
| `runs/<fixture>/<timestamp>/placement/` | Full-board SVG and JSON, block/stage previews, resolved input, and `board.assemble.json`, if generated. |
| `runs/<fixture>/<timestamp>/native/<block\|board\|refine>/<process>/<request>/` | One exact Rust solver call: `problem.json` input, `meta.json` identity/timing, and `solution.json` when it completed. |
| `replays/<block\|board\|refine>/<timestamp>-<label>/` | Result of rerunning **one saved native call**, not a new full-board run. Contains `summary.md`, `summary.json` with timings and baseline match, and the new `solution.json`. |

To investigate one slow block, locate its request directory under a run's `native/block/` using the block label or `summary.json`, then pass that directory to `replay`. The replay uses the captured `problem.json` exactly; it does not parse the DSL, rebuild the graph, or create board/block SVGs. `replays/` is therefore separate from `runs/` and is not an alternative board placement.

Native refiner captures include both local geometric and final route-aware calls, distinguished by `meta.stage`. Replay keeps the captured timeout and compares result fields without nondeterministic profile timings. Two time-limited searches can finish different candidate subsets; inspect quality and stop reasons rather than assuming a result mismatch proves arithmetic drift.

A process exit of zero is not enough to establish a successful placement. Check `stages.json` (`placementOk`), the placement report, and hard violations. If a capture fails early, some output files are absent; `run.log` and `summary.json` explain what happened.

## Log field reference

`capture` stores all these lines in `run.log`. One line is usually one solver call or one refinement iteration; counters from different lines are not necessarily disjoint. Millisecond fields end in `Ms` or `_ms`; `[block-detail]` alone uses nanoseconds in its raw pairs.

| Prefix | Meaning |
| --- | --- |
| `[pcb-layout]` | Overall placement stage and progress percentage. |
| `[pcb-block-solver]` | One native block solve: `components` is component count; `beam_ms`, `singles_ms`, `pairs_ms` are elapsed time in search, single-move improvement, and pair improvement. |
| `[block-detail]` | Detailed counters for one native block solve; see below. |
| `[pcb-board-packer]` | Board search times: `beam` (includes initial packing), `local_improve`, `repair`, `total`; `threads`, hard violation count (`hard`), and joint candidate count. `candidates ...` appears only when tracing a selected designator. |
| `[pcb-post-place]` | Human-readable progress for final placement refinement. |
| `[pcb-post-place-native] iteration=N` | One native post-place refinement iteration; see below. |
| `[pcb-post-place-native] total=...` | Total native refinement time in milliseconds, actual `threads`, executed/allowed `passes`, and `stop` reason (`no_improvement`, `timeout`, `iteration_limit`, or `disabled`). |

### `[block-detail]`

`components` is the number of block components. `pairsOnly=true` means this call resumed a saved single-move state to run pair refinement; `false` means a normal solve. `padCache=[hits, misses]` counts reuse and recomputation in the pad-crossing cache.

Each `totals` entry is `[count, total_nanoseconds]`. For timed operations, `count` is invocation count; for `candidate_*_raw`, `candidate_*_unique`, and `candidate_net_pad_pairs`, it is a **sum of produced/considered items**, so the second number is `0`. These are totals over this solver call, not globally unique board candidates. Timed operations nest, so their durations must not be added to estimate wall time. `summary.md` converts the raw nanoseconds to accumulated milliseconds.

| Field in `totals` | What is counted or timed |
| --- | --- |
| `block_candidates` | Generating positions for one primitive, before scoring. |
| `candidate_orientations` | Orientation variants considered across generation calls. |
| `candidate_body_raw`, `candidate_relation_raw`, `candidate_net_raw`, `candidate_bridge_raw` | Raw proposals near component bodies, explicit relations, matching net pads, and signal-path bridges. |
| `candidate_total_raw` | All raw proposals before deduplication; sums the preceding proposal sources for each orientation. |
| `candidate_per_orientation_unique`, `candidate_positions_unique` | Positions after deduplication within each orientation, then after merging orientations/fallbacks. |
| `candidate_net_pad_pairs` | Matching moving/placed pad pairs used as net-position anchors; not a count of generated positions. |
| `candidate_hard_violation_count`, `hard_geometry_violation_count` | Hard-constraint checks of a proposed primitive and full-geometry checks. |
| `candidates_pruned_hard`, `candidates_pruned_score` | Candidates rejected because hard rank could not enter the shortlist or partial score exceeded its safe bound. |
| `score_block_with_overlap_matrix` | Full or bounded block score calculation (the main scoring envelope). |
| `block_micro_route_penalty` | Extra route-quality check on shortlisted candidates. **Historical name:** with `routing_metric=geometric`, this calls the fast geometric route penalty, not Micro-A*. `routing_metric=off` returns zero; micro routing is used only if explicitly selected. |
| `convex_hull`, `overlap_penalty` | Compactness hull and physical overlap parts of the score. |
| `dense_ic_access_penalty`, `external_port_exposure_penalty`, `port_facing_penalty` | Room to escape IC pads, access to external block connections, and port direction. |
| `scoped_relation_penalty`, `same_net_spread_penalties`, `signal_path_topology` | DSL relation distances, spread of pads on the same net, and signal-path structure. |
| `direct_pad_crossing_penalty`, `long_local_net_penalty` | Direct-line crossings of unrelated pads and excessively long local nets. |
| `power_frame_build`, `power_frame_append`, `power_yield_penalty` | Build/reuse of incremental power-access data, or full power-access scoring. |
| `long_net_frame_build`, `long_net_frame_append` | Build/reuse of incremental long-net data. |

### `[pcb-post-place-native] iteration=N`

All `*Ms` fields here are milliseconds. The pass generates possible moves, rejects illegal or unhelpful ones, and applies at most one best improvement.

| Field | Meaning |
| --- | --- |
| `generatedCandidates`, `generationMs` | Distinct proposed moves and wall time to generate them. |
| `candidates`, `evaluationWallMs` | Moves actually entered into evaluation and wall time spent evaluating the batch; these can differ if timed out. |
| `geometryMs`, `hardRejected` | Time checking hard placement violations and number rejected there. |
| `globalScoreMs`, `scoreNativeMs` | Time scoring candidate board geometry inside Rust. These currently measure the same work. |
| `scoreEncodingMs` | Reserved for encoding a score request across a language boundary. Current native refinement scores directly in Rust, so this stays `0`; it is not missing measurement of `globalScoreMs`. |
| `baselineEvaluations`, `baselineCacheHits`, `baselineMs` | Route baselines computed, reused, and time spent computing new baselines. Used for route-aware refinement. |
| `boundRejected` | Candidate rejected by an optimistic route-improvement bound before the route comparison. |
| `routeEvaluations`, `routeMs` | Number and time of detailed route comparisons. `0` means this pass used geometric scoring, or no candidate reached route comparison; it does **not** mean no connections were scored. |
| `routeNativeMs` | Native route work including baseline preparation and comparison. |
| `routeEncodingMs` | Reserved route-request encoding time; stays `0` in the current all-Rust path. |
| `feasibilityRejected` | Candidates rejected because the route comparison worsened feasibility. |
| `insufficientImprovement` | Legal candidates whose improvement did not exceed the configured minimum. |
| `accepted` | Whether this iteration applied a best move. |
| `timedOut` | Whether the overall refinement deadline had been reached by the end of this iteration. |

For example, `routeEvaluations=0`, `scoreEncodingMs=0`, and `globalScoreMs>0` means the pass still scored candidates, entirely inside Rust, without a detailed route comparison. `total` can exceed the sum of per-field times because it also includes setup, scheduling, diagnostics, and other uninstrumented work.
