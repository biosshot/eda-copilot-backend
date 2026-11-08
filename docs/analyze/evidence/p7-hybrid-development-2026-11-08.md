# Hybrid operator development controls, 2026-11-08

Implementation in progress under [the accelerated plan](../plans/p7-hybrid-implementation-plan.md). These controls do not close P7 or establish whole-board speed, frequency applicability, or physical acceptance. No heavy board campaign has run.

## Recorded small controls

Native test binaries were compiled with `cargo test --manifest-path native/analyze/Cargo.toml --offline --lib --no-run`. Numerical tests run serially under the existing closure supervisor, 1536 MiB tree private cap and 1 GiB commit reserve, using an ignored development wrapper to convert test exit status into supervisor evidence. Times below include supervisor child startup; peaks are 20 ms sampled lower bounds.

Rows without an explicit release label use debug binaries and do not measure production performance. Release builds are separate from numerical budgets; source hashes were unchanged during compilation.

| Control | Result | Supervisor wall | Evidence under `debugging/p7-closure/` |
|---|---|---|---|
| Charge P inverse, dependent-mode refusal | pass | 0.188 s | `1794119372808264800` |
| Frequency screening DC/RF and invalid materials, two tests | pass | 0.187 s | `1794119384704103400` |
| Initial planar self quadrature | refused / test failed | 0.813 s | `1794119346755013600` |
| Adaptive axial/volume self quadrature, maximum order 64 | refused / test failed | 17.078 s | `1794119515388848700` |
| Self autocorrelation, guarded kernels, overlap, sparse inputs, seven tests | pass | 0.265 s | `1794119677730371400` |
| Two geometric supports, two excitations, contact refusal and charge controls, three tests | pass | 0.250 s | `1794119761099636400` |
| Thin touching prisms positive | pass | 0.203 s | `1794119889831467700` |
| Thin self versus tetra integral reference | pass; relative difference 3.53745373e-7 | 0.234 s | `1794120051399932600` |
| Thin touching cross versus tetra integral reference | pass; relative difference 1.12178793e-7 | 0.250 s | `1794120061387000400` |
| Imported air-board prototype dispatch | failed; zero current-mode dimension | 0.250 s | `1794120029215401700` |
| Imported air-board after zero-current fix, two drives and missing contact | pass | 0.265 s | `1794120283622096900` |
| Spatial dielectric versus uniform-layer analytic capacitor and cut-cell refusal | pass | 0.203 s | `1794120733589767000` |
| Expanded planar kernel suite | 11 pass, arbitrary shared-edge refinement refuses | 0.532 s | `1794120580710209200` |
| Arbitrary shared edge after Duffy decomposition | pass | 1.906 s | `1794120894151578800` |
| Arbitrary edge versus tetra reference and reversed support order | pass; tetra relative difference 1.90125853e-8 | 3.563 s | `1794121131140798500` |
| Imported air-board plus spatial FR4 solve, two drives and missing contact | pass | 0.250 s | `1794121121658698300` |
| Imported air/component/FR4 control in full-mixed mode before coalescing | deadline refusal | 25.219 s | `1794121286461377400` |
| Same full-mixed control after exact constant-support coalescing | pass | 0.375 s | `1794121433391781800` |
| Four-coordinate tetra vertex dual rank | pass | 0.187 s | `1794121443984777000` |
| Spatial dielectric energy, cut-cell and count-budget controls | three pass | 0.172 s | `1794121445581689300` |
| Matrix-free charge CG exact coupled solution and invalid-operator/resource controls | pass | 0.171 s | `1794121915239682700` |
| Planar action versus small projected Gram | pass | 0.188 s | `1794121916810023800` |
| Imported via full-mixed prototype | refused by 1024-coordinate prototype guard | 0.719 s | `1794121653755905000` |
| Compact field-action/charge-inverse development controls | pass | 0.218 s | `1794122550318700800` |
| Prepared sparse spatial-field action, two drives and insulated gauge versus dense C | pass | 0.188 s | `1794122961255458600` |
| Spatial dielectric controls including prepared factor path | four pass | 0.172 s | `1794122974603574000` |
| Aligned XY footprints with unequal heights and thicknesses versus tetra reference and reciprocity | pass | 0.344 s | `1794124472534135500` |
| Shared-vertex radial primitive versus independent outer rule | pass | 0.187 s | `1794124901658850500` |
| Skinny shared-vertex pair versus high-order axial rule | pass; relative 4.25700074e-9 | 0.438 s | `1794124902958604900` |
| Shared-vertex pair versus tetra reference | pass | 0.328 s | `1794125222605107700` |
| Complete planar primitive suite after 2D edge and 3D vertex reductions | nineteen pass | 1.031 s | `1794125694692061200` |
| Adaptive air/component/FR4 positive after prepared-field and certificate changes | pass | 0.266 s | `1794125806098218600` |
| Reversed small-tetra observer versus independent three-tetra source partition | pass; relative difference 1.35470036e-16 | 0.172 s | `1794126842014797700` |
| Compact projected/operator controls after adaptive wiring | pass | 0.219 s | `1794126872232650200` |
| Imported air/component/FR4, adaptive route | pass | 0.266 s | `1794126873547863400` |
| Release tetrahedral polynomial moments and independent finite-integral bound | pass | 0.203 s | `1794129209195452400` |
| Release complete planar primitive suite | nineteen pass | 0.344 s | `1794129210549882500` |
| Release unified prism/tetra magnetic action and row bounds | pass | 0.172 s | `1794129212056263600` |
| Release imported via, unified near/far plan | native deadline refusal; no channel | 25.218 s | `1794128258061510700` |
| Release finite-thickness profile basis: DC, weighted Gram/transport, analytic slab loss convergence, high-thickness and invalid inputs | four pass | 0.172 s | `1794129765182360700` |
| Release goal-admitted near moments, strict exact fallback, shared rows, cancellation and positive-underflow bound | pass | 0.219 s | `1794129883172122100` |
| Release adaptive air/component/FR4 after goal admission | pass | 0.171 s | `1794129930738947800` |
| Release imported via after goal admission | native deadline refusal; no channel | 25.219 s | `1794129883632587900` |
| Release compact suite including serial/two-worker bit identity on two distinct planar/local jobs and worker-memory refusal | nine pass | 0.187 s | `1794130552987098700` |
| Release skin mesh target, radial constraints and tighter supplied-step regression | three pass | 0.172 s | `1794130553412407500` |
| Release explicit automatic-hybrid layered dispatch, two drives and missing contact | pass | 0.203 s | `1794130568907316700` |
| Release imported via with four planar/local workers | native deadline refusal; no channel | 25.219 s | `1794130593376590500` |
| Release original profiled pad/barrel fixture with ownership and lift/projection checks | pass | 0.250 s | `1794130934337610800` |
| Debug wide isolated stratified foil, δ/5 target, per-support 0.2 screen, ownership/volume/contacts and transpose power | pass | 0.235 s | `1794131079169309300` |
| Exact physical sample algebra, one/two-port S→Y/Z, actual derived-coefficient roundtrip, singular/reference refusal | pass | 1.484 s | `1794131006426455400` |
| Automatic foil DSL/physical-sample suite, including producer-table pre-clone cap, off-grid, topology, package/contact and evidence controls | four pass | 1.641 s | `1794131108711896500` |

