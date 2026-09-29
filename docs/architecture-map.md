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
| Shared contracts and runtime | [`src/types/`](../src/types/), [`src/runtime/`](../src/runtime/) | Circuit/PCB types and packaged resource loading. Update callers and native encoders when a contract changes. |
| Verification | [`tests/`](../tests/), [`tests/pcb-layout/`](../tests/pcb-layout/), [`scripts/`](../scripts/) | Focused tests, example boards, capture/replay, profiling, and report generation. Use `npm run test:pcb-layout -- --list` for fixture names. |

PCB flow: DSL and circuit → resolved placement input → placement graph and block search → board packing and refinement → report, SVG, and assembly JSON. The exact Rust inputs are encoded in [`src/pcb-layout/pcb-auto-place-v2/native/`](../src/pcb-layout/pcb-auto-place-v2/native/). Search there and in existing geometry/scoring modules before introducing a second representation.

Local generated data belongs in ignored `debugging/`. Reviewed PCB experiment records and selected evidence live under [`docs/experimental/pcb/`](experimental/pcb/). The [changelog](../CHANGELOG.md) describes the current behavior; historical experiment reports describe how particular hypotheses were evaluated.
