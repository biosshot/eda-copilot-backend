# PCB analyze: import, copper, conduction, DC and explicit RLC

Standalone Rust library (`pcb-analyze`, ordinary `rlib`) with no Node addon,
packer dependency or GPU initialization. The shared
[Node host](../node-addon/README.md) calls this importer through the internal
`importAnalysisBoard(json)` and `prepareAnalysisGeometry(json, options?)` bindings.
`prepareAnalysisConduction(json, options?)` prepares a layered mesh through the same host.

Production steady DC is implemented in `src/dc/` using that mesh and pinned
`faer` 0.24.4 sparse QR (CPU/F64, std+sparse-linalg, no mandatory BLAS installation).
The addon `AnalysisDcSession` owns at most one preparation/factorization; `clear()`
releases it deterministically. No Node dependency is added to this Rust library.
The public entry remains `runAnalysisDsl`, with explicit source/load/material
adapters. [DC verification](../../docs/experimental/analyze/dc-analysis-verification.md)
records numerical tests and eight converged real-board scenarios. F64 refinement
retains sub-ULP corrections for stable small voltage differences on a nonzero rail.
The importer retains its original semantics. `geometry::prepare` consumes its
validated `model::Board`; `geometry::prepare_json` is a boundary convenience.
The separate serializable geometry report does not expose the internal Board.
`regions` is the single prepared copper representation. `contacts`, `islands`
and findings reference its IDs; `layers` aggregates the same regions for display.
Drills use enclosing polygons with the reported curve/boolean error allowance,
so a faceted drill cannot leave copper inside the physical bore. Only a source
split or removed by drilling gets a temporary local comparison with inscribed
drills. A component-count difference is `drill_topology_uncertain`, with source
ID/path; exhausted local checks are `drill_topology_unresolved`. Such splits
are not confirmed opens. No alternative board snapshots or second graph are
retained. Invalid drill generation stops preparation instead of using an
unconfirmed hole. Finite approximation limits still apply.
This is a geometric part of P4, not completion of P4 or a physical solver.
See [geometry precision, tests and artifacts](../../docs/experimental/analyze/copper-geometry-verification.md).

## Conduction preparation

`conduction::prepare_json` accepts `{ geometry?, mesh?, profile? }` options.
`conduction::prepare(&Board, options)` and
`conduction::prepare_geometry(&Board, &PreparationReport, options)` avoid JSON round-trips.
The latter requires matching geometry options and geometry from the same board.
One layer union is triangulated once; overlaps do not multiply thickness.
Layer domains have independent node indices. Terminals retain pad fragments,
element membership and boundary subedges, including drilled annuli. Barrels retain
separate layer contacts and adjacent participating-layer segments, never ideal nodes.

Profiles contain ordered tagged copper/dielectric `stackup` entries, per-layer
`materials` (`conductivity_s_m`, explicit `state`), optional barrel material,
`plating_thickness_mm` and `diameter_convention` (`finished_hole` or `before_plating`).
Explicit stackup/plating override board values; no omitted material or gap is filled.
Sheet coefficients and barrel area/length/conductance are SI; mesh coordinates stay mm.
Plated slots retain planar copper/cutouts but have no supported barrel section.

Mesh options bound maximum area, requested minimum angle, total nodes/triangles and
refinement steps. Constraints are noded, divided to the area-derived boundary size,
then kept fixed during bounded refinement. Unachievable quality/budgets remain partial.
`mesh_complete`, `topology_confirmed`, `copper_complete`, `physics_complete` and
`dc_ready` are separate for every domain. Known drill/contact uncertainty and missing
fill freshness are not erased by successful meshing. There is no electrical solution
or electrical convergence claim. See [representation, checks and limits](../../docs/experimental/analyze/conduction-mesh-verification.md).

## API and data boundary

```rust
use pcb_analyze::import_json;

let report = import_json(json_text);
if let Some(board) = report.board {
    // Validated structural data, not yet a prepared physical model.
    println!("{} pads", board.pads.len());
}
for diagnostic in report.diagnostics {
    println!("{}: {}", diagnostic.path, diagnostic.message);
}
```

- `input::dto` mirrors the current `src/types/pcb/routing-board-v2.ts` transport.
  `import_board(dto)` accepts a Rust DTO; `import_json(&str)` also checks JSON
  structure. Unknown fields/variants, duplicate struct fields/layer-shape keys,
  explicit null and missing required fields are errors, never discarded/defaulted.
- `model` contains independent internal data, with layer/net/component/pad/group
  references resolved to vector indices. Fixed/editable ownership is retained.
  No internal type depends on a DTO or native EasyEDA object.
