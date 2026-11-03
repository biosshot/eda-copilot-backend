# P7 shared-return benchmarks — 2026-10-31

Eight independently authored ideal DC network controls pass the bounded Rust graph prototype. This is **not acceptance of P7 or an EM shared-plane implementation**. The machine table retains 24 unavailable distributed-EM comparisons; no failed case was removed. The maximum observed absolute matrix error is below 3e-18 ohm. All matrices, ordered ports, input assumptions and sampled resources are saved in `tests/fixtures/analysis-shared-return-prototype/observed-dc-result.json`.

## Reproduce

From backend root (build the example first using the formulation report):

```powershell
Get-Content tests/fixtures/analysis-shared-return-prototype/manifest.json -Raw | native/analyze/target/debug/examples/shared_return_prototype.exe > debugging/p7-shared-return-benchmark/prototype-output.json
node --import tsx scripts/verify-p7-shared-return-prototype.ts debugging/p7-shared-return-benchmark/prototype-output.json
node --import tsx --test tests/analyze-shared-return-benchmark.test.ts
```

The harness without an output argument emits pending/unavailable rather than a fabricated pass. It writes verification JSON plus topology/comparison SVGs under ignored debugging. PNG versions were rendered with local matplotlib and visually inspected: the DC reference and prototype markers overlap; topology drawings explicitly state that they are schematic, not PCB geometry. SVG generation is part of the runnable harness and needs no added dependency.

## Predeclared scope and acceptance

`manifest.json` was delivered to formulation before the prototype run. It declares metre/ohm units, physical ordered differential ports, one ampere excitation with the other port currents zero, DC only, and tolerance `max(1e-12 ohm, .08*abs(reference))` for every matrix entry. The picohm floor handles exact zero mutual entries, not weak HF coupling. The 8% bound is the requested coarse primary target; it does not quantify PCB model discrepancy. No 15% guarantee is asserted. Analytic uncertainty is floating arithmetic of the ideal network; continuum/model uncertainty outside its assumptions is unbounded.

| Control | Independent oracle | Observed |
| --- | --- | --- |
| Wide plane | Full-width contacted 10x10 mm rectangle, 35 um thick, authored rho=1.724e-8 ohm m; R=rho L/A | Pass, 0.492571 mOhm |
| Finite edge | Same rectangle width halved; no spreading-current claim | Pass, 0.985143 mOhm |
| Slot detour | Ideal series path r+2r; not continuum slot geometry | Pass, 1.477714 mOhm |
| Multiple planes | Two equal resistors in parallel | Pass, 0.246286 mOhm; return split .5/.5 |
| Near/remote return | Series path r or 3r with authored lumped r/10 transition | Both pass; not physical via EM |
| Shared narrow return | Tree path overlap gives Z12=5r, Z11=6r, Z22=7r | Pass, Z12=2.462857 mOhm |
| Artificial subdivision | Replace branch r by r/2+r/2, preserve ports | Pass, matrices differ below 3e-18 ohm |

This is a different solution path from the prototype's dense Laplacian factorization: closed series/parallel and path-overlap arithmetic. It checks graph implementation, physical common resistance and port semantics. It does not independently check graph extraction from a sheet or shared L/C. Tests mutate units, ports, tolerance, nonfinite entries and off-diagonal coupling; a property-order regression test prevents treating JSON object key ordering as port identity. Eleven focused tests pass, including actual binary rejection of disconnected returns, negative resistances and duplicate node IDs (binary-dependent tests skip explicitly if the example is absent). Synthetic reference-fed output is explicitly labeled harness self-check, not solver evidence.

## Source and saved-reference coverage

Exact URLs and retrieval limitations are in `sources.json`. The [NASA resistance relation](https://www.grc.nasa.gov/www/k-12/airplane/ohms.html) supports only the rectangle controls. The [Ansys training](https://innovationspace.ansys.com/courses/wp-content/uploads/sites/5/2021/07/HFSS_3DL_GS_2023R1_EN_LE03_Meshing_Sol.pdf) describes volumetric FEM and layout meshing. The [Cadence cut-and-stitch paper](https://www.cadence.com/content/dam/cadence-www/global/en_US/documents/tools/ic-package-design-analysis/sigrity-resources/sigrity-modeling-structure-high-speed-long-tansmission-lines-methodology-cp.pdf) provides architecture context; the [Rosser et al measurement paper](https://www.cadence.com/content/dam/cadence-www/global/en_US/documents/tools/ic-package-design-analysis/sigrity-resources/sigrity-measurement-simulation-simultaneous-switching-noise-mutli-reference-plane-package-cp.pdf) demonstrates why distributed reference-plane dynamics matter. None supplies a matching machine-readable graph/EM oracle here. Siemens fetch failed; no numerical claim rests on it. Corrupt PDF dimension text was not silently reconstructed.

`saved-reference-index.json` records existing whole-volume identity/result and comparison packets with SHA256 plus a separate FastCap layered reference index. They are preserved, not regenerated or modified. The historical straight coupon has 1/3.162/10 kHz holdouts and reported delay refinement about .32%; it is not the same geometry or ports as these graph controls. Its reported comparison (S .000574507%, delay 4.994104%, loss .000711407%) is historical separate-path evidence, not a pass awarded to this prototype. FastCap is electrostatic evidence, not a return-current reference. Local debugging packets are explicitly nonportable.

Delay, S21, HF crosstalk and distributed return-current accuracy remain unavailable for all six families at 1/100/10k/1M/100M/1G Hz; this is recorded in each machine-table row. Required next evidence: fully specified continuum geometry/materials/ports and source reference, then converged volume or measured port matrices across low-band and HF holdouts. No matched continuum data was fabricated to fill these gaps.

## Real-board selection and resources

`board-selection.json` preserves original full-board paths, exact-byte hashes, export inventories and geometry keys for STM32G0_CAN_RS485 (2 layers), NanoScope (4), ThunderF722 (6). Counts come from captured export inventory, not names. STM32's existing full candidate is U5.7/U5.5 to J5.2/J5.3. NanoScope's existing PDN source is recorded; complete-channel endpoints remain unselected. Thunder's existing e216 example is explicitly an 8.0009 mm fragment with hypothetical reference stack, not a whole-channel substitute. These boards cannot be accepted through resistor edges without an ownership-preserving copper/port export. No full-board solve was launched.

The single small prototype process took 130.8021 ms including startup/supervision. Five-ms samples observed RSS 4,112,384 B and private bytes 557,056 B; fast exit makes these **sampled lower bounds**, not certified peak process-tree memory. It spawns no children. Per-case assembly/factor/solve timings, full/gauged unknown counts, nnz, factor entries and dense byte estimates are saved verbatim in the portable observation. Those estimates exclude allocator/runtime/scratch and cannot establish memory savings against old full-volume solves. There was one solver run; harness was rerun after correcting object-key-order comparison without rerunning the solver. No heavy run or budget escalation occurred.

Integration decision: **no-go for EM integration; graph controls useful and complete within DC scope**. A distributed shared-plane coupling/ownership mechanism and independent matched broadband evidence remain required. P7 remains open.
