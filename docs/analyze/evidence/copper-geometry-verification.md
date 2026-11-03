# Copper geometry acceptance evidence

2026-10-21. Review found R1 (false intra-source neck) and R2 (missing strict
addon export expectation). Initial measurements below are historical and are
**not evidence of topology preservation**. The current single-geometry policy
and verification record below supersede the historical connectivity claims.

Initial stage implemented `next-task-copper-geometry.md`. Current revision removes
the three-set correction at the user's request; no commit is made for this revision.
This is physical copper/topology preparation, not completion of P4 or DC.

## Boundaries and modules

```text
RoutingBoardV2 JSON → unchanged import_json → model::Board
  → geometry/primitives.rs: pad shapes, round strokes, drills, local transform
  → geometry/areas.rs: normalized/validated polygons, boolean operations
  → geometry/spatial.rs: bounding-box sweep and R-tree queries
  → geometry/topology.rs: narrow-phase contacts, barrels, physical islands
  → PreparationReport (serializable polygons, provenance, findings, diagnostics)
  → shared node-addon.prepareAnalysisGeometry
  → existing AnalysisPool/process worker, kind: prepare-geometry
  → fixture geometry.ts/common helper → report.json + geometry.json + layer SVGs
```

The internal task/result conditional types distinguish import from preparation.
No scheduler, public package export, Node addon, cache, session, CAD framework,
placement mode or transport format was added. The host adds exactly one export.
`importAnalysisBoard` and its `geometry_not_prepared` diagnostic remain unchanged;
the new report explains its own preparation status separately.

## Dependency and numeric policy

Checked existing packer geometry (F32 placement obstacles, unsuitable as physical
copper), sibling exporter (JS clipper, cannot be the Rust geometry runtime), and
the primary library documentation before selecting **geo = 0.31.0**, with
**default-features = false**. MIT OR Apache-2.0, Rust >=1.85; pure Rust with no
external executable/GEOS/PROJ runtime. This version supplies validity checks,
DE-9IM robust predicates, boolean operations and prepared geometry indexes.
Spade, earcut and parallel features are disabled. Direct **rstar = 0.12.2** reuses
geo's existing transitive R-tree implementation. Lockfiles pin **i_overlay 4.0.7**,
**i_float 1.15.0**. No faer/FFT/meshing dependency was added.

