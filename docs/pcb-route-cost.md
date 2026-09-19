# Bounded route-cost comparison during PCB refinement

Micro-A* is a placement estimator, not the production PCB router or a DRC pass.
Its values are **costs/penalties: lower is better**. It does not create copper.

## Why swap variants share a route plan

The post-place refiner still generates legal pose exchanges and 180-degree
rotations. It first checks hard placement constraints, then compares the existing
global placement score plus a native route penalty.

For every changed-designator set in one refinement iteration:

1. `preparePostPlaceRouteComparison()` resolves endpoints, priorities and weights
   from the current layout, selects a bounded job list, and evaluates that list.
2. The resulting serializable baseline is cached for that iteration only.
3. `comparePostPlaceRouteCandidate()` resolves the **same terminal references**
   in each candidate pose. Explicit pairs retain their endpoints. For complete
   ordinary nets recorded in `topologyNets`, the geometric minimum spanning tree
   is rebuilt at the candidate coordinates, allowing a different intermediate pad.
4. Each evaluation routes jobs sequentially in priority order, retaining virtual
   copper within that evaluation. Candidate evaluations never share occupancy.

The native plan has one total cap (32 jobs) and an ordinary cap (16). Selected
ordinary nets with 3–8 terminals and no explicit jobs on that net are upgraded
atomically to a complete tree (N-1 jobs), bypassing the two-job sampling cap
only when the whole tree fits both budgets. Every pad reference is retained,
including pads on unchanged components. Other nets keep the existing sampled
pair estimator. Version 2 baselines identify complete nets in `topologyNets`;
legacy version 1 baselines continue to use fixed pairs.

The tree is chosen by geometric distance; Micro-A* then evaluates its edges
with obstacles and temporary same-net copper. This does not yet search alternative
trees around obstacles or compute a Steiner tree. Tree-edge samples are compared
in deterministic slots within each net, with the same priority, weight and via
price; unresolved counts still prevent dropping connectivity for a cheaper score.

Ordinary jobs do not duplicate an explicit
component-pair/net obligation just because another pad of that component becomes
closer. This is a representative estimate, not proof that all duplicate physical
pads of a net have been connected.

The paired comparison is intentionally different from calling
`postPlaceRoutePenalty()` independently on two layouts. That compatibility API
can select different jobs; it is not the acceptance criterion for swaps.

## Temporary copper and clearance

Grid ranges are only a spatial lookup window. Occupancy checks use geometric
centerline distances to the actual reconstructed planar segments. Same-net
reuse is allowed; different nets must satisfy trace width plus clearance.

The default grid is 0.25 mm, trace width 0.127 mm, and clearance 0.254 mm. Thus
required trace-center spacing is 0.381 mm: two centers 0.5 mm apart are not
blocked merely because `ceil(0.381 / 0.25)` equals two grid cells.

Via landings occupy the layers traversed by the virtual path. The existing
simplified via diameter/stack model is unchanged; manufacturing via DRC is not
implemented here. Static pad geometry and its endpoint-access model are also
unchanged by this fix.

## Search outcomes and the bounded fallback

A route sample reports one of:

- `found`: a path was obtained in the estimator's model, with physical cost,
  planar length and via count.
- `budget_exhausted`: search stopped at its expansion cap; existence of a path
  is unknown, not disproved.
- `no_path`: an endpoint is blocked in the model or the search frontier emptied.

The first search retains the configured route-class via prices and is capped
at 1,500 expanded states. Post-place evaluation allows one additional search,
capped at 3,000 states, only after budget exhaustion. The retry uses a lower
**search-order** via price so it can explore another layer instead of spending
its budget entirely on the top layer. The returned cost restores the full real
price for every via. `usedFallback` distinguishes this feasible fallback from a
normal search result; it is not guaranteed to be shortest under the real price.

The total is bounded by 4,500 expanded states per post-place job. Board/block
candidate rerankers retain their 1,500-state cap without this extra retry.

Missing routes have `null` physical cost and detour rather than a fabricated
length. For one before/after obligation, an unresolved penalty is based on:

```text
max(known before detour, known after detour, 2 * job via cost) + 30 mm
```

The same unresolved price is used on both sides of that comparison and is then
multiplied by the job weight. Thus an unresolved job cannot be cheaper than its
known successful counterpart, including a costly via route. The refiner also
rejects a candidate that worsens unresolved priority-weighted obligations
(highest priority first). Budget exhaustion remains diagnostic uncertainty;
this conservative policy does not certify physical unroutability.

## Diagnostics and regression fixtures

`PostPlaceMove` records route penalties, job count, unresolved counts and
budget-exhausted counts. The native baseline/candidate APIs additionally return
per-job status, expanded states, via count and `usedFallback`.

`tests/pcb-route-cost-comparison.test.ts` covers adjacent 0.5 mm endpoints,
via fallback with restored real cost, fixed explicit plans despite changed nearest pads,
ordinary-net topology changes with preserved terminals, complete five-pad trees,
and a captured ESPower USB placement. The fixture keeps all 53 component body
boxes and 174 pad obstacles but limits explicit routing obligations to the four
USB segments. A separate refiner test keeps the surrounding geometry fixed and
verifies that the actual R7/R8 swap is accepted despite a slightly worse global
geometric score.

These are estimator regressions, not a claim that a production router will
produce identical copper. The USB connector side can still require vias.

## Search performance

The micro-router preserves the search order, expansion budgets and physical
costs while avoiding repeated work:

- Static cell checks are memoized within each search. Endpoint, net and layout
  exceptions cannot leak to the next job or candidate.
- Temporary-copper checks are memoized by directed edge, separately from static
  cells, because clearance depends on both ends of the move.
- Via distances to the goal layer are computed once per search. Neighbor lists
  use inline storage (with a heap fallback for more than six neighbors).
- Costs and reconstruction parents share one state table.
- Unit grid edges use an exact endpoint-distance shortcut for copper clearance.
  Longer/diagonal edges and mixed-net endpoint occupancy retain the general
  segment predicate. No square keepout approximation is introduced.

To measure the native API on the saved ESPower geometry:

```sh
npm run native:build
node scripts/benchmark-micro-router.mjs /absolute/path/to/baseline.node
cargo test --release --manifest-path native/pcb-board-packer/Cargo.toml benchmark_route_search -- --ignored --nocapture
```

Build and save the baseline addon from the revision being compared before
building the current addon. Omit the baseline argument to time only the current
version. The script checks exact equality of all returned route samples, warms
both versions, then alternates their timing order for six batches of 20 calls.
Both ESPower cases route the same four USB obligations; the second additionally
schedules jobs with every component marked changed. This measures route
evaluation including native API conversion and scheduling, not full placement.

Measured on Windows x64, release build, 2026-09-19 (median milliseconds per call):

| ESPower affected set | Before | After | Time reduction |
| --- | ---: | ---: | ---: |
| R7/R8 USB | 6.848 | 5.656 | 17.4% |
| All components | 8.184 | 7.008 | 14.4% |

Both comparisons returned exactly equal route samples. Timings vary by machine
and board. If route evaluation accounts for 40% of total placement time, a
17.4% reduction in that portion would reduce total time by about 7%; this is an
estimate, not a measured full-placement result.

The Rust regressions compare complete paths, costs, expansion counts and
outcomes against the reference loop, including directed multilayer transitions,
foreign/same-net copper, pad exceptions and clearance boundaries.

## Avoiding unnecessary solves

Further acceleration keeps the existing routing budgets and candidate sets:

- Block greedy/beam selection and board beam selection evaluate routing lazily.
  The geometric rank plus inherited route penalty is a lower bound, since the
  candidate's route correction is nonnegative. Once the selected top-k ranks
  are exact, the remaining candidates cannot enter that top-k.
- Local improvement skips candidates whose lower-bound rank cannot win. Board
  candidates are streamed in final rank order, preserving the existing severity
  epsilon, score epsilon and ordinal tie handling.
- Post-place baselines expose optional `maximumImprovement`. For a resolved
  route the ceiling is its existing detour cost. For an unresolved route it is
  `(2 * viaCost + unroutablePenalty) * weight * routeScale`, including the case
  where the candidate's known detour exceeds the normal unresolved penalty.
  Candidate routing is skipped only when this ceiling cannot meet the minimum
  improvement or beat the incumbent, with a conservative numerical margin.
  Older addons/baselines without the field keep eager candidate evaluation.
- Route problem encoding omits placement-only component collision matrices.
  The router still receives every primitive body and pad obstacle it used before.
- Exact block/board solutions are reused across attempts in the same process.
  A bounded LRU is scoped to the loaded addon and keyed by the full encoded
  input, including geometry, nets, constraints, options and signed zeros.
  Results are cloned to prevent mutation of cached values. It retains at most
  64 entries and an estimated 16 MiB of keys/serialized results per addon;
  oversized requests bypass it. Workers have independent caches. Set
  `PCB_NATIVE_SOLVE_CACHE=0` for cache-disabled measurements.

End-to-end ESPower measurements on Windows x64 (single sequential samples,
2026-09-19), compared with the addon after the first micro-router optimization
and with its solve cache disabled:

| Attempt | Reference | Updated | Reduction |
| --- | ---: | ---: | ---: |
| Original 48 x 32 mm board, first solve | 28.00 s | 20.97 s | 25.1% |
| Next attempt: change width to 49 mm | 26.52 s | 16.58 s | 37.5% |
| Repeat that 49 mm variant | not measured | 3.56 s | — |

All runs reported valid placements of 53 components. Full placement/report/layout
checksums matched the reference for both widths. Post-place candidate routing
calls fell from 562 to 38; baseline route evaluations remained at 307. Changing
width reused unchanged subproblems but still ran the board solver (and nine
block solves); the exact repeat avoided all block/board solves. Timing gains
depend on the board and how much of the next attempt is unchanged. These samples
are not a general throughput guarantee or a distribution of benchmark runs.

Reproduce full-pipeline measurements with:

```sh
node --import tsx scripts/benchmark-pcb-placement.ts ESpower 3 report.json
node --import tsx scripts/benchmark-pcb-placement.ts ESpower 3 report.json variant.js
```

The optional variant DSL is used after the first attempt. Set
`PCB_BOARD_PACKER_NATIVE_PATH` to an older release addon for reference runs.
Reports include checksums and native call counts/timings; compare checksums for
the same DSL before drawing performance conclusions. No files are exported by
the placement pipeline during these measurements.

## Native rebuild

The change adds `prepareRouteLayoutComparison` and
`compareRouteLayoutCandidate` to the native addon. Rebuild it before running the
updated TypeScript:

```sh
npm run native:build
npm run check
cargo test --release --manifest-path native/pcb-board-packer/Cargo.toml --lib
```
