# DC implementation and verification

Status: **D1–D3 correction review pending**. The current correction report is
[DC fixes verification](dc-fixes-verification.md). The earlier numerical evidence
below is retained as the explicit pre-fix baseline, not a new-version run.
All **8/8 real PDNs solve at both loads and both meshes: 32 complete physical
results, no rejected calculations or unknown checks**. Every measured voltage is
finite; integral drop/loss changes remain below 2%. Three V2 JSONs were re-exported
from the preserved native captures through Copilot `c7b40f2`; only the 12 diagnosed
API phantom holes were removed. Native source/project files remain unchanged.
No automatic DC commit or next-stage work. DSL unit-check fix remains `5b07c33`.
Earlier 0/8 and 5/8 reports remain historical evidence, superseded by these runs.

## Input boundary (before physical implementation)

| DSL input | Internal problem → Rust | Units/source | Missing/unsupported |
| --- | --- | --- | --- |
| board | input V2 JSON → existing import/conduction | absolute mm, y down, captured SHA256 | reject invalid input; no repairs |
| stackup/materials | existing AnalysisPhysicalProfile | explicit mm/oz→mm; S/m and declared state | no conductivity/thickness defaults |
| temperatureC | explicit constant temperature or complete linear resistivity law | °C; sigma(T)=sigma(ref)/(1+alpha*(T-ref)) | partial law or missing state rejected; no guessed material |
| viaPlatingThicknessUm, platingMaterial, diameterConvention | profile.barrel | µm→mm; named material; finished_hole/before_plating | required for participating barrels |
| sources | concrete positive/negative pad terminals, ideal voltage/current elements | V/A; authored ideal model | resources, waveform, circuit rejected specifically |
| loads | existing prepare resolver → explicit pad assignments; port resistance/current | A/Ohm; preserve manual/equal provenance | power/model/waveform rejected; no capacitor short |
| finite pad contact | one equipotential external electrode over its mesh area | existing terminal triangles/nodes | absent/ambiguous/unbuilt contact rejected |
| mesh/geometry | existing ConductionOptions | mm²/mm; explicit scenario or recorded defaults | existing readiness/M1/M2 guards preserved |
| inputPolicy.cadFill | strict/default or explicit snapshot permission | captured freshness/hash | only unknown freshness eligible; no missing/stale copper bypass |
| budget/numerics | DC assembly/factor/maps caps and residual thresholds | MiB/s; dimensioned floors documented with tests | reject before oversized allocation; no hidden gmin |

The implementation reuses the existing process dispatch inside the disposable
DSL process, with no nested native process to orphan. A scenario-owned Rust DC
session retains at most one preparation/factorization for repeated RHS; changing
context, geometry, profile, contact/element matrix or limits invalidates it.
No global cache or alternate public runner is introduced.

## Implemented physical boundary

`runAnalysisDsl → dc().run() → existing process problem dispatch → shared addon →
pcb-analyze::dc`. Native runs in the disposable DSL worker itself: no nested child
process can survive its cancellation. Independent internal `kind:'dc'` jobs clear
the owned session after each call; the DSL closure retains at most one preparation
and one factorization until its scenario exits.

