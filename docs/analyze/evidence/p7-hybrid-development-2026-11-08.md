# Hybrid operator development controls, 2026-11-08

Implementation in progress under [the accelerated plan](../plans/p7-hybrid-implementation-plan.md). These controls do not close P7 or establish whole-board speed, frequency applicability, or physical acceptance. No heavy board campaign has run.

## Recorded small controls

Native test binaries were compiled with `cargo test --manifest-path native/analyze/Cargo.toml --offline --lib --no-run`. Numerical tests run serially under the existing closure supervisor, 1536 MiB tree private cap and 1 GiB commit reserve, using an ignored development wrapper to convert test exit status into supervisor evidence. Times below include supervisor child startup; peaks are 20 ms sampled lower bounds.

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

The tetra comparisons use constant density integrals on the same finite support; they are independent integration controls, not independent full-channel or broadband EM references. Initial invalid reference-tolerance test invocation (`1794119924965750500`) was corrected before the two successful reference comparisons; no production tolerance was relaxed.

The focused TypeScript cache/compact-report suite passed ten tests, including operator-version/policy isolation and evidence/power/ownership tampering. TypeScript typechecking passed. Temporary directories were placed under workspace `debugging/cache-test-temp` because the sandbox OS temporary path could not support the disk-cache subprocess checks.

An all-target test compilation encountered the stale `docs/experimental/analyze/routing-board-v2-example.json` include in `native/analyze/tests/import.rs`. The test now uses the existing canonical `docs/analyze/reference/input/routing-board-v2-example.json`, without altering the fixture. Targeted import test compilation and `stackup_and_materials_are_validated_without_defaults` pass (0.172 s supervised, `1794122115850943500`). This does not establish an all-target build pass.

## Outstanding work

The crosswalk parser was corrected to retain headerless table continuations after blank lines. It now checks all 39 rows in the three canonical case sections, including blind/buried via, connector/pad and differential cases. Earlier 31-row structural passes omitted eight required rows and did not prove full coverage. New rows remain explicitly unmapped/open until linked to implementation and references; no physical status is promoted.

The shared contract and direct kernels are being integrated. Zero-current capacitive dispatch and the small spatial FR4 solve now have positive controls. The former arbitrary-edge failure was corrected by Duffy decomposition; the reference and reciprocity control passed. Other near geometries, full mixed AC basis and charge rank, mixed interfaces/components, representative spatial dielectric boards, planner selection, complete coverage/reference bindings, resource scaling and final five-board/coupon acceptance remain open. Experimental compute completion always retains `coverage: unverified`; no acceptance status is raised by this record. The first native checkpoint recorded the failing arbitrary-edge positive test explicitly; its subsequent correction does not confer complete-family acceptance.