- `Source` retains the original ID and input path. Components also retain their
  designators; native fill fragment IDs remain attached to the corresponding
  layer fill. Paths identify objects whose contract allows an omitted ID.
- Validation precedes conversion. Any error returns `board: None` and zero
  accepted counts, with diagnostics; it never drops individual invalid objects.
  Missing physical data and limitations are reported separately and may coexist
  with an imported board. Import success is not physical-analysis readiness.

## Units and geometry

All coordinates and lengths stay F64 millimetres in the authored absolute Y-down
board frame; rotations stay in degrees. No origin shift, Y inversion, bottom-side
mirror, rounding or winding change is performed. Pad shapes, drill offsets and
drill rotations remain pad-local; component outlines remain absolute. Geometry
placement/boolean operations are performed separately by `geometry`, not import.

Layers must be listed in physical order with strictly increasing indices and
consistent top/inner/bottom sides. Explicit via endpoints must increase in that
order, with a through span covering the full board and blind/buried spanning a
proper subset. No reversed/absent span is repaired. A common pad shape remains
distinct from `shapesByLayer`, which must cover exactly the pad's layer set.
Independent-drill data preserves absent versus explicitly empty input.

Checks cover finite numbers, positive/nonnegative dimensions, ring vertex count
and nonzero signed area, slots/radii/via lands, unique identities, named references,
rule dimensions and references, matched endpoints/follow cycles, stackup copper
order/coverage and supplied thickness totals (relative/absolute 1e-9 mm tolerance
for floating point summation only). Existing copper is not rejected merely because
it violates a routing rule: this is an importer, not a DRC solver.

## Diagnostics and CAD fill

Diagnostics carry severity (`error`, `missing`, `limitation`), stable code, input
path, source ID when available, and message. `accepted` reports accepted object
counts, including available/unavailable per-layer fill states.

- `invalid_structure`, `non_finite`, `invalid_dimension`, `unknown_*`,
  `duplicate_identifier`, `invalid_via_span`, `incomplete_layer_shapes` and
  inconsistent fill/rule/stackup data reject the complete input.
- An `available` fill with zero polygons is retained as verified empty CAD copper.
  `missing`, `failed` and `unsupported` retain their reason and state, generating
  `cad_fill_unavailable`. Every zone layer needs exactly one explicit fill state.
- `fill_not_current` reports stale/unknown freshness without upgrading it to
  current or dropping the supplied polygons. Zone authoring boundaries are never
  substituted for actual copper. Refill rules and unresolved entries are retained;
  `incomplete_refill_rules` makes the reconstruction limitation explicit.
- Missing stackup/plating/dielectric data, independent-drill data, component body
  sizes and source IDs are reported where applicable. No FR4, copper thickness,
  conductivity, component model or analysis boundary condition is invented.
- `geometry_not_prepared` explicitly leaves intersections/containment, contacts,
  shorts and copper overlaps uncertified. Import does not certify ring topology or
  CAD export fidelity. Mask openings are not present in the current transport.

There is no refill, electrical solver, cache, GPU, DSL or common compute runtime.

## Build and checks

From the Backend repository root:

```bash
cargo build --locked --manifest-path native/analyze/Cargo.toml
cargo test --locked --manifest-path native/analyze/Cargo.toml -- --nocapture
cargo clippy --locked --manifest-path native/analyze/Cargo.toml --all-targets -- -D warnings
```

The integration test reads all eight saved boards under `tests/pcb-analyze`, checks
their accepted inventories, coordinates, IDs, pad drills and resolved references.
Small cases cover bottom pads/slots, per-layer shapes, through/blind/buried vias,
all fill states including empty copper, missing data and malformed inputs.
No per-fixture analysis `.ts` is needed or created.

For an individual JSON diagnostic report (one JSON object per input file):

```bash
cargo run --locked --manifest-path native/analyze/Cargo.toml --example import -- tests/pcb-analyze/NanoScope/NanoScope.json
```

The example accepts multiple filenames and exits nonzero on any rejected input
or read error. It is a development reader; it does not alter the existing named
analysis launcher or write into fixtures. Generated reports belong in ignored
`debugging/`, not in the source test bank.

## Explicit electrical network

`electrical::Session` compiles explicitly given R/C/L/independent-source networks
into sparse G/E, using faer CPU/F64 sparse LU for DC, real-block complex driving-point
impedance and adaptive BE time integration. One bounded compiled model/factor/latest
state per session; no PCB extraction, placement/GPU or global cache. N-API remains
in the single node-addon host. See [semantics](../../docs/experimental/analyze/electrical-network-semantics.md)
and [verification](../../docs/experimental/analyze/electrical-network-verification.md).