Rust owns triangle assembly, electrode reduction, finite barrel resistors, sparse
factorization, RHS/refinement and field/power post-processing. There is no third
board model, independent mesher, ideal net/GND, hidden gmin, refill or GPU path.
The existing geometry approximation and M1/M2 readiness restrictions are retained.
`faer` is pinned at 0.24.4, std+sparse-linalg without a mandatory external BLAS/runtime;
the sparse QR API is used from [its primary documentation](https://faer.veganb.tw/docs/sparse-linalg/linsolve/).

The finite-area external electrode collapses nodes of its existing terminal
triangles on the first physical surface. Missing/unbuilt first-surface contacts
do not fall back to another layer. Barrel contact rings are equipotential
circumferences connected by each finite segment, retaining intermediate contacts
and empty bores. These are declared contact approximations, not 3D extraction.

Voltage-source constraints use a sparse saddle system and one mathematical gauge
per connected system component. Reported reference_node is the actual chosen
electrical gauge, not a guessed global GND. Current sources inject positive into
their positive contact; user current loads withdraw it. Source currents are
positive delivered; load accessors are positive absorbed. Conflicting/dependent
ideal constraints, open returns, rank-deficient systems, nonfinite fields and failed
KCL/energy guards reject the result. No successful short is inferred by net name.

Supported: constant ideal voltage/current sources, port current/resistance loads,
existing per-pin equal/manual residual assignments, explicitly described two-pad
resistors/DCR, and uniform rectangular conductor current/voltage excitation.
Capacitors remain DC opens. Unsupported models (power, waveforms/atS, general
circuit, resources, implicit DCR/ideal inductor, aggregated multi-pad port sides,
custom solver tolerances) are specifically rejected. Other analyzers/search still
return ANALYZER_NOT_IMPLEMENTED.

The DSL adds only DC inputPolicy/mesh and scenario platingMaterial/diameterConvention.
No V2/export change. Existing profile conversion handles mm↔SI; FEM area mm² times
gradient (V/mm)² gives W directly, J is sheet(S)×gradient(V/mm)/thickness(mm)=A/mm².
lossMap stores triangle W, not W/mm². Barrel/load/copper powers remain separate.
Real scalar transforms and reducers preserve validity; incomplete map reducers
stay conditional and potentials with independent references cannot be combined.

`prepare()` reports structural validity and an available DC executor, while keeping
physicalReady false until native preparation. A run does all required physical
checks itself. Snapshot permission only accepts available/unknown-freshness CAD
polygons, records the original freshness and input identity, and stays conditional.
Stale/missing copper, unknown drills/spans, topology uncertainty, lost contacts,
unbuilt mesh and unspecified drill spans remain blocking. The solver never rewrites its input.

## Numerical evidence

F64 targets: numeric backward residual ≤1e-8; normalized KCL and power balance
≤1e-8, with absolute floors **1e-12 A** and **1e-15 W** near zero. Voltage constraints
use **1e-12 V** absolute floor and 1e-8 relative guard. These are distinct units,
not one epsilon. Analytical R tolerance is 1%; integral convergence target 2% with
1e-9 V floor. No tolerance was increased to force a pass.

| Independent problem | Observed result |
| --- | --- |
| Straight 8 mm effective strip, w=2 mm, t=.035 mm, sigma=58 MS/m, I=1 A | R=0.001970443349754 Ohm; V=0.001970443349754 V; copper loss=0.001970443349754 W. Three areas .5/.125/.03125 mm² pass analytical comparison. |
| Parallel widths 2 mm / 1 mm, same effective length | R=0.001313628899836 Ohm; independent x=4.731 mm cut integration gives 0.666666666667 / 0.333333333333 A. |
| Supply and return strips + 0.1 Ohm load, 1 V | Finite return drop and divider current match R including both conductors; copper loss 0.364771680780 W. Normalized KCL 2.15e-12, power balance 5.70e-13. |
| Three-layer round via chain, 1 A | R/drop=0.000229161701071 Ohm/V; barrel loss=0.000229161701071 W, two finite segments and intermediate contact retained. Planar loss ≈4.75e-34 W; bore remains empty. |
| Bent coupon, areas .25/.0625/.015625 mm² | Drop at 1 A: .002471756529277 / .002502167195366 / .002514180252338 V. Successive change ≈1.22% / .48%; integral losses agree with V×I. Sharp-corner J maxima are not used as convergence claims. |
| Production JS board coupon with supply/return, 1 / 2 A | Port drop .003940886700 / .007881773399 V; copper loss .003940886700 / .015763546798 W. J doubles, loss ×4. Same .006 V threshold passes nominal and fails raised load; following check/plot executes. Two meshes agree to far below 2%. |

The initial bent-coupon .5→.125 mm² interval changed ~2.16%, above the 2% target.
The test retained the target and used three finer levels; those earlier artifacts
remain under debugging. A development mesh with adjacent unused constraints also
exposed a small backward residual but unacceptable KCL/power error; the balance
guard rejected it. A small residual alone is never a success criterion.

Eleven Rust DC tests also cover rotation/translation, material invalidation,
current/load scaling, missing paths/materials, conflicting/indeterminate sources,
memory refusal, unavailable/stale fills and nonfinite post-processing. Node tests
cover explicit zero/manual pins, independent return distributions, invalid sums,
classified defaults/exclusions, current target ambiguity, unsupported modes and
real production workers. No mock executor is used as DC physical evidence.

Detailed native JSON: `debugging/analysis-dsl/dc-coupons/`. Real public JS coupon:
[`debugging/pcb-analyze/dc-coupon/index.html`](../../../debugging/pcb-analyze/dc-coupon/index.html),
with dc.json/scenario.js. It is visibly marked synthetic and is outside the corpus.

## Repairs after the real-board failures

- Reuse coincident straight domain boundaries when inserting pad terminal
  constraints, including partial overlaps spanning several edges. Insert domain
  boundaries first. Exclude collinear boundary faces caused by floating-point
  centroid classification; do not move copper or waive area/connectivity guards.
  A captured USB union contour regression keeps the finite terminal and mesh area.
- Decompose exact touching union contours into simple cycles, preserving area,
  holes and point-separated regions. Reject crossings/zero-area cycles rather
  than inventing a polygon or merging copper at a point.
- A topology warning becomes nonblocking only with an independent reliable
  **same-layer planar path** proving that uncertain edge/split redundant. Barrel
  links and net labels cannot supply that proof. Original warnings and explicit
  redundancy diagnostics remain in conduction; results stay conditional. Missing
  mandatory contacts and nonredundant uncertainty still block (M1/M2 tests pass).
- Support round-ended plated slots with explicit profile/material/plating data:
  section = round annulus section + 2 * (slot length - width) * plating thickness.
  Bores remain empty, each interlayer segment has finite conductance.
- Assemble triangle conductance as a pairwise Laplacian, preserving the constant
  voltage nullspace. Evaluate KCL and iterative refinement from physical voltage
  differences instead of subtracting rounded large common-mode matrix products.
  The original residual/current/power thresholds remain unchanged.
- Use sequential CPU/F64 `faer` sparse QR. The former dense-fill estimate 96*N²
  rejected sparse real boards without attempting their actual sparse factor.
  Bound symbolic work first, then check the library's factor/index/numeric/solve
  workspace sizes before numerical allocation. No GPU or placement numeric change.

## Preparation, factorization, timing and memory

The scenario owns one preparation/factorization. Board/context/profile/contact/
element matrix or limits invalidate it; changing only source/load RHS reuses it.
DSL defaults: 100000 mesh nodes, 200000 triangles, 100000 reduced unknowns, 1024 MiB
planned working set, 120 s. Corpus uses 500 s native / 540 s scenario deadlines;
NanoScope, NanoSpec and Thunder explicitly permit 300000 nodes/600000 triangles.
Memory planning is **not a hard OS allocator ceiling**. RSS is sampled at native
call boundaries, not peak RSS. No residual or energy tolerance was loosened.

Native elapsed_ms includes preparation/assembly/solve/post-processing, excluding
serde/N-API transfer. run.json wall time includes four attempts, transfer and HTML.
For USB coarse: first native call 2136.90 ms, factor/assembly 43.67 ms; fine high-load
factor reused, solve/refinement 9.47 ms. These are one local run, not a benchmark.

Computed-domain meshes/fields are retained; excluded domains keep readiness/counts
and gray physical copper. A repeated preparation references an earlier solution ID only when that compact
mesh contains every currently computed domain; new domain coverage retains a new mesh. Plots can reference fields already stored in the completed result instead of
copying the arrays again. The default DSL report budget remains 4 MiB, with an
explicit supported ceiling of 128 MiB. NanoScope, NanoSpec and Thunder request
128 MiB; their final reports are 60.70 / 82.17 / 77.88 MiB respectively. Other
corpus scripts retain 64 MiB. Two earlier Thunder attempts
hit the report budget and are recorded as failed, not as successful four-case runs.

## Pre-fix eight-board baseline (before D1–D3)

All runs use the re-exported V2 boards, unchanged native captures/sources and explicit **test hypotheses**:
35 µm copper, 58 MS/m at 20°C, 25 µm plating, total 1.6 mm stack with equal dielectric
gaps. These are not verified product stackups or load ratings. Unknown CAD fill
freshness remains explicitly conditional. A computed `fail` is a failed authored
voltage-drop limit, not a solver rejection or proof of a defective design.

| Board | Four attempts | Largest drop change | Total copper+barrel loss change | Four-case wall s | Max sampled RSS MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| BandAmpV5 | 4 complete | 0.632% | 0.633% | 13.622 | 258.0 |
| ESPower | 4 complete | 1.003% | 0.979% | 18.297 | 303.7 |
| NanoScope | 4 complete | 0.392% | 0.392% | 208.421 | 890.2 |
| NanoSpec | 4 complete | 0.382% | 0.086% | 148.954 | 842.9 |
| STM32G0_CAN_RS485 | 4 complete | 0.680% | 0.610% | 10.057 | 339.7 |
| TPS63060DSCR | 4 complete | 0.847% | 0.839% | 3.995 | 198.3 |
| ThunderF722 | 4 complete | 1.491% | 1.401% | 74.165 | 906.5 |
| USB_PD_Trigger_5A | 4 complete | 0.295% | 0.550% | 8.541 | 284.2 |

Changes are relative to the coarse result, with a 1e-9 V/W floor; both load levels
are checked. Every computed map has finite values and exact mesh node/triangle
cardinality. Residual/KCL/energy guards pass. Thunder has 292 nonzero finite barrel
segments in its nominal coarse case. It supplies actual multilayer current coverage.

USB_PD_Trigger_5A: nominal 2 A / high 4 A. Fine-mesh output drop is
0.086607280868 / 0.173214561736 V; copper+barrel loss is
0.093214561736 / 0.372858246944 W. The same 0.100 V limit passes nominal and fails
high load. A following loss check and plot execute; no value is replaced by unknown.
F1 is an explicit 0.02 Ohm test resistor; its loss is separate from copper loss.

Resolved input discrepancies: NanoScope had ten affected pads (both pins of
SJ1–SJ5), NanoSpec U5.11 and TPS63060DSCR U1.11 one each. Native footprint PAD
records contain hole:null, while the EasyEDA POLYGON pad API returned a drill.
Copilot commit `c7b40f2` omits API holes on TOP/BOTTOM POLYGON pads with a per-pad
warning. This is an explicit compatibility workaround, not a source-based general
hole validator. True multilayer pad holes and vias remain unchanged.

The user authorized updating the Backend fixtures. `fixture.json.reexport` records
exporter revision, original capture hash, prior board hash and replay time; original
capture versions/time/nativeSummary remain historical raw-capture metadata. Export
inventories, diagnostics and normalized file hashes describe the corrected V2.
All 32 manifested files verify; raw captures equal their original Git versions.
Only 120 hole-description lines were removed from the three board JSONs.

After re-export, NanoScope/NanoSpec reached the equations but exposed loss of
sub-ULP refinement corrections at the absolute rail voltage. Preserve a F64
compensation term for each unknown and use the compensated difference consistently
for refinement/KCL, gradients/losses and barrel/element drops. Same sparse QR,
three refinement steps and unchanged acceptance thresholds; no mesh/material edits.
The small rail-offset regression fails the former code with KCL 7.4e-5 A and passes
the correction. Its 1e8 V case stresses the linear algebra, not a physical rating.
NanoScope's direct replay KCL improved from ~1e-7 A to 6.25e-17 A. The largest
normalized KCL across the final real corpus is 3.56e-14.

Baseline evidence: `dc-reexport-corpus-final.log`, `dc-reexport-corpus-matrix.json`,
`dc-reexport-audit.log`, and `dc-reexport-fixture-integrity.json` under
`debugging/analysis-dsl/`. Each run records input/profile/script/source/binary hashes.
All 32 use one implementation hash
`877db21a7ed01df32e4630f377c39cb8c79c1f985f493a518561b399deb21ddd`
and native binary `088324da0ff23e41ebc72c7103aceb17d427ac6a6bb61b6ee26e5381e7d90ddd`.
The earlier re-export attempt (6 complete scripts / 2 failed balance guards) is
retained in `dc-reexport-corpus.log`; it is not counted as final success.

## Commands and verification

From the backend root:

```powershell
npm run native:build
cargo test --release --locked --manifest-path native/analyze/Cargo.toml
npm run typecheck
npm run build
$env:PCB_BLOCK_BACKEND='cpu'
$env:PCB_BOARD_BACKEND='cpu'
$env:PCB_POST_PLACE_BACKEND='cpu'
node --import tsx --test --test-concurrency=1 tests/analysis-dsl.test.ts tests/pcb-analysis-dc.test.ts tests/pcb-native-addon.test.ts tests/pcb-analysis-runtime.test.ts tests/pcb-analysis-conduction.test.ts tests/pcb-analysis-geometry.test.ts tests/public-api.test.ts tests/pcb-board-hole.test.ts tests/pcb-constraint-region.test.ts tests/pcb-local-layout.test.ts tests/pcb-signal-path.test.ts tests/pcb-placement-validation.test.ts
npm run test:package
npm run test:pcb-analyze -- --all --name dc
npm run test:pcb-analyze -- --all --name import
node --import tsx debugging/analysis-dsl/audit-dc-reexport.ts
node --import tsx debugging/analysis-dsl/dc-packer-replay.ts
```

- Full Rust release suite: **69 passed, zero failed/ignored**, including the
  eight-board geometry corpus and the new compensated-refinement regression.
- **106 selected Node tests passed**, zero failed/skipped: DSL/DC, strict addon,
  runtime, geometry/conduction, public APIs and placement compatibility. This is
  the named suite above, not a claim that full npm test was run.
- Eight named import scripts and eight named DC scripts pass. All DC scripts
  require four complete physical results; the former expected-rejection goldens
  were replaced. Audits separately assert finite fields/measurements, mesh array
  cardinality, current/power/residual guards, two-mesh convergence and no unknowns.
- Native build, typecheck and build pass. Installed package passes normal,
  GPU-disabled and unavailable-driver modes, including real FEM DC and original
  placement APIs. No package was published; consumer:
  `C:/Users/kiril/AppData/Local/Temp/eda-backend-package-81kxGR`.
- Short CPU packer replay remains deep-equal to the pre-DC installed archive,
  including f32-rte-ftz-v1 and rank 6.099999904632568.
- Actual native abort/timeout recovery passes in the selected Node suite.

A report check `fail` means the explicitly authored voltage-drop limit was exceeded
under the test load hypothesis. These outcomes are retained and execution continues;
passing the runner does not mean every board meets an arbitrary scenario limit.
All 32 calculations and every requested measurement are available and finite.

Logs are `debugging/analysis-dsl/dc-reexport-*.log`, with the native build and
before/after stress evidence under `dc-compensated-*.log`. No tolerance was relaxed.

## Artifacts, visual QA and limits

[Eight-board overview](../../../debugging/pcb-analyze/dc/index.html) links the
current runs. Each includes scenario.js, dc.json, implementation.json, run timing,
input hypotheses, maps, measurements, checks and mesh comparisons. Original failed
runs remain in their separate timestamped folders. The synthetic coupon is labelled
separately and is not counted as a real board.

Browser QA checks actual USB current maps at both loads on a shared scale, the
high-load failed check followed by a pass, and Thunder TOP/INNER layers. Copper,
bores and terminals share the absolute mm/Y-down frame. Computed regions are
coloured from native values; excluded regions remain gray. Raw numerical inputs
and large barrel tables are collapsible, so consumer results remain visible.
Current additional browser checks cover NanoScope INNER_1 current density and NanoSpec nominal/high-load voltages, with four complete mode options and working tables. Screenshots: `debugging/pcb-analyze/dc/screenshots/reexport/`.

Review [diff](../../../debugging/analysis-dsl/dc-changes.patch),
[manifest](../../../debugging/analysis-dsl/dc-manifest.json),
[integrity](../../../debugging/analysis-dsl/dc-reexport-integrity.json).
The preexisting task document and native/analyze/debugging/ remain excluded.

All eight real boards meet the requested working-set coverage with both load
levels and converged integral results. This completes the DC task's numerical
coverage; acceptance remains with the reviewer. Finite
geometric approximation, equipotential pad/ring assumptions, unverified stackup/
material/load hypotheses, unknown CAD freshness, and sampled memory remain explicit.
No sub-tolerance topology guarantee or peak-J convergence/certification is claimed.
Unsupported dynamic/resource/circuit/constant-power models still reject explicitly.
Linux/macOS, remote CI and full npm test were not run. No DC commit and no next stage.
