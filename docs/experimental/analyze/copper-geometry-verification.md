# Copper geometry acceptance evidence

2026-10-21. Review found R1 (false intra-source neck) and R2 (missing strict
addon export expectation). Initial measurements below are historical and are
**not evidence of topology preservation**. The correction record at the end
supersedes the initial connectivity/numeric claims. Awaiting repeat review.

2026-10-21. Implements `next-task-copper-geometry.md`; awaiting review, no commit.
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

### Two-sided connectivity policy (R1 correction)

Let P be an uncut nominal source, D an inscribed nominal drill, M the supplied
board material, e the source/drill sagitta, and g = 2 * boolean_error_bound_mm.
Generated round primitives satisfy P contained in true source contained in
P expanded by e; CAD polygons are taken as supplied, without certifying native
arc tessellation. Envelopes are formed **before** clipping/subtraction:

- reliable copper: shrink(P, g) intersect shrink(M, g) minus union(expand(D, e + g));
- possible copper: expand(P, source_e + g) intersect expand(M, g) minus union(shrink(D, g)).

Exact straight sources wholly inside material and without nearby drills retain
their original boundaries. All other operands have a rounding guard. Offsets
reuse pinned geo/i_overlay, with Round(pi/4) joins and twice the requested distance.
The pinned join implementation floors segment counts: actual arc steps can
approach pi/2. The factor 2 still covers the requested disk because
2 cos(pi/4) > 1, for expanded holes and shrunken copper at concave corners.
This guard is deliberately conservative; it is not a tighter nominal tolerance.

Every connected polygon of reliable copper becomes a graph node. An internal
nominal neck can therefore split into several reliable nodes. The upper graph
joins potential overlaps, near/point contacts and possible barrel associations;
its edges are always `reliable:false`. Comparing upper components against lower
components reports zero/multiple lower components as `topology_uncertain`, with
source IDs/paths. No short finding is deleted after graph construction.
`net_islands` lists separate **reliable** components; it does not prove an open.
A genuine narrow bridge remains possible until precision resolves it. A feature
erased from nominal copper can still survive in the upper bound, because bounds
are not made by buffering that already-cut snapshot.

`regions`, `holes`, `layers` retain nominal snapshots/areas. `reliable_regions`
are referenced by reliable `contacts`/`islands`/findings; `possible_regions` are
referenced by `possible_contacts`/`possible_islands`. Namespace prefixes and
source path + layer distinguish these representations; fragment correspondence
need not be one-to-one. SVGs show gold nominal, green reliable, purple possible
copper; red highlights reliable shorts.

Contacts still distinguish area, line, point and uncertain overlap. Original
uncut predicates prevent a rounded positive source gap from becoming reliable.
Shared straight edges need an original shared edge. Plating remains a surface,
not a filled hole: a witness patch must be strictly inside the uncut source,
clear of board/other holes, and close to its surviving reliable fragment within
the envelope guard. Round/slot drills are convex; that clear local patch can
reach the true barrel without crossing another void. No net filtering is used.

Offset workspace is checked before construction (12 input vertices per ring
vertex at the fixed join setting); actual offset vertices and both retained
boolean envelopes count against `max_generated_vertices`. Overflow/invalid
bounds give source-specific invalid diagnostics, without nominal fallback or
tolerance relaxation. A degenerate generated envelope permits at most six
local guard attempts (g, 2g, 4g, 8g, 16g, 32g), consuming the same budget. A successful wider
guard is reported as `envelope_guard_widened`; this widens uncertainty, never
changes nominal sagitta or repairs captured geometry. Boolean intermediate allocation/CAD input volume still
have no hard RSS quota; process deadline/cancellation remain the isolation bound.
No global ultra-fine tessellation, adaptive curve refinement, new dependency, solver or
GPU work is introduced. Tightening e cannot resolve features below the separate
boolean guard or certify missing/stale CAD data.

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
- Sagitta refinement, vertex-limit rejection, the straight-vertex budget and extreme finite coordinates are asserted by coupons. No tolerance increase, small island removal, board source repair or boundary-as-fill fallback is performed. Primitive/envelope retained vertices are bounded; CAD input volume, transient boolean allocation and OS memory are not separately quota-limited. Existing process deadline/cancellation remain the isolation boundary.
- Full `npm test`/full placement benchmarks, real GPU numerical replay, Linux/macOS builds and Node 20/24 were **not run locally**. Existing CI already runs locked Rust analyze tests, package build/tests and installed smoke across those target OS/Node matrices; no remote CI result is claimed.
- `complete` only certifies preparation of confirmed geometric input; it is not DC readiness, FEM mesh validity, full DRC or proof of CAD agreement. Unknown/stale fills, unresolved spans, error-band contacts, native capture precision and fragment identity limitations remain explicit. No mesh, DC/thermal/PI/SI/EM solver, refill, materials, DSL, GPU analysis, cache, sessions, compute-runtime or placement algorithm change was introduced.