The algebraic controls use the existing native frequency-network solver; these are matrix and mock-producer integrations, not new EM extraction or board timing. Root review changed derived replay to consume its own Y/Z values rather than silently substituting the retained S provenance. One-port 100 Ω and two-port coupled expected matrices, actual roundtrips, singular conversion and unchanged-reference refusals pass. TypeScript typechecking passes after the schema/resource-budget changes. No physical or temporal acceptance follows.

The first stratification assertion failed on the original via fixture (`1794130553803999700`, 0.234 s), whose local halo leaves no applicable foil in this mesh. That fixture retains its contact/ownership checks; a separate positive wide-foil input is modified only in memory. Its first import failed because the test added an unsupported top-level `vias` field (`1794130934782036100`, 0.156 s). Removing that field and using the canonical nested geometry permits the positive control above. Neither a source fixture nor an applicability threshold was weakened. The production native changes compiled in release; the final fixture correction and stricter positive assertion compiled and ran in debug. No full profiled-channel or RF acceptance is established.

The automatic-hybrid policy uses explicit global compact assembly rather than misreporting an original-space Schur solve. Ten focused cache/validation tests pass with generation v3, including policy/worker isolation, mismatched evidence and automatic request routing. The initial positive mock incorrectly reported a producer budget different from its supplied budget; correcting the mock restored the pass without changing validation. TypeScript typechecking passes. The four-worker via still does not finish; no parallel full-channel speedup is claimed.

Goal admission does not resolve the via bottleneck in this control. Its initial sparse field/charge preparation completes in 0.074 s, then the magnetic action still reaches the unchanged native deadline. Moment-tail contributions use absolute density envelopes and outward-rounded positive arithmetic; near quadrature and floating-point evaluation of the moment expansion are not certified by the geometric tail. Initial unified admission control (`1794129779219477100`, 0.188 s) passed before root review added positive-underflow protection and the corresponding algebraic assertion; the final control above covers that correction. No full-board speed claim follows from these results.

