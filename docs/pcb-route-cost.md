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
3. `comparePostPlaceRouteCandidate()` resolves the **same references** in each
   candidate pose. It does not reselect nearest pads or add new ordinary jobs.
4. Each evaluation routes jobs sequentially in priority order, retaining virtual
   copper within that evaluation. Candidate evaluations never share occupancy.

The native plan has one total cap (32 jobs), an ordinary cap (16), and the
existing per-net sampling cap. Ordinary jobs do not duplicate an explicit
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
via fallback with restored real cost, fixed plans despite changed nearest pads,
and a captured ESPower USB placement. The fixture keeps all 53 component body
boxes and 174 pad obstacles but limits explicit routing obligations to the four
USB segments. A separate refiner test keeps the surrounding geometry fixed and
verifies that the actual R7/R8 swap is accepted despite a slightly worse global
geometric score.

These are estimator regressions, not a claim that a production router will
produce identical copper. The USB connector side can still require vias.

## Native rebuild

The change adds `prepareRouteLayoutComparison` and
`compareRouteLayoutCandidate` to the native addon. Rebuild it before running the
updated TypeScript:

```sh
npm run native:build
npm run check
cargo test --release --manifest-path native/pcb-board-packer/Cargo.toml --lib
```
