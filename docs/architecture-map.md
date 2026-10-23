# Architecture map

Start with the existing entry point or helper before adding a new implementation. The repository packages component lookup, schematic layout, and PCB placement; the EasyEDA extension and copper router live in sibling repositories.

| Area | Start here | Responsibility |
| --- | --- | --- |
| Public API | [`src/index.ts`](../src/index.ts), [`src/components.ts`](../src/components.ts), [`src/schematic.ts`](../src/schematic.ts), [`src/pcb.ts`](../src/pcb.ts) | Published package entry points and host-facing calls. |
| Devices | [`src/devices/`](../src/devices/), [`src/devices/footprints/easyeda-footprint.ts`](../src/devices/footprints/easyeda-footprint.ts) | EasyEDA parts, symbols, footprints, and geometry import. |
| Circuit layout | [`src/circuit-layout/index.ts`](../src/circuit-layout/index.ts), [`patterns/`](../src/circuit-layout/patterns/), [`refinement/`](../src/circuit-layout/refinement/) | Schematic placement, pattern recognition, wiring, and refinement. See [module instructions](../src/circuit-layout/AGENTS.md) and [pattern guide](schematic-patterns.md). |
| PCB layout entry | [`src/pcb-layout/run-pcb-layout.ts`](../src/pcb-layout/run-pcb-layout.ts), [`pcb-layout-dsl/spec.ts`](../src/pcb-layout/pcb-layout-dsl/spec.ts), [`placement-input.ts`](../src/pcb-layout/placement-input.ts) | Parse and validate the DSL, resolve footprints, compile placement input, and assemble results. |
| PCB search | [`pcb-auto-place/auto-place.ts`](../src/pcb-layout/pcb-auto-place/auto-place.ts), [`pcb-auto-place-v2/tree-solver.ts`](../src/pcb-layout/pcb-auto-place-v2/tree-solver.ts), [`block-solver-engine.ts`](../src/pcb-layout/pcb-auto-place-v2/block-solver-engine.ts), [`board-packer-engine.ts`](../src/pcb-layout/pcb-auto-place-v2/board-packer-engine.ts) | Placement graph, block hypotheses, board packing, and final refinement. Check [PCB instructions](../src/pcb-layout/AGENTS.md) before changing scoring or concurrency. |
| Native solver | [`native/pcb-board-packer/src/`](../native/pcb-board-packer/src/), [`native/contract.ts`](../src/pcb-layout/pcb-auto-place-v2/native/contract.ts) | Rust block/board solvers, geometric and route-cost evaluation, and the TypeScript/native contract. See [native instructions](../native/pcb-board-packer/AGENTS.md) and [route-cost guide](pcb-route-cost.md). |
| Native Node boundary | [`native/node-addon/`](../native/node-addon/README.md), [`native/analyze/`](../native/analyze/README.md) | One N-API host owns the loader/binary and links the independent placement and F64 board-import libraries; module-owned declarations are aggregated by the host. |
| Analysis DSL | [`src/analyze/run-analysis-dsl.ts`](../src/analyze/run-analysis-dsl.ts), [`analysis-dsl/`](../src/analyze/analysis-dsl/spec-doc.d.ts) | Public JS runner, finite schemas/immutable builders, references, structural preparation, reports and explicit-value sweep. Production DC, explicit RLC/frequency networks and ideal cross-section impedance use the existing process/addon; other solvers/search remain unavailable. Scenario exit/abort owns native lifetime. |
| Analysis execution | [`src/analyze/`](../src/analyze/README.md), [`native/analyze/src/geometry/`](../native/analyze/src/geometry/), [`conduction/`](../native/analyze/src/conduction/), [`dc/`](../native/analyze/src/dc/) | Existing process family imports/prepares F64 copper and meshes or solves DC with finite contacts/barrels and sparse faer QR. No ideal net merging or readiness bypass. A scenario-owned single preparation/factorization reuses RHS; independent jobs clear it. [Eight-board DC coverage](experimental/analyze/dc-fixes-verification.md) is preserved. Explicit circuit DC/Z(f)/adaptive time uses the independent [`electrical/`](../native/analyze/src/electrical/) MNA module; see [P6a semantics](experimental/analyze/electrical-network-semantics.md). |
| Shared contracts and runtime | [`src/types/`](../src/types/), [`src/runtime/`](../src/runtime/) | Circuit/PCB types and packaged resource loading. Update callers and native encoders when a contract changes. |
| Verification | [`tests/`](../tests/), [`tests/pcb-layout/`](../tests/pcb-layout/), [`scripts/`](../scripts/) | Focused tests, example boards, capture/replay, profiling, and report generation. Use `npm run test:pcb-layout -- --list` for fixture names. |

PCB flow: DSL and circuit → resolved placement input → placement graph and block search → board packing and refinement → report, SVG, and assembly JSON. The exact Rust inputs are encoded in [`src/pcb-layout/pcb-auto-place-v2/native/`](../src/pcb-layout/pcb-auto-place-v2/native/). Search there and in existing geometry/scoring modules before introducing a second representation.

Local generated data belongs in ignored `debugging/`. Reviewed PCB experiment records and selected evidence live under [`docs/experimental/pcb/`](experimental/pcb/). The [changelog](../CHANGELOG.md) describes the current behavior; historical experiment reports describe how particular hypotheses were evaluated.

Analysis frequency blocks: `src/analyze/analysis-dsl/frequency-model.ts` validates
the shared Resource factory/direct path; `frequency-executor.ts` flattens instances
and adapts results. `native/analyze/src/electrical/frequency/` owns the sole Touchstone
parser, power-wave math and common-return composition. Existing MNA/FEM algorithms
remain separate; one node-addon, existing disposable worker, CPU/F64.


Line cross sections: `cross-section-executor.ts` adapts the existing impedance
builder to `native/analyze/src/extraction/{geometry,field,losses,propagation}.rs` through
`AnalysisElectricalSession.solveCrossSection`. Geometry is in SI, the FEM mesh
covers dielectric, C/C0/external L feed modal distributed propagation once per
section; resulting NetworkResource uses the unchanged P6b composition engine.
See [P7a semantics](experimental/analyze/line-cross-sections-semantics.md).
Optional P7b losses recover the vacuum unit-current basis from FE boundary reactions,
integrate each physical surface and each dielectric region once per field mesh,
then reuse those integrals across frequency. Complex passive modal propagation
produces the existing NetworkResource; no additional runtime/addon/network kind.
See [loss conventions and pending acceptance](experimental/analyze/line-losses-semantics.md).