The profile control reconstructs complex even-profile dissipation from the returned real basis and resistive Gram, then compares thickness refinement against Re[κ coth(κt/2)/(2σ)]. It includes t/δ=1000 to detect upper-surface exponential overflow. This standalone basis is not yet integrated into conservative mixed interface/current coordinates; runtime frequency screening is unchanged. The analytic loss reference's coth argument was corrected during root review before the first numerical invocation. Ten cache tests and TypeScript typechecking pass with the hybrid operator generation bumped to v2; the 39-row structural crosswalk also passes, without promoting physical status.

The unified mixed plan retains original finite supports and the existing near kernels; degree-four tetrahedral moments enter the same geometric far remainder as prisms. Pair interactions are shared across training columns. Its partition control passed in debug (`1794127637990320400`, 0.171 s), as did tetra moments (`1794127636585656400`, 0.266 s) and unified actions (`1794127639301473100`, 0.172 s). Both debug (`1794127725717895000`, 25.688 s) and release via attempts remain incomplete at the unchanged native deadline. The plan still expands individual pairs and does not establish scalable block/FMM complexity.

The reversed observer control initially used an unconverged 28-point large-prism observer reference (`1794126358366879500`). Its replacement sums three exact tetrahedral source supports with the same small observer and unchanged 1e-10 adaptive tolerance; the 1e-7 comparison gate is preserved. The via control now retains a 25 s native deadline inside the 30 s supervisor to preserve error context. It still refuses before a complete answer (`1794126360053080300`, 25.672 s, 3136 cached mixed pairs); no complete via performance or physical result is claimed. The adaptive runtime uses sparse dielectric factors and full mixed-coordinate residuals, but its magnetic local/cross interaction cost remains open. No request memory or accuracy gate was increased. The field assembly term cap is converted with checked 12× tetra allowance (36 terms per product prism / 3 tetra per prism), rather than equating unlike counts.

The radial edge reference initially missed the 1e-8 comparison at distance 10 with a plain 64-point outer rule (`1794125587624834400`). Its endpoint logarithm is now resolved with r=t², and independent 32/64 sensitivity must first pass 1e-9. Production gates are unchanged. After edge reduction the via planar actions finish around 5.6 s; the run then refuses the second prism/tetra interaction at the unchanged quadrature gate (`1794125709774229100`, 6.297 s overall). This is progress past an earlier timed-out stage, not a complete via result.

The tetra comparisons use constant density integrals on the same finite support; they are independent integration controls, not independent full-channel or broadband EM references. Initial invalid reference-tolerance test invocation (`1794119924965750500`) was corrected before the two successful reference comparisons; no production tolerance was relaxed.

The focused TypeScript cache/compact-report suite passed ten tests, including operator-version/policy isolation and evidence/power/ownership tampering. TypeScript typechecking passed. Temporary directories were placed under workspace `debugging/cache-test-temp` because the sandbox OS temporary path could not support the disk-cache subprocess checks.

An all-target test compilation encountered the stale `docs/experimental/analyze/routing-board-v2-example.json` include in `native/analyze/tests/import.rs`. The test now uses the existing canonical `docs/analyze/reference/input/routing-board-v2-example.json`, without altering the fixture. Targeted import test compilation and `stackup_and_materials_are_validated_without_defaults` pass (0.172 s supervised, `1794122115850943500`). This does not establish an all-target build pass.

## Outstanding work

The adaptive via control no longer hits the dense 1024-coordinate cap. Sparse field preparation and initial C actions finish around 0.7 s, but the 30 s development control still times out in magnetic preparation/actions. Stage/pair profiles are retained in `1794123170655018100` (obsolete charge diagonal), `1794123593433105100` (spatial field fixed; magnetic action), and `1794124485867284700` (batched planar pair timings). These are incomplete attempts, not accepted via results or full-channel speed measurements.

The crosswalk parser was corrected to retain headerless table continuations after blank lines. It now checks all 39 rows in the three canonical case sections, including blind/buried via, connector/pad and differential cases. Earlier 31-row structural passes omitted eight required rows and did not prove full coverage. New rows remain explicitly unmapped/open until linked to implementation and references; no physical status is promoted.

The shared contract and direct kernels are being integrated. Zero-current capacitive dispatch and the small spatial FR4 solve now have positive controls. The former arbitrary-edge failure was corrected by Duffy decomposition; the reference and reciprocity control passed. Other near geometries, full mixed AC basis and charge rank, mixed interfaces/components, representative spatial dielectric boards, planner selection, complete coverage/reference bindings, resource scaling and final five-board/coupon acceptance remain open. Experimental compute completion always retains `coverage: unverified`; no acceptance status is raised by this record. The first native checkpoint recorded the failing arbitrary-edge positive test explicitly; its subsequent correction does not confer complete-family acceptance.