Primary references: [geo manifest/features/license](https://docs.rs/crate/geo/0.31.0/source/Cargo.toml.orig),
[boolean operation validity contract](https://docs.rs/geo/0.31.0/geo/algorithm/bool_ops/trait.BooleanOps.html),
[i_float adapter source](https://docs.rs/crate/i_float/1.15.0/source/src/adapter.rs).
Portability is source/dependency review plus local Windows verification; other
target CI has not been run from this task.

Analyze coordinates/computation remain **F64 mm, absolute Y-down**, without packer
FTZ guards or GPU initialization. geo boolean operations internally use a local
integer adapter: centre of operand bounds, power-of-two scale
`2^(29 - roundAway(log2(max(halfWidth, halfHeight))))`, then rounded integer coordinates.
The report's `boolean_error_bound_mm = max(1e-12, span*64/2^29)` guards the
bounded conversion pipeline separately from sagitta. It is not a connection
tolerance or a standalone theorem about topology. A boundary error alone cannot
certify that a nominal region remains internally connected in true geometry.

| Setting | Default | Accepted range |
| --- | ---: | --- |
| `max_deviation_mm` | 0.001 | 0.000001 .. 0.01 mm |
| `max_vertices_per_curve` | 16384 | 8 .. 65536 |
| `max_generated_vertices` | 2000000 | 8 .. 10000000 |
| Coordinate magnitude | — | <=10000 mm per axis, including transformed/drilled boundaries |

Circles use a multiple-of-four segment count satisfying
`r*(1-cos(pi/n)) <= max_deviation_mm`, exact cardinal vertices and inscribed
chords. Resource overflow excludes the source with an invalid diagnostic; it
never increases error automatically. Rings narrower than the boolean resolution
are explicitly excluded/invalid, not silently simplified. Consecutive duplicate
and repeated closing vertices normalize equivalently. Self-intersections,
misplaced/intersecting holes and invalid boolean output are not repaired.
No small-area/island filter is enabled. Input CAD tessellation error/freshness
is not improved or certified by tightening these settings.

### Single-geometry connectivity policy (current revision)

There is one `regions` collection and one graph, `contacts` / `islands`.
Findings reference the same region IDs. `layers` aggregates those regions for
rendering/area measurement. No whole-board offsets, second graph, alternative
region/contact/island collections or offset guard retry loop are retained.

Copper curves retain their existing inscribed tessellation. Round and slot
drills reuse that builder with circumscribed circles: the existing angular
phase and vertex radius `r/cos(pi/n)`, choosing n so that
`r*(sec(pi/n)-1) <= max_deviation_mm`. The drill radius additionally includes
`2*boolean_error_bound_mm` for conversion rounding. Slot centreline length,
local offset/rotation, plating and confirmed layer scope retain their meanings.
The derived drill polygon encloses the physical bore instead of leaving copper
inside it. `approximation_mm` includes the curve and drill conversion allowance.
This changes prepared drill contours/areas within their explicit allowance;
the saved inputs and CAD fill polygons are not rewritten.

Only when subtraction splits or entirely removes a source polygon, a temporary
local subtraction with inscribed drills compares component counts. A difference
produces `drill_topology_uncertain` with source ID/path and `partial` status.
An exhausted local comparison produces `drill_topology_unresolved`. No second
board or graph survives that check. Failure to generate/validate an enclosing
drill returns `invalid` / `drill_geometry_unresolved`, without unconfirmed
connectivity fallback or relaxed tolerance. Vertex budgets count generated
primitives including the temporary local check.

The supplied Ø2 mm / 1.999 mm strip no longer produces a confirmed short. A
true 2.001 mm strip can be conservatively split at coarse tolerance; this is
explicitly uncertain, **not a confirmed open**, and finer tolerance resolves
the control case. Neither a boundary-error bound nor the local component-count
check is a universal topology proof: it does not compare every possible contact
partition or certify arbitrary sub-resolution features. No such claim is made.

Contacts retain the existing area/line/point/uncertain classification and uncut
source-gap check. Plating requires a clear uncut-source witness near the
surviving bore boundary. Net labels never repair geometry. `net_islands` lists
separate reliable components and does not establish an open. Missing/stale CAD
fills, unconfirmed drill spans and arbitrary near-tangencies remain explicit
limits; tighter tolerance cannot supply missing CAD evidence.

## Supported geometry and semantics

All PadShape forms, per-layer shapes, fixed/editable round-cap/round-join polyline
strokes (including bends and explicitly closed paths), through/blind/buried vias,
and both zone kinds' actual Available fills are supported. Overlap is unioned once
per layer for area, while individual source fragments retain all provenance.
Names/ownership/disableRouting/component identity do not command connectivity.
Unknown-net copper remains physical copper; known labels on the same confirmed
physical island produce a short finding, without choosing a label.

Pads apply local rotation/translation once; component pose and bottom mirroring
are not reapplied. Slots use **overall length**, so the centreline segment is
`length - diameter`; hole offset rotates with the pad, slot rotation is additive.
This matches the current sibling `easyeda-board-v2.ts::padHole` and native slot
fields/rendering. Round stroke semantics match the exporter's documented
round-cap/join expansion. Captured exporter warnings for native stroked fills
remain a CAD-equivalence limitation; fills are used as already exported, never
expanded again or replaced by their boundary.

Span semantics were checked against the exporter and
[EasyEDA Pro pad editor](https://prodocs.easyeda.com/en/footprint/side-panel-left-pad/):
native MULTI pads are through-hole pads, exported to every copper layer.
All-layer pad holes and exported drill-only NPTH holes are through cuts.
Subset-layer pad holes carry no confirmed full span in V2: they cut only the
declared pad layers (including neighboring copper there), get a limitation,
and cannot establish a barrel or an intermediate/through cut. Independent
plated drills have no confirmed layers and get no invented subtraction/barrel.
Independent plating supplies neither net nor lands. Via spans use only
validated endpoints; no external-layer connections are made.

Drills subtract neighboring copper as well as their own lands. Board outline and
cutouts intersect the derived geometry; any positive detected outside area is
reported, source data remains unchanged, and edge/castellated validity is not
declared. Polygon holes are empty material, not standalone drills. Keepouts are
retained in the source inventory and do not remove captured conductors.

Available-empty is empty copper. Missing/failed/unsupported fill is not replaced
by a zone outline. Freshness current/stale/unknown is retained separately;
stale/unknown and missing data make preparation partial. Invalid geometry makes
the result invalid and lists exclusions; it is not an ordinary prepared board.
Real shorts retain the geometry and are board findings, not import failure.
Stackup absence does not block planar work; no thickness/resistance is invented.

## Coupon oracles

Expected values below are analytic, not generated from the production output.
Curved area checks use perimeter × boundary error plus boolean rounding allowance;
the tests tighten sagitta from 0.001 to 0.0001 mm and check non-increasing error.

| Coupon | Expected area / topology |
| --- | --- |
| Rectangle 4×2 | 8 mm², exact authored edges |
| Circle R=2 | 4π mm², cardinal bounds ±2 |
| Oval 6×2 | 8+π mm² |
| Round-rect 6×4, r=1 | 20+π mm² |
| Round stroke L=4, w=2 | 8+π mm² |
| Coincident fixed/editable strokes | 8+π once, both sources retained |
| Two perpendicular strokes | 2(8+π)−4 mm² |
| Right-angle bend, two length-4 segments | 2(8+π)−(1+3π/4) mm² |
| Closed square side 4, stroke 2 | 28+π mm², one empty centre |
| R=2/r=1 drilled ring | 3π mm² |
| 6×6 polygon, 4×4 hole, separate 1×1 island | 21 mm², two components |
| Slot length 3, diameter 1 | 2+π/4 mm², offset/rotation checked independently |

Other asserted oracles cover split net, physical short/unknown-net bridge,
noncontact despite BBOX overlap, positive gaps, shared edge, point touch, narrow
bridge, malformed outer/holes, through/blind/buried spans, global NPTH cuts,
plated pad and unconfirmed independent plating, layer shapes, bottom transform,
90°/37° whole-case rotation, translation/permutation, CAD pad-claim mismatch,
fills/freshness/empty/keepout/edge clipping and generation limits. A separate
coupon places a foreign pad wholly in a circular drill's approximation band:
no reliable barrel short may be created from the inscribed-hole residual.

## Commands and measured results

Initial pre-review validation: Windows x64, Node 26.5.0, rustc 1.98.1, Ryzen 5 5600H (12 logical cores).
Generated local evidence belongs to `debugging/copper-geometry/`, not fixtures.

```powershell
npm run native:build
$env:PCB_ANALYZE_RUST_REPORT_DIR = "$PWD/debugging/copper-geometry/rust"
cargo test --release --locked --manifest-path native/analyze/Cargo.toml -- --nocapture
npm run typecheck
npm run build
node --import tsx --test --test-concurrency=1 tests/pcb-analysis-geometry.test.ts tests/pcb-analysis-runtime.test.ts tests/pcb-board-packer-native.test.ts tests/public-api.test.ts
node --test scripts/test-pcb-analyze.test.mjs
npm run test:pcb-analyze -- --all --name geometry
node scripts/check-pcb-analysis-geometry.mjs
npm run test:package
git diff --check
```

- Rust: **17 geometry + 13 unchanged import tests passed**, no skipped tests. The corpus test writes complete Rust reports when `PCB_ANALYZE_RUST_REPORT_DIR` is set.
- Node: **37 passed, 0 failed, 0 skipped**: real preparation binding/process/cancellation, 19 runtime lifecycle/race tests, and existing packer/public API checks. Focused run used CPU placement backends.
- Launcher infrastructure: **4 passed**, including all saved source hashes/native inventories. Eight geometry launches: **8 completed, 0 failed**; all eight full reports exactly match standalone Rust, not just counts.
- Native release rebuild, typecheck, build (also through prepack), installed archive/type consumer, and diff whitespace checks passed. Prior packer warnings remain; analyze added no compiler warnings.
- Compatibility: **43 old exports retained, exactly 1 preparation export added**. All prior versions/numeric contracts and eight complete import reports match the pre-change capture. Short CPU placement replay matches the saved solution and quality exactly: hardCount=0, score=772.9915771484375. Placement → geometry → placement also matches, with `f32-rte-ftz-v1` retained.
- Installed package: existing placement worker/public types and import worker checks, plus the real preparation binding/compiled worker and invalid-JSON result, passed in normal, GPU-disabled and both-driver-unavailable smoke modes. No source/tsx dependency or public `/analyze` export was introduced.

Evidence: [machine report](../../../debugging/copper-geometry/evidence.json), [compatibility comparison](../../../debugging/copper-geometry/compatibility.json), [Rust log](../../../debugging/copper-geometry/rust-tests.log), [Node log](../../../debugging/copper-geometry/node-tests.log), [package log](../../../debugging/copper-geometry/package.log), [packed archive](../../../.artifacts/eda-copilot-backend-0.4.0.tgz).

## Eight-board artifacts and coverage

All eight are **partial**, with **0 excluded sources**. No confirmed short was found in this conservative graph; uncertain contacts and incomplete CAD data prevent treating that as proof of no shorts. Area is the sum of per-layer copper unions, counting overlap once on each layer.

| Fixture / report | Status | Regions | Physical islands | Copper mm² | Worker+IPC s | Pretty report MiB | Observed worker peak MiB | Shorts |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| [BandAmpV5](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-21T17-25-41-521Z-geometry-Wrnovs/report.json) | partial | 321 | 8 | 2110.623 | 1.319 | 3.98 | 73.5 | 0 |
| [ESPower](../../../debugging/pcb-analyze/ESPower/runs/2026-10-21T17-25-43-217Z-geometry-4ZHiRk/report.json) | partial | 668 | 56 | 1460.973 | 3.152 | 5.74 | 86.4 | 0 |
| [NanoScope](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-21T17-25-46-950Z-geometry-weqNK9/report.json) | partial | 3019 | 175 | 21687.470 | 34.356 | 35.19 | 229.6 | 0 |
| [NanoSpec](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-21T17-26-22-050Z-geometry-6GZfo4/report.json) | partial | 2509 | 160 | 10014.099 | 18.891 | 24.39 | 164.4 | 0 |
| [STM32G0_CAN_RS485](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-21T17-26-41-566Z-geometry-XOMe6z/report.json) | partial | 849 | 73 | 2430.549 | 1.322 | 6.47 | 86.4 | 0 |
| [TPS63060DSCR](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-21T17-26-43-296Z-geometry-klef6r/report.json) | partial | 170 | 8 | 1533.593 | 0.406 | 1.50 | 73.9 | 0 |
| [ThunderF722](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-21T17-26-44-037Z-geometry-Rt1mJO/report.json) | partial | 1896 | 117 | 9638.503 | 13.326 | 25.48 | 196.1 | 0 |
| [USB_PD_Trigger_5A](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-21T17-26-57-990Z-geometry-FMcboI/report.json) | partial | 388 | 20 | 2874.711 | 1.157 | 4.07 | 75.7 | 0 |

| Fixture | Used pad / track / via / pour / fill / keepout sources | Generated vertices | Narrow-phase candidate pairs | Explicit partial reasons (count) |
| --- | --- | ---: | ---: | --- |
| BandAmpV5 | 29 / 21 / 133 / 2 / 0 / 0 | 10536 | 374 | fill_freshness: 2; contact_uncertain: 1 |
| ESPower | 172 / 326 / 74 / 2 / 0 / 2 | 31124 | 1411 | fill_freshness: 2; barrel_contact_uncertain: 2; contact_uncertain: 5 |
| NanoScope | 724 / 1135 / 249 / 8 / 2 / 2 | 135028 | 10181 | drill_span_unconfirmed: 10; fill_freshness: 10; contact_uncertain: 15 |
| NanoSpec | 509 / 823 / 553 / 7 / 18 / 2 | 108432 | 6942 | drill_span_unconfirmed: 1; fill_freshness: 25; barrel_contact_uncertain: 1; contact_uncertain: 14 |
| STM32G0_CAN_RS485 | 213 / 472 / 66 / 6 / 0 / 4 | 47264 | 1472 | fill_freshness: 6; contact_uncertain: 16; point_contact: 2 |
| TPS63060DSCR | 39 / 73 / 24 / 6 / 0 / 0 | 7056 | 419 | drill_span_unconfirmed: 1; fill_freshness: 6 |
| ThunderF722 | 340 / 494 / 136 / 31 / 0 / 2 | 75180 | 3718 | fill_freshness: 31; barrel_contact_uncertain: 2; contact_uncertain: 8 |
| USB_PD_Trigger_5A | 65 / 144 / 80 / 2 / 0 / 2 | 20916 | 873 | fill_freshness: 2; barrel_contact_uncertain: 1; contact_uncertain: 4 |

Every report link above has adjacent `geometry.json`, `measurements.json`, `run.json`, `run.log`, and one `layer-<index>-<name>.svg` per layer. Per-layer areas, source paths/IDs, pad/component references, all input native fill IDs, contacts and findings are in JSON.
- BandAmpV5: [geometry](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-21T17-25-41-521Z-geometry-Wrnovs/geometry.json), [layer SVGs / run directory](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-21T17-25-41-521Z-geometry-Wrnovs/).
- ESPower: [geometry](../../../debugging/pcb-analyze/ESPower/runs/2026-10-21T17-25-43-217Z-geometry-4ZHiRk/geometry.json), [layer SVGs / run directory](../../../debugging/pcb-analyze/ESPower/runs/2026-10-21T17-25-43-217Z-geometry-4ZHiRk/).
- NanoScope: [geometry](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-21T17-25-46-950Z-geometry-weqNK9/geometry.json), [layer SVGs / run directory](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-21T17-25-46-950Z-geometry-weqNK9/).
- NanoSpec: [geometry](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-21T17-26-22-050Z-geometry-6GZfo4/geometry.json), [layer SVGs / run directory](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-21T17-26-22-050Z-geometry-6GZfo4/).
- STM32G0_CAN_RS485: [geometry](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-21T17-26-41-566Z-geometry-XOMe6z/geometry.json), [layer SVGs / run directory](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-21T17-26-41-566Z-geometry-XOMe6z/).
- TPS63060DSCR: [geometry](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-21T17-26-43-296Z-geometry-klef6r/geometry.json), [layer SVGs / run directory](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-21T17-26-43-296Z-geometry-klef6r/).
- ThunderF722: [geometry](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-21T17-26-44-037Z-geometry-Rt1mJO/geometry.json), [layer SVGs / run directory](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-21T17-26-44-037Z-geometry-Rt1mJO/).
- USB_PD_Trigger_5A: [geometry](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-21T17-26-57-990Z-geometry-FMcboI/geometry.json), [layer SVGs / run directory](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-21T17-26-57-990Z-geometry-FMcboI/).

## Independent captured-section checks

The read-only validator `scripts/check-pcb-analysis-geometry.mjs` checks two saved capture sections using independent rectangle inverse transforms, ray casting and shoelace area. It also cross-checks native PAD_NET compound IDs in `source-pcb.txt`. It does not use the production Rust geometry predicates or the exporter to compute these oracles.

| Board | Pad / track, endpoint Y-down | Through via | Native circular fill void |
| --- | --- | --- | --- |
| BandAmpV5 | 1ef61450f2f63c6ee14 / 73d6db5f0c9d2233; (-2.8, 1) lies strictly inside the captured rectangular pad | 856a6903da6d1b01; TOP/BOTTOM lands share a physical island; expected annulus 0.124080932 mm² | raw 8d5bddca2285ea2c, pour 81e4a91d2893b069; centre (-15, -11), R=1.9041; analytic 11.390148303, prepared 11.382346010 mm² |
| ESPower | e536e19 / 581f781a134fbbbf; (3.4549, -2.369) lies strictly inside the captured rectangular pad | a415b2db57d123eb; TOP/BOTTOM lands share a physical island; expected annulus 0.124080932 mm² | raw 01f44c26159f223a, pour 9a9ba671fb863ba8; centre (11.2, -13.7), R=1.4081; analytic 6.228979042, prepared 6.223172015 mm² |

Native fill void oracles come from exactly two native 180° arcs; every prepared void vertex stays within the export tessellation bound, its centre is absent material, and its shoelace area meets perimeter × 0.001 mm. Multiple-island/hole exclusion is also covered by the explicitly synthetic 21 mm² coupon; no additional CAD island claim is invented.

**Concrete capture/transport limitations, left unchanged:** both selected via records contain viaDiameter=19.685 mil and holeDiameter=11.811 mil (0.499999 / 0.2999994 mm), while capture API and V2 contain 0.4978 / 0.2997 mm. Deltas are −0.002199 / −0.0002994 mm. Preparation matches V2/capture; the native-record discrepancy is retained in evidence and cannot be silently corrected or certified as CAD equivalence. The selected V2 fills also contain the pour/group ID in `nativePrimitiveIds`, not the raw fragment IDs listed above. The importer/preparation preserve the supplied IDs; a per-fragment native mapping cannot be inferred from V2 alone. Independent comparison uses captured `pourId` for this check. Neither V2 nor the exporter is changed in this task.

The debug overlays were also rendered/read for BandAmpV5 and ESPower ([BandAmpV5 PNG](../../../debugging/copper-geometry/BandAmpV5-TOP.png), [ESPower PNG](../../../debugging/copper-geometry/ESPower-TOP.png)); visual inspection supplements, rather than replaces, the numeric checks.

## Resources and remaining limits

- Final measured launcher run used the existing test-only dlopen hook and an external Windows process sampler at 100 ms; no instrumentation/debug API was added to production. [Sampler script](../../../debugging/copper-geometry/measure-launcher.ps1), [raw PID log](../../../debugging/copper-geometry/worker-pids.jsonl), [observed OS peak working sets](../../../debugging/copper-geometry/process-resources.json). Worker+IPC time excludes subsequent artifact formatting/writing. Peak samples cover worker processes, not total parent+child RSS, and are observations rather than a memory quota or universal bound.
- Initial pre-review dispersed 100 / 500 / 1000 rectangle coupons: **0 narrow-phase pairs**, 5 / 17 / 37 ms in the recorded Rust run (concurrent native build). Generation counts are 400 / 2000 / 4000. This demonstrates no required whole-board all-pairs narrow phase; it does not promise universal linear complexity, and dense overlapping bounds can still be expensive.
- Sagitta refinement, vertex-limit rejection, the straight-vertex budget and extreme finite coordinates are asserted by coupons. No tolerance increase, small island removal, board source repair or boundary-as-fill fallback is performed. Generated primitive vertices are bounded; CAD input volume, transient boolean allocation and OS memory are not separately quota-limited. Existing process deadline/cancellation remain the isolation boundary.
- Full `npm test`/full placement benchmarks, real GPU numerical replay, Linux/macOS builds and Node 20/24 were **not run locally**. Existing CI already runs locked Rust analyze tests, package build/tests and installed smoke across those target OS/Node matrices; no remote CI result is claimed.
- `complete` only certifies preparation of confirmed geometric input; it is not DC readiness, FEM mesh validity, full DRC or proof of CAD agreement. Unknown/stale fills, unresolved spans, error-band contacts, native capture precision and fragment identity limitations remain explicit. No mesh, DC/thermal/PI/SI/EM solver, refill, materials, DSL, GPU analysis, cache, sessions, compute-runtime or placement algorithm change was introduced.

**Stopped for review. No commit and no following stage.**

## Single-geometry revision verification (2026-10-22)

The previous three-set implementation and its 129.970 s / 94.68 MiB NanoScope
measurement are historical. Their reports/evidence are preserved under
`debugging/copper-geometry/three-set-baseline/`, including the old verification
record. The earlier original one-set baseline is preserved separately under
`corrections-baseline/`. Neither historical baseline is overwritten.

The supplied P1 test is unchanged (including dimensions, angle, tolerances and
assertions). Additional controls retain round/slot, rotated/translated cuts,
near-edge holes, robust true shorts, refined true narrow bridges and exhausted
budgets. The coarse ambiguous true-bridge control faces an enclosing-drill
vertex toward the strip edge (-90 degrees); the original supplied cut remains
at -87.5 degrees. The existing topology test additionally retains an uncertain
near contact across the full drill/boolean error allowance.

Final source code uses the original curve sampling phase. An intermediate
half-step phase produced many sub-tolerance annulus fragments on captured
boards; it was removed before final verification. Rust/corpus and native checks
were rerun after that change and after aligning the broad-phase error bound.

Verification commands:

```powershell
$env:PCB_ANALYZE_RUST_REPORT_DIR = "$PWD/debugging/copper-geometry/rust"
cargo test --release --locked --manifest-path native/analyze/Cargo.toml -- --nocapture
npm run native:build
npm run typecheck
npm run build
$env:PCB_BLOCK_BACKEND = 'cpu'
$env:PCB_BOARD_BACKEND = 'cpu'
$env:PCB_POST_PLACE_BACKEND = 'cpu'
node --import tsx --test --test-concurrency=1 tests/pcb-native-addon.test.ts tests/pcb-analysis-geometry.test.ts tests/pcb-analysis-runtime.test.ts tests/pcb-board-packer-native.test.ts tests/public-api.test.ts
node --test scripts/test-pcb-analyze.test.mjs
npm run test:package
node debugging/copper-geometry/compatibility.mjs
# Invokes test-pcb-analyze --all --name geometry once, with external sampling:
& ./debugging/copper-geometry/measure-launcher.ps1
node scripts/check-pcb-analysis-geometry.mjs
cargo fmt --manifest-path native/analyze/Cargo.toml --check
git diff --check
```

This revision does not change
RoutingBoardV2, exporters, fixture captures, placement algorithms or numeric
contracts; no following analysis stage is implemented.


### Results of the single-geometry revision

- **34 Rust tests pass**: 21 geometry and 13 import, including all eight saved
  boards. The supplied P1 regression remains unchanged and passes.
- **43 Node tests pass**, zero skipped: strict addon exports/declarations,
  real geometry/worker parity, cancellation/recovery, runtime and legacy packer
  and public APIs. Removed arrays/counters are explicitly asserted absent.
- Fresh production native build, typecheck, build and **4 launcher tests** pass.
  Saved input hashes/native inventories are unchanged.
- Installed-package checks pass in all three configurations: ordinary, GPU
  disabled and unavailable GPU drivers. Geometry/import and legacy workers,
  assets and declarations load from the installed archive.
- Final launcher: **8 completed, 0 failed**. Full Node worker reports equal
  standalone Rust reports on all eight boards, including geometry coordinates,
  diagnostics, counts and graph references. Independent native pad/track,
  through-via and circular fill-void checks pass on BandAmpV5 and ESPower.
- All **43 previous exports and versions** remain, plus the existing
  `prepareAnalysisGeometry` export. Eight previous full import reports are
  unchanged. Short CPU replay solution and quality match exactly, before and
  after geometry: hardCount=0, hardSeverity=0, score=772.9915771484375,
  numericContract=f32-rte-ftz-v1.

| Regression | Status | A/B joined | Shorts | Diagnostic |
| --- | --- | --- | ---: | --- |
| Ø2 / 1.999 strip, e=0.001 | partial | No | 0 | drill_topology_uncertain |
| Same cut, e=0.0001 | complete | No | 0 | None |
| True 2.001 strip, e=0.001, vertex-facing control | partial | No | 0 | drill_topology_uncertain |
| Same true strip, e=0.00001 | complete | Yes | 1 | None |
| Generated vertex budget=100 | invalid | No | 0 | drill_geometry_unresolved |

No post-graph short suppression is used. Partial/invalid results and separated
islands do not assert an open. Regression JSON is under
[regression/](../../../debugging/copper-geometry/regression/).

### Measured corpus comparison

Before means the removed three-set implementation; after means the current
single set. Time measures worker preparation plus report serialization/IPC;
it excludes subsequent pretty-JSON formatting/writing and SVG generation.
Peak working set uses the existing external 100 ms Windows sampler, with one
uncontended final launcher pass. These are observations, not hard resource limits.

| Fixture | Worker+IPC seconds, before → after | Pretty report MiB, before → after | Worker peak MiB, before → after | Regions / islands now |
| --- | ---: | ---: | ---: | ---: |
| BandAmpV5 | 2.986 → 0.631 | 11.50 → 3.93 | 98.1 → 74.4 | 319 / 6 |
| ESPower | 11.928 → 2.533 | 15.13 → 5.76 | 123.5 → 85.2 | 668 / 56 |
| NanoScope | 129.970 → 25.335 | 94.68 → 35.26 | 449.2 → 224.1 | 3019 / 175 |
| NanoSpec | 58.238 → 12.961 | 69.48 → 24.49 | 299.8 → 173.2 | 2509 / 160 |
| STM32G0_CAN_RS485 | 2.959 → 1.003 | 17.15 → 6.13 | 125.7 → 84.0 | 844 / 65 |
| TPS63060DSCR | 0.707 → 0.323 | 4.29 → 1.50 | 72.5 → 72.7 | 170 / 8 |
| ThunderF722 | 35.676 → 9.935 | 66.48 → 25.52 | 289.8 → 194.8 | 1895 / 116 |
| USB_PD_Trigger_5A | 3.024 → 0.882 | 11.81 → 4.06 | 108.5 → 73.9 | 388 / 20 |

All eight remain partial, with zero excluded sources and zero confirmed shorts;
this is not a short-free certificate. Source inventories/provenance and CAD
fill states match the preserved baseline exactly. Prepared polygons/areas are
**not identical**: circumscribed drill cuts replace inscribed ones. The checker
bounds per-layer area differences by drill perimeter × boundary allowance plus
boolean rounding allowance; it does not require the old flawed drill contours.

BandAmpV5 loses two sub-tolerance pad-ring fragments. STM32G0 loses eight thin
pad fragments and splits three tracks. ThunderF722 loses one drilled track
fragment. Each affected source has a local drill-topology diagnostic, including
features removed entirely. No small-feature filter or source modification is
used. ESPower/NanoScope/NanoSpec/TPS/USB retain nominal region counts; that alone
does not prove topology. Complete details and area changes are in comparison.json.

- [Machine evidence](../../../debugging/copper-geometry/evidence.json)
- [Performance / geometry comparison](../../../debugging/copper-geometry/comparison.json)
- [Legacy compatibility and CPU replay](../../../debugging/copper-geometry/compatibility.json)
- [Current revision diff](../../../debugging/copper-geometry/single-geometry.patch), relative to HEAD `94d853e`
- [Rust log](../../../debugging/copper-geometry/rust-tests.log), [Node log](../../../debugging/copper-geometry/node-tests.log), [package log](../../../debugging/copper-geometry/package.log)
- [Full parity / independent checks](../../../debugging/copper-geometry/artifact-check.log), [launcher log](../../../debugging/copper-geometry/launcher.log)

Current reports, geometry JSON and per-layer SVGs:

- BandAmpV5: [report](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-22T08-02-05-400Z-geometry-PmtIIW/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-22T08-02-05-400Z-geometry-PmtIIW/).
- ESPower: [report](../../../debugging/pcb-analyze/ESPower/runs/2026-10-22T08-02-06-418Z-geometry-mQWtYT/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/ESPower/runs/2026-10-22T08-02-06-418Z-geometry-mQWtYT/).
- NanoScope: [report](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-22T08-02-09-268Z-geometry-dmDRYO/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-22T08-02-09-268Z-geometry-dmDRYO/).
- NanoSpec: [report](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-22T08-02-35-163Z-geometry-m9hEe5/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-22T08-02-35-163Z-geometry-m9hEe5/).
- STM32G0_CAN_RS485: [report](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-22T08-02-48-590Z-geometry-PvYKIt/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-22T08-02-48-590Z-geometry-PvYKIt/).
- TPS63060DSCR: [report](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-22T08-02-49-903Z-geometry-BWTbFs/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-22T08-02-49-903Z-geometry-BWTbFs/).
- ThunderF722: [report](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-22T08-02-50-509Z-geometry-frcIge/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-22T08-02-50-509Z-geometry-frcIge/).
- USB_PD_Trigger_5A: [report](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-22T08-03-00-923Z-geometry-HUDE3N/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-22T08-03-00-923Z-geometry-HUDE3N/).

Verified on Windows x64 / Node 26.5.0. Linux/macOS, other Node versions, remote
CI, full npm test and GPU numerical benchmarks were not rerun. Analyze remains
F64 and does not request GPU initialization. Finite approximation, local
component-count checks, missing/stale CAD data and capture discrepancies remain
limits, as described above. No mesh, solver, refill, DSL or next stage is added.

### Validator disposition after the memory tradeoff

2026-10-22: accepted for continued development by source/diff inspection only, at the user's explicit request not to run tests or builds. The commands/results above are the implementation author's record, not independently rerun verification of this revision.

The user accepts finite-tolerance connectivity limitations instead of retaining whole-board lower/upper envelopes. The current code still contains the cheaper enclosing-drill mitigation and the original regression; it is not a complete rollback to inscribed drilling. No claim is made here that the regression was rerun or that the remaining cases are statistically rare: their frequency has not been measured.

Local component-count comparison is not a general proof of topology preservation: changes in feature/contact identity can occur without a change in component count. Near-tolerance bridges, gaps and contacts can therefore affect connectivity and future DC results without a dedicated warning in every case. `complete` must not be interpreted as such a proof. This is an accepted current limitation, not a requirement to restore the removed global envelopes. Subsequent DC validation must include geometry/mesh refinement on its supported cases.
