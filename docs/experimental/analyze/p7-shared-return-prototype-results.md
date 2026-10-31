# Shared return prototype — bounded DC falsification

2026-10-31. Decision: **NO-GO for EM integration**. P7 remains open. This report and example are experimental; production sources, DSL, dependencies and existing changes are preserved.

## Formulation fixed before implementation

One deliberately restricted formulation is implemented: a finite resistive conductor graph, the DC limit of charge-conserving conduction. Nodes have potentials v [V], edges have current i [A], supplied positive resistance R [ohm], incidence A, and ports B with +1/-1 on their actual terminal nodes. Equations are i=R^-1 A^T v and K v=B j, K=A R^-1 A^T [S]. Each physically connected component has exactly one mathematical gauge removed. Port voltages are B^T v and Z=B^T K_g^-1 B [ohm]. No component is connected to another by a gauge. Ports crossing disconnected components are refused; insulating omissions remain omissions. Contacts are ideal finite terminal nodes specified by the input graph, rather than inferred net names.

After gauging, K_g is SPD because positive edge energy vanishes only on component constants. Use the existing faer dense LLT, not complex CG. Power j^T Z j equals sum R_e i_e^2 >=0; reciprocity follows from K_g symmetry. Shared returns are actual graph edges, counted once, including local return-via edges and plane bridges. Multiple ports use one K and retain off-diagonal common-impedance coupling. Edge currents recover the finite return distribution. Input geometry coordinates are visualization metadata: this executable does not extract resistance from arbitrary polygons.

Internal nodes can be eliminated by the Schur complement K_TT-K_TI K_II^-1 K_IT; reported Z already eliminates internal nodes through solves. The executable does not materialize this Schur matrix or claim extraction memory savings from its smaller output. Artificial series subdivisions preserving R should leave Z invariant. This is a network subdivision control, not spatial convergence of a PCB EM mesh.

The supported band is exactly DC (0 Hz). L/C/G, delay, propagation, skin/proximity effects, inductive/capacitive crosstalk and radiation are **unavailable**, not assigned zero. Resistance inputs require independently justified conductor approximations. A slot reroutes DC only through retained edges; it does not demonstrate AC return detours. Wide plane extension is allowed, while breaks cannot be discarded.

## Why the requested EM hybrid cannot yet be justified

The existing section matrices include finite reference conductors and their self/mutual energies. Existing local extraction supplies compact electrode ports; P6 identifies potentials and conserves currents. Neither identification nor a new resistor plane supplies the distributed lateral plane trace degrees of freedom needed where the same plane enters multiple section corridors. Adding a common plane operator to already extracted section/local self energies duplicates storage/loss unless an explicit disjoint energy ownership and calibrated boundary trace map is constructed. Omitting that operator loses nonlocal return transfer and mutual fields. A valid implementation needs a shared conductor trace basis, transfer of flux/current to section and local contacts, and subtraction-free partition of magnetic/electric energy (or a single global integral operator). Public APIs alone do not implement this mechanism. No production-core relocation is authorized here.