**Stopped for review. No commit and no following stage.**

## Review correction verification (2026-10-22)

R1 and R2 are corrected in this uncommitted stage. This record supersedes the initial pre-review topology claims. Repeat acceptance remains with the reviewer.

The supplied drill_approximation_cannot_create_a_reliable_path_inside_one_source test retains its dimensions, angle, tolerances and assertions. Reliable graph nodes now come from lower copper envelopes, not nominal source polygons. Upper envelopes retain uncertain paths and features absent from the nominal snapshot. The addon test adds exactly prepareAnalysisGeometry and retains strict runtime/declaration/version equality.

Additional controls cover round/slot cuts, rotations/translations, off-centre holes near copper edges, a robust true bridge/short, a real narrow bridge resolved by refinement, budget exhaustion, and refined true bridges on board half-extents 5/10/20/100 mm. At most six local guard attempts handle offset degeneracy with unchanged sagitta and the same finite vertex budget.

### Commands and results

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
# One final launcher pass, executed by the external sampler below:
npm run test:pcb-analyze -- --all --name geometry
# Actual measured invocation (the sampler invokes the launcher once):
& ./debugging/copper-geometry/measure-launcher.ps1
node scripts/check-pcb-analysis-geometry.mjs
cargo fmt --manifest-path native/analyze/Cargo.toml --check
git diff --check
```

- Rust: **34 passed** (21 geometry + 13 import). All eight preparations assert partial and no nominal source exclusions.
- Fresh production addon, typecheck and build pass. **43/43 joint Node tests**, zero skipped, including the real addon/process-worker regression and strict addon export test.
- Launcher: **4/4 tests** including saved-source hashes/native inventories; final geometry launch: **8 completed, 0 failed**.
- Installed-package smoke passes normally, with GPU disabled, and with GPU drivers unavailable. This includes geometry, old APIs, declarations, workers and assets. Analyze requests no GPU initialization.
- **8/8 full standalone Rust / Node worker preparation reports match**: all diagnostics, counts, topology and coordinates. Independent native pad/track, through-via and fill-hole checks pass on BandAmpV5 and ESPower.
- Old 43 exports and all versions are retained, plus only prepareAnalysisGeometry. Eight old full import reports are identical to the pre-geometry baseline. Short CPU solution and quality replay match exactly, also across geometry execution: hardCount=0, hardSeverity=0, score=772.9915771484375; numericContract=f32-rte-ftz-v1.

Logs: [Rust](../../../debugging/copper-geometry/rust-tests.log), [native](../../../debugging/copper-geometry/native-build.log), [Node](../../../debugging/copper-geometry/node-tests.log), [package](../../../debugging/copper-geometry/package.log), [compatibility](../../../debugging/copper-geometry/compatibility.json), [launcher](../../../debugging/copper-geometry/launcher.log).

### Regression outputs

| Case | Status | Reliable A/B | Possible A/B | Confirmed shorts |
| --- | --- | --- | --- | ---: |
| Cut e=0.001 | partial | False | True | 0 |
| Cut e=0.0001 | complete | False | False | 0 |
| Real narrow bridge e=0.001 | partial | False | True | 0 |
| Real narrow bridge e=0.00001 | partial | True | True | 1 |
| Vertex budget=100 | invalid | False | False | 0 |

The cut never produces a reliable A/B path. The real narrow bridge remains possible at coarse precision and becomes a confirmed short after refinement. No findings are removed after graph construction. A budget-limited invalid report is not preparation success.

### Preserved baseline and corpus comparison

All eight source inventories, nominal source polygons and per-layer union areas match the original baseline **exactly**. The original baseline remains under corrections-baseline. Source counts describe nominal inventory, not certified connectivity.

| Fixture | Nominal regions | Reliable / possible regions | Old islands -> reliable / possible | Uncertain possible components |
| --- | ---: | ---: | ---: | ---: |
| BandAmpV5 | 321 | 319 / 321 | 8 -> 6 / 8 | 2 |
| ESPower | 668 | 666 / 668 | 56 -> 54 / 54 | 0 |
| NanoScope | 3019 | 3066 / 3030 | 175 -> 222 / 175 | 19 |
| NanoSpec | 2509 | 2541 / 2509 | 160 -> 192 / 159 | 13 |
| STM32G0_CAN_RS485 | 849 | 844 / 849 | 73 -> 65 / 73 | 8 |
| TPS63060DSCR | 170 | 170 / 170 | 8 -> 8 / 8 | 0 |
| ThunderF722 | 1896 | 1898 / 1896 | 117 -> 117 / 116 | 1 |
| USB_PD_Trigger_5A | 388 | 388 / 388 | 20 -> 20 / 20 | 0 |

| Fixture | Reliable / uncertain contacts: old -> new | Worker+IPC seconds: old -> new | Pretty report MiB: old -> new | Observed worker peak MiB |
| --- | --- | ---: | ---: | ---: |
| BandAmpV5 | 729/1 -> 729/1 | 1.319 -> 2.986 | 3.98 -> 11.50 | 98.1 |
| ESPower | 990/5 -> 990/1 | 3.152 -> 11.928 | 5.74 -> 15.13 | 123.5 |
| NanoScope | 4616/15 -> 4616/115 | 34.356 -> 129.970 | 35.19 -> 94.68 | 449.2 |
| NanoSpec | 4355/14 -> 4355/85 | 18.891 -> 58.238 | 24.39 -> 69.48 | 299.8 |
| STM32G0_CAN_RS485 | 1166/18 -> 1172/17 | 1.322 -> 2.959 | 6.47 -> 17.15 | 125.7 |
| TPS63060DSCR | 413/0 -> 413/0 | 0.406 -> 0.707 | 1.50 -> 4.29 | 72.5 |
| ThunderF722 | 2761/8 -> 2765/8 | 13.326 -> 35.676 | 25.48 -> 66.48 | 289.8 |
| USB_PD_Trigger_5A | 796/4 -> 796/4 | 1.157 -> 3.024 | 4.07 -> 11.81 | 108.5 |

All eight remain partial, with zero nominal exclusions and zero confirmed shorts. These are not short-free certificates. Existing freshness/span limitations remain, with additional two-sided contact/topology uncertainty. Upper-only components have no reliable core; possible island counts can therefore exceed reliable counts. Conversely one possible component can contain multiple reliable islands: that is uncertainty, not a confirmed open.

BandAmpV5 pad 2f0f346e43eca4b3 has 0.0020091856 mm2 nominal remnants on TOP/BOTTOM without reliable cores at this tolerance; possible copper is retained. ESPower tracks d7f127a3ed86bbb1 and ebfb63c042cfe9e9 similarly have 0.0000482537 mm2 nominal remnants. NanoScope/NanoSpec gain separate lower fragments where necks are within the envelope. STM32G0_CAN_RS485 loses 8 nominal fragments from its reliable bound and gains 3 split fragments; its guarded barrel associations increase from 251 to 254. Such barrel associations still prove a clear uncut-source patch, rather than connecting through a filled hole centre. Per-source fragment changes and all before/after finding counts are saved in comparison.json.

Local widened guards were needed for captured fills e2412 (NanoScope), e8048 (NanoSpec) and e664 (ThunderF722); diagnostics include guards and source paths. Nominal snapshots are unchanged. No source repair, global tolerance reduction or nominal-graph fallback was used.

[Machine evidence](../../../debugging/copper-geometry/evidence.json), [before/after comparison](../../../debugging/copper-geometry/comparison.json), [preserved original baseline](../../../debugging/copper-geometry/corrections-baseline/evidence.json).

### Current artifacts

[Full uncommitted implementation diff](../../../debugging/copper-geometry/copper-geometry.patch) is relative to HEAD bf3869c and includes the supplied regression test unchanged; the user's review-note files are not included in the patch.

Rendered/read diagnostic views: [drill coupon](../../../debugging/copper-geometry/regression/cut-comparison.png), [zoomable coupon SVG](../../../debugging/copper-geometry/regression/cut-comparison.svg), [BandAmpV5 bounds](../../../debugging/copper-geometry/BandAmpV5-TOP-bounds.png), [ESPower bounds](../../../debugging/copper-geometry/ESPower-TOP-bounds.png). They supplement the numeric oracles; sub-tolerance necks are not certified from pixels.

- BandAmpV5: [report](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-22T07-04-09-212Z-geometry-y351Hr/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/BandAmpV5/runs/2026-10-22T07-04-09-212Z-geometry-y351Hr/).
- ESPower: [report](../../../debugging/pcb-analyze/ESPower/runs/2026-10-22T07-04-12-705Z-geometry-EkUZsu/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/ESPower/runs/2026-10-22T07-04-12-705Z-geometry-EkUZsu/).
- NanoScope: [report](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-22T07-04-25-118Z-geometry-NIAZfi/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/NanoScope/runs/2026-10-22T07-04-25-118Z-geometry-NIAZfi/).
- NanoSpec: [report](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-22T07-06-36-513Z-geometry-jgFM5d/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/NanoSpec/runs/2026-10-22T07-06-36-513Z-geometry-jgFM5d/).
- STM32G0_CAN_RS485: [report](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-22T07-07-35-736Z-geometry-bzoqmg/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/STM32G0_CAN_RS485/runs/2026-10-22T07-07-35-736Z-geometry-bzoqmg/).
- TPS63060DSCR: [report](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-22T07-07-39-134Z-geometry-vaNvqk/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/TPS63060DSCR/runs/2026-10-22T07-07-39-134Z-geometry-vaNvqk/).
- ThunderF722: [report](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-22T07-07-40-171Z-geometry-VcPkoa/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/ThunderF722/runs/2026-10-22T07-07-40-171Z-geometry-VcPkoa/).
- USB_PD_Trigger_5A: [report](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-22T07-08-16-787Z-geometry-tLy8XY/report.json), [geometry / SVG directory](../../../debugging/pcb-analyze/USB_PD_Trigger_5A/runs/2026-10-22T07-08-16-787Z-geometry-tLy8XY/).

### Remaining limits

- Envelopes, the second graph and serialization increase CPU time, memory and report size; the measured cost is shown above. Worker+IPC time excludes subsequent artifact formatting/writing. External 100 ms OS peak sampling is observational, not an RSS quota.
- Sagitta alone does not preserve topology. Bounds are conditional on the supplied CAD snapshot. Degenerate offsets or exhausted budgets can still return invalid, with no nominal-connectivity fallback. CAD input volume and transient boolean allocation have no hard RAM quota.
- Existing capture discrepancies in via dimensions/native fill fragment identity, unknown or stale fills and unconfirmed spans remain. Tightening tolerance cannot certify CAD equivalence.
- Verified locally on Windows x64 / Node v26.5.0. Linux/macOS, Node 20/24, remote CI, full npm test and real GPU numerical benchmarks were not run for this correction. No mesh, solver, refill, DSL, GPU analysis, cache, board format/exporter or placement algorithm change is included.

**Stopped for repeat review. No commit and no following stage.**
