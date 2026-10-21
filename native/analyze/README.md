# PCB analyze: board import

Standalone Rust library (`pcb-analyze`, ordinary `rlib`) with no Node addon,
packer dependency or GPU initialization. The shared
[Node host](../node-addon/README.md) calls this importer through the internal
`importAnalysisBoard(json)` binding. This library implements
only the RoutingBoardV2 input boundary. It does not complete P3/P4 of the broader
analysis plan.

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
placement/boolean operations belong to a later preparation stage.

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

There is no meshing, refill, solver, cache, GPU, DSL or common compute runtime.

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