Primary sources checked: [Cadence C&S paper](https://www.cadence.com/content/dam/cadence-www/global/en_US/documents/tools/ic-package-design-analysis/sigrity-resources/sigrity-modeling-structure-high-speed-long-tansmission-lines-methodology-cp.pdf), pp.1–2, requires calibrated cutting ports and compensation; its speedups do not validate our interfaces. [MIT FastHenry manual](https://www.fastfieldsolvers.com/Download/FastHenry_User_Guide.pdf), introduction and discretization sections, describes a global filament self/mutual RL operator with GMRES/FMM. It illustrates the missing nonlocal magnetic mechanism; no FastHenry implementation or frequency result is claimed here.

## Audited existing code and concrete obstruction

Read the repository rules, CHANGELOG, implementation plan, DSL working contract/reference, memory audit and both route/oriented result reports before editing. Relevant implementations are `extraction/polygon_section.rs` (finite transverse section), `extraction/local/board_prepare.rs`, `volume_reduction.rs` and `volume_solve.rs` (local contact basis and energy operators), `electrical/coupled/{mod,composition}.rs` (passive descriptor/conservation). Existing `dc/system.rs` is private and board-specific; the experimental graph adapter therefore uses the already pinned faer LLT rather than copying a linear solver or importing an additional dependency. Existing `cross-section` example remains the section transport tool.

`src/analyze/run-corridors.ts:151` removes intervals intersecting lateral continuations; `board-channel-plan.ts:103` refuses the resulting channel. The saved STM32 audit in [route results](p7-route-partition-results.md) records fifteen runs, a bottom plane crossing track48 corridor sides over 3.2201 and 1.96191933 mm, and zero uniform intervals after whole-return widening. This evidence concerns an interface limitation, not an assertion that wide planes intrinsically prevent hybrid extraction. A graph cannot remove it while retaining the field model's claimed band.

## Resource boundary and interpretation

Maximum input: 8 MiB, 64 cases, 256 nodes, 4096 edges and 16 ports per case; positive R is bounded to [1e-12,1e12] ohm. Cases run serially with a 10-second between-case deadline. This is much smaller than the authorized 8 GiB/600-second ceiling. The independent harness must enforce process-tree/headroom/time limits because the executable's between-case timer is not asynchronous cancellation. No heavy solve is launched. Dense working-payload estimate is emitted explicitly and excludes allocator/library scratch. Full gauged nnz, full node count, gauged unknown count, p-by-p port result size and dense factor allocation entries are separate. There is no sparse factor/memory reduction claim. The port impedance is a terminal reduction, not a new extracted PCB operator.

The historical 1,411,442-unknown magnetic factor (784,635,174 stored entries; 6,277,081,392 value bytes) in [memory audit](p7-memory-architecture-audit.md) is **not comparable** to these supplied-resistance graphs: geometry, band, ports and observables differ. No ratio or projected speedup is valid. Real 2/4/6-layer EM runs are intentionally not advanced after the missing physical interface is identified; independent manifest graph cases are not mislabeled board fixtures. Existing original copper, DSL and debugging inputs remain intact.

## Executed independent controls

`cargo build --manifest-path native/analyze/Cargo.toml --example shared_return_prototype` passed (7.31 s, existing library warnings). The independent benchmark owner then ran the executable against `tests/fixtures/analysis-shared-return-prototype/manifest.json`; the solver never reads reference values. `node --import tsx scripts/verify-p7-shared-return-prototype.ts debugging/p7-shared-return-benchmark/prototype-output.json` compares supplied analytic references. Results: **8 DC passes, 0 failures, 24 EM metrics unavailable**. The absolute zero floor is 1e-12 ohm; main relative target is 8%. These references are exact ideal-network arithmetic, without a continuum-model uncertainty claim.

| Case / family | DC Z11 (mOhm) | Full / gauged nodes | gauged nnz / factor allocation entries | Payload estimate B | Result and physical limit |
| --- | ---: | ---: | ---: | ---: | --- |
| wide / wide plane | 0.49257143 | 2 / 1 | 1 / 1 | 88 | PASS full-width 1D resistor; no transverse field |
| edge / finite edge | 0.98514286 | 2 / 1 | 1 / 1 | 88 | PASS half-width resistor; no fringing |
| slot-detour / slot | 1.47771429 | 3 / 2 | 4 / 4 | 208 | PASS series DC detour; no inductive return prediction |
| two-planes / multiple references | 0.24628571 | 2 / 1 | 1 / 1 | 96 | PASS equal parallel split 0.5/0.5; no frequency redistribution |
| via-near / transition | 0.54182857 | 3 / 2 | 4 / 4 | 208 | PASS lumped return resistance; no via inductance |
| via-remote / transition | 1.52697143 | 3 / 2 | 4 / 4 | 208 | PASS longer resistive path; no spatial field evidence |
| shared-narrow / two lines | 2.95542857 | 4 / 3 | 7 / 9 | 536 | PASS shared return; Z12=2.46285714 mOhm |
| shared-split / artificial subdivision | 2.95542857 | 5 / 4 | 10 / 16 | 800 | PASS same port matrix within 2e-18 ohm |

Max reference absolute error: 1.1275702593849246e-17 ohm; maximum relative error approximately 7.4e-15. Maximum KCL error 7.771561172376096e-15 A per 1 A drive, relative power discrepancy 7.384357285845382e-15, reciprocity error 4.336808689942018e-19 ohm. These are separate numerical/reference checks, not physical error bounds for fabricated copper. No spatial refinement or band convergence has been established. Analytic network references are independent of this matrix implementation but do not constitute independent EM extraction.

Generated machine evidence is `debugging/p7-shared-return-benchmark/{prototype-output,verification,resources}.json`; the benchmark owner retains a portable observed result fixture. Its geometry/comparison SVG and PNG describe authored graph topology and DC transfer only. There is no propagation plot because no propagation result exists. Edge currents and port-matrix off-diagonals are emitted for honest return/common-impedance plots.

The independent benchmark suite reports **11 passing tests**, including actual executable refusals for disconnected return edges, negative resistance and duplicate node IDs. See [independent benchmark report](p7-shared-return-benchmark-results.md) for commands and the distinction between graph validation and absent EM evidence.

Independent supervisor observed approximately 130.8 ms wall including startup. Very short-lived process RSS/private samples (~4.1 MB/~0.56 MB) are **not reliable peaks** and must not be used for a memory acceptance claim. Per-case phase times and allocation estimates are in output JSON. No complete field/operator process-tree peak or comparable old/new memory benchmark exists.

## Decision

**GO only for retaining this DC topology falsification tool; NO-GO for integration of a shared-return EM extractor.** The requested coupled line + shared finite reference + local EM transition formulation is not validated in the authorized boundary. All six families have bounded network controls, but all their distributed EM behavior and the real 2/4/6-layer fixtures remain unaccepted. This is the explicit early no-go allowed by the task, not a completed hybrid or P7 closure. The next necessary mechanism is the distributed conductor trace/energy ownership interface described above, followed by matching independent continuum/field references and spatial/band/resource checks. P8/P11 readiness is not established. No commit was made.
