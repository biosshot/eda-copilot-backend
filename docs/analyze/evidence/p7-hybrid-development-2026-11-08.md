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
| Debug stratified foil with integrated profile hint factory, uniform-J transport cancellation and budget refusal | pass | 0.250 s | `1794131678668879000` |
| Debug automatic layered air/component dispatch after hint seeding integration; no eligible groups | pass | 0.344 s | `1794131679141938600` |
| Release hybrid policy, operator-specific limits, required assembly policy and unchanged electrical/contact refusal | pass | 0.188 s | `1794132678026834700` |
| Release initial connected 100 kHz skin coupon with narrow pads and exposed rounded track ends | native deadline refusal | 25.172 s | `1794132678427791100` |
| Release connected rectangular 1 MHz foil coupon, finite pads, nonzero added skin hints, coupled residual/power checks | pass | 0.329 s | `1794133031540159800` |
| Port/contact observable algebra and automatic DSL/mock replay, with native electrode names distinct from terminal IDs | six pass | 1.750 s | `1794133376038377800` |
| Debug connected 1 MHz hint response after output-reserve correction, including planned/report bytes versus input budget | pass | 0.797 s | `1794133971636968500` |
| Mixed-mode algebra and existing differential-target planner mapping | seven pass | 1.703 s | `1794134397315060500` |
| Mixed-mode algebra plus result publication, asymmetric driven-column power, passivity and source-port metadata | six pass | 0.719 s | `1794134810094855700` |
| Rebuilt actual addon, coupled 1..1.1 MHz DSL with finite physical ports, adaptive audit and cold/warm/restart | pass | 3.516 s | `1794134754639610200` |
| Complex-drive loss interference and conjugate symmetry, debug | pass | 0.203 s | `1794135913643304900` |
| Initial model-regime control, debug | 30 s supervisor timeout | 30.016 s | `1794135914051443400` |
| Same-model passive RLC DC/AC/time and invalid bindings, release | two pass | 0.938 s | `1794136278467469800` |
| Extracted connected foil with optional model regime control, release | pass | 0.422 s | `1794136357337879000` |
| Initial component regime control, release | refused; singular DC model | 0.219 s | `1794136430390573500` |
| Bounded component failure capture, debug | refused; missing common charge candidate confirmed | 0.719 s | `1794136904435211900` |
| Corrected regime controls including actual copper/resistor composition, release | three pass | 0.938 s | `1794137508163999000` |
| Extracted air/component/FR4 controls with loss Grams and requested regime verification | pass | 0.250 s | `1794137509313028800` |
| Existing saved bend/via composition conservation regressions | two pass | 0.938 s | `1794137509758921400` |
| Existing physical-terminal regressions | nine pass | 1.344 s | `1794137510895664200` |
| Existing coupled descriptor regressions | ten pass | 0.687 s | `1794137512451650800` |
| Consumer loss Gram/phase/PSD/modal power and cache/model-control tampering | eighteen pass | 1.968 s | `1794137796614063300` |

The debug transient timeout was not treated as production performance; the optimized identical passive-model control completes below one second without raising its limit. The first component control exposed a real missing global charge candidate: the assembled SPD capacitance was retained but its shared physical mode had been lost when charge modes and local package gauges were intersected separately. The bounded capture contains two copper potentials and one 25 Ω bridge branch, with no conserved mode. The fix unions the two source spans before the existing weld intersection, preserving the separate certified structural gauge. Regression now assembles the actual submodels and verifies a 12.5 V DC differential for a 1 A matched drive, physical common-charge initialization and AC/transient agreement. Source C, physical contacts and all solver guards are preserved. Temporary model-capture code was removed after diagnosis.

The loss Gram consumer also rejects false owner IDs, mismatched diagonals, conjugation/phase errors, indefinite matrices and a negative weak-power matrix. Scalar source power floors remain unchanged; PSD/projection roundoff uses its own floating-point scale. Native Gram storage reserves 2048 bytes per complex output entry plus package identity/state buffers before allocation, conservatively accounting for JSON object leaf storage. TypeScript typechecking and package build pass. Rebuilt-addon verification of this extended Gram/regime protocol is pending; these numerical model checks do not close independent physical DC/AC/time or full-board acceptance.

The corrected addon rebuilt successfully in 9m03s, separately from numerical budgets. `scripts/verify-p7-hybrid-coupled-smoke.ts` retains the same in-memory two-conductor rectangular coupon, original fixture file unchanged, with 25 s/512 MiB request caps and the existing 30 s/1536 MiB supervisor plus 1 GiB host reserve. The run checks nonzero skin hints, validated supplemental points, port observations and identical baseline cache keys in all modes. Cold makes two native solves (baseline and audit); warm makes zero, and a prototype guard forbids any native solve in the restart child. Sampled tree-private peak is 403,197,952 bytes (20 ms lower bound). Exact-grid quality is KCL=1.11022853e-16, KVL=8.36447330e-12 and matched power error=5.04972583e-15 W; four profile hints are retained across twelve eligible groups. This is a small development coupon, not a real-board performance or independent RF accuracy result. The initial ignored runner passed in `1794134512013341700`; promotion to the checked-in runner passed in `1794134587122640300`. The last run above adds strict zero-native-call guards for warm and restart and is the current cache evidence.

Mixed-mode controls use explicit differential DSL targets and a power-wave transform. Root review corrected an early row-norm power calculation to use the driven column; an asymmetric matrix now checks expected modal powers 24.75/4.4375 W. Material negative power is refused, and a floating-point rounding bound is exposed. Source port pairs and physical contacts remain explicit; no spatial-current or separate modal copper/package-loss acceptance is claimed. TypeScript typechecking passes.

`npm run native:build` completed the first fresh addon in 7m04s (separate build budget). The first actual coupled RF DSL attempt refused at the DSL verification-budget contract before numerical work (`1794133589188604000`, 1.063 s). Matching explicit verification settings to the smaller 25 s/512 MiB test budget let the numerical solve finish around 0.444 s, but strict consumer validation correctly refused producer memory evidence (`1794133610128254700`, 1.765 s overall). The native producer had added its output reserve back without deducting it on the board-endpoint path. It now reserves output/package space before map/model allocation and keeps direct mesh callers consistent; the debug positive above also asserts planned bytes and reported memory stay within the admitted budget. Consumer gates were not changed. The failed actual-addon attempt is not accepted; the subsequent rebuilt-addon control above verifies the correction.

The observable controls cover a 100 Ω DUT at a 50 Ω reference (V=33.333 V, I=1/3 A under a 1 A matched Norton drive), unequal references with real port power equal to copper plus package loss, shared-contact aggregation and distinct layers at the same source path. Root review corrected the original mapping through native electrode names; the DSL integration now uses actual physical terminal IDs. These are port observations, not recovered spatial copper currents. TypeScript typechecking passes.

The first connected control retained 516 physical supports (predominantly local tetrahedra) and did not complete its initial magnetic action; it remains an incomplete geometric-control attempt. The new minimal planar positive uses 35 µm copper and pads enclosing the track caps, producing 18 resolved finite-prism supports and 36 mixed current coordinates. It completes at 1 MHz with nonzero admitted profile hints, KCL/KVL below 1e-8 and the unchanged matched-power checks; its native elapsed time is 0.136 s. These are different in-memory coupons, not a before/after speed comparison, and do not prove the refused geometry or RF-board accuracy. All source fixtures remain unchanged.

Thirteen focused TypeScript cache/frequency tests pass, including the separate hybrid input envelope, operator/policy/profile isolation, stronger planar spacing, evaluated supplemental point visibility, no insertion after a refused batch and stable-basis identity versus varying quadrature statistics. Early new mock tests used an invalid hybrid certificate and an incorrect sample field; they were corrected to exercise the actual validation/audit pipeline without loosening its gates. TypeScript typechecking passes. Production/native RF-policy and connected-coupon changes compiled in release with stable source hashes. Independent RF references, full board timings, scalar/DC/time overlap and remaining case acceptance are still open.

The adaptive producer now previews complete uniform-material slab groups, admits hint/descriptor/growth storage before allocation and R-orthogonalizes nonuniform profiles on top of DC modes. Its retained peak uses the larger profile preflight, and the factory refuses nonfinite products. The positive factory control preserves current lift/transpose power and zero integrated nonuniform transport for a manufactured uniform-J seed. The small dispatch control verifies the no-eligible-group branch. An end-to-end frequency response exercising nonzero added hints, independent interface refinement and RF physical acceptance remain outstanding. Debug builds retained unchanged Rust source hashes during compilation; TypeScript typechecking passes after correcting the compact result-label type narrowing. No full-volume audit is claimed by the compact result label.

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

### Fresh addon integration after ceb8511

- Release addon build completed in 6m29s (build cost, outside numerical test budget).
- `1794138422401324600`: extended coupled DSL cold/warm/restart smoke passed in 3.672s; sampled process-tree private peak 408834048 bytes under 1536MiB cap. Complex copper loss Grams survive validation and disk replay; numerical KCL 1.11023e-16, KVL 8.36448e-12, power balance 5.04973e-15W. Physical coverage remains unverified.
- `1794138385189256000`: same smoke with optional regime audit failed in 1.828s at first time integration (`COUPLED_KCL`, drive 0, 128 steps). This is an open integration defect despite passing smaller regime regressions. No acceptance status or numerical gate changed. Heavy final campaign remains pending.
