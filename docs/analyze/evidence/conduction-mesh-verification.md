# Conduction mesh verification

**Validator acceptance, 2026-10-22:** [M1/M2 closed after source/artifact review](conduction-mesh-review-2026-10-22.md). The validator inspected code, regression tests and saved artifacts without rerunning tests/builds at the user's request; the execution evidence below is the implementation author's record. The mesh stage is accepted within its stated limitations.

## M1/M2 corrections, 2026-10-22

Only contact mapping/readiness and its diagnostic contract changed. `barrel.expected_contacts` retains each reliable geometry barrel-edge endpoint with region/layer/source IDs, mapped domain IDs and explicit reasons independently of successful mesh boundaries. Its `ready` describes mesh/mapping/topology, not material completeness. Unbuilt mesh, incomplete refinement, absent domain/boundary mapping or uncertain topology blocks the barrel and its mapped/expected dependent domains. Physically absent geometry contacts do not require copper on every span layer. Available diagnostic segments remain visible even when the full barrel is unready.

Existing `barrel_contact_uncertain` now blocks source-scoped domain topology and the identified barrel; own pad/via provenance does not override it. The geometry diagnostic has source path and hole ID in its message, but no layer/region field. Domain invalidation is therefore conservative across that source's fragments/layers; the barrel association matches the exact `hole_id:` prefix. No proof of redundant paths or new geometry tolerance mechanism was added.

Both new reproducers failed before the fixes ([M1](../../../debugging/conduction-mesh/review-corrections/reproduce-m1.log), [M2](../../../debugging/conduction-mesh/review-corrections/reproduce-m2.log)). The full-profile four-layer JSON coupon has four geometry-confirmed contacts, a naturally unbuilt TOP mesh under its 500-node budget and three surviving mapped contacts. After correction the barrel and four dependent domains are unready; the separate fifth domain remains ready. A second coupon produces a real geometry uncertainty warning. Mapping-unit fault injection separately tests missing boundaries, missing domain association and retained-but-incompletely-refined mesh. A drill wider than the TOP pad tests genuinely absent copper; existing fully built through/blind/buried controls remain ready.

The final Rust run passed **54 tests**: 1 mapping unit test, 19 conduction, 21 geometry and 13 importer tests. The unchanged geometry test reran all eight boards and saved reports; all eight match the pre-review full semantic geometry reports (excluding clocks/encoded byte count). Conduction corpus replay in this correction is limited to **NanoScope and ThunderF722**, plus the negative coupon; the original eight-board conduction measurements below are historical.

| Board | Expected region contacts | Mapped contacts before → after | Original M1 gap barrels now explicitly blocked | Barrels with any unready expected contact |
| --- | ---: | ---: | ---: | ---: |
| NanoScope | 1981 | 859 → 859 | 124 | 256 |
| ThunderF722 | 1604 | 1022 → 1021 | 36 | 105 |

These counts use different units: several source-region contacts can share one mesh-domain contact. The last column also includes incomplete refinement and topology uncertainty; it is not an increase in lost mesh. For NanoScope `pads[216]` / `domain-000191` and Thunder `pads[127]` / `domain-000003`, expected TOP contacts now retain `mesh_unbuilt` and `boundary_mapping_missing`. Thunder `domain-000077` is now topology-unconfirmed, and `hole:via:e103` has `barrel_contact_uncertain:copper.fixed.tracks[215]`; its former domain mapping is excluded.

All domain data except readiness (including complete node/triangle/boundary/terminal arrays and sheet coefficients), profiles and counts are equal to baseline on those two boards. Surviving mappings and common diagnostic segment coefficients are equal. Both remain partial with zero ready domains, since no physical profile was supplied. Encoded report sizes grew from 15,392,098 to 16,224,153 bytes (NanoScope) and 16,906,391 to 17,402,751 bytes (Thunder) due to expected contact references/reasons, without duplicated polygons or meshes. Observed Rust total times were 104.1 s and 41.4 s during concurrent compilation/tests; these are not controlled performance comparisons.

Pre-fix source/report snapshots are preserved under `debugging/conduction-mesh/review-corrections/before/`; the original `rust/` and `geometry-rust/` corpus baselines were not overwritten. [Comparison with per-contact reasons and original gap lists](../../../debugging/conduction-mesh/review-corrections/comparison.json), [comparison script](../../../debugging/conduction-mesh/review-corrections/compare.py), [final Rust log](../../../debugging/conduction-mesh/review-corrections/rust-final-tests.log) and separate `rust/` / `geometry-rust/` directories retain correction evidence.

**48 focused Node tests passed, zero skips**, including the full-profile negative coupon through addon and process worker, strict exports/versions, geometry/runtime, native board packer and public API. Native release build, typecheck, package build, installed-package smoke/fallback checks and four launcher/fixture hash tests passed. [Compatibility evidence](../../../debugging/conduction-mesh/review-corrections/compatibility.json) confirms the eight complete import reports, all prior exports/versions, exact short CPU replay solution/quality (score `772.9915771484375`, hard count 0), and identical replay after conduction preparation. Packer remains `f32-rte-ftz-v1`; analyze remains CPU/F64. Native build warnings are the existing 20 packer warnings and Windows linker informational warning.

The complete NanoScope, ThunderF722 and negative-coupon worker reports match standalone Rust, excluding only `timings_ms` and `serialized_bytes` ([parity results](../../../debugging/conduction-mesh/review-corrections/node-parity.json), [runner](../../../debugging/conduction-mesh/review-corrections/run-node.mts), reports in `review-corrections/node/`). Worker elapsed times were 89.5 s, 28.8 s and 10.5 ms, respectively, under the same non-benchmark conditions.

Executed correction checks (repository root; PowerShell, CPU selectors for Node tests):

```powershell
cargo test --release --locked --manifest-path native/analyze/Cargo.toml -- --nocapture
npm run native:build
npm run typecheck
npm run build
$env:PCB_BLOCK_BACKEND='cpu'; $env:PCB_BOARD_BACKEND='cpu'; $env:PCB_POST_PLACE_BACKEND='cpu'
node --import tsx --test --test-concurrency=1 tests/pcb-native-addon.test.ts tests/pcb-analysis-conduction.test.ts tests/pcb-analysis-geometry.test.ts tests/pcb-analysis-runtime.test.ts tests/pcb-board-packer-native.test.ts tests/public-api.test.ts
npm run test:package
node --test scripts/test-pcb-analyze.test.mjs
node debugging/conduction-mesh/review-corrections/compatibility.mjs
python debugging/conduction-mesh/review-corrections/compare.py
node --import tsx debugging/conduction-mesh/review-corrections/run-node.mts
```

Rust geometry snapshots were enabled with `PCB_ANALYZE_RUST_REPORT_DIR` during the final test run (the relative path resolved under the crate and reports were copied to the root evidence directory). The two real boards and shared coupon were separately prepared by `cargo run --release --locked --manifest-path native/analyze/Cargo.toml --example conduction -- <input.json> <output.json> <options.json>`; exact commands/options are in `real-rust.log` and `options.json` / `coupon-options.json`. Final logs are in `review-corrections/{native-final-build,typecheck,build,node-tests,package,launcher-tests,compatibility,comparison}.log`.

Only Windows x64 / Node 26.5.0 was rerun. Linux/macOS, remote CI, full `npm test`, the other six conduction mesh reports and general mesh/convergence repair were not rerun or claimed by this correction.

The [corrections-only patch](../../../debugging/conduction-mesh/review-corrections/corrections.patch) and [changed files](../../../debugging/conduction-mesh/review-corrections/changed-files.json) compare against the preserved working state immediately before this review task, not HEAD (which also predates the original uncommitted mesh stage). Production changes are 99 added Rust lines in barrel bookkeeping, a two-line expansion of the existing topology filter and two declaration lines; the remaining changes are tests, one shared synthetic JSON coupon and documentation.

No commit or next-stage implementation was made. The remaining unsupported mesh cases and accepted approximation limitation below remain in force.

## Representation and scope (before implementation)

One prepared `regions` model feeds one CPU/F64 constrained Delaunay mesh per connected layer polygon. Boolean union removes overlapping copper before meshing. Pad boundaries are constraints, not centroid terminals. Constraints may split; their resulting mesh edges retain source references. Distinct layer and point-touch domains never share electrical nodes. There is no solver or ideal net/barrel collapse.

Coordinates remain absolute Y-down millimetres. Triangles have positive algebraic XY area (clockwise on a Y-down display). Physical conversion is centralized: mm to m, mm² to m²; sheet conductance is sigma[S/m] * thickness[m]. Z is measured from the top stack surface, copper contacts at layer midplanes. Round barrel area uses an explicitly declared finished-hole or pre-plating diameter convention. Slots retain planar geometry but their barrel model is unsupported.

The optional profile supplies ordered stack entries, per-layer constant isotropic conductivity with explicit material state, and barrel conductivity/plating/diameter convention. Explicit entries override board stackup; omitted data stay missing. No default copper, dielectric or plating is supplied. Readiness records mesh completion, topology, copper completeness and physical completeness separately. Known drill/contact uncertainty and unbounded missing fills block readiness, independently of successful triangulation. Approximation close to tolerance remains the accepted geometry limitation; no universal topology or electrical convergence guarantee is claimed.

Spade is pinned to `=2.15.1`, `default-features = false`, `features = ["std"]`; refinement is bounded by node, triangle and iteration limits. Actual output triangles, constraint subedges and area coverage are checked, rather than trusting triangulator counters. Real fixture profiles, when supplied, are scenario assumptions rather than measured board properties.

## Implementation and supported boundary

- `native/analyze/src/conduction/{mod,mesh,profile}.rs` owns plain Rust report/numerical structures, mesh construction and material resolution. `prepare(&Board, options)` and `prepare_geometry(&Board, &PreparationReport, options)` avoid a JSON round-trip; the latter requires geometry from that board and matching geometry options. There is no second importer or TS geometry implementation.
- One mesh domain is one polygon of the existing layer copper union. Shared mesh indices/edges are local to that domain; identical XY on other layers or point-touch domains are independent. Original region IDs and source paths identify contributors; `sources` resolves paths to native IDs, component IDs and pad numbers. Boundaries trace to source paths; terminals retain all area elements and boundary subedges, not just a centre.
- When a domain is contributed exclusively by one pad, its existing domain geometry is the terminal. Globally empty drill/cutout boundary rings are also reused. Other terminal boundaries are clipped against the domain before noding. Spade's `add_constraint_and_split` resolves remaining crossings into compatible subconstraints before refinement. No net-based stitching or finding suppression is performed.
- Source and union snapshots can differ by the existing boolean adapter's bound `q`. Only terminal constraint vertices may align to fixed domain boundaries within `4q + 16*coordinate_scale*1e-12` mm. The maximum displacement is `terminal_alignment_mm`. Copper boundaries themselves are not shifted. Original/subedge coverage is subsequently checked; a lost constraint rejects the domain, rather than silently removing the feature.
- Constraints are subdivided to a length of at most `sqrt(2*max_area_mm2)` before refinement. Spade refinement then keeps constraint edges fixed. Its result flag does not certify requested quality: actual maximum element area and minimum angle are checked again. The fixed-boundary choice avoids repeated encroachment refinement of nearly coincident CAD contours. It can leave acute/poor elements and does not promise an unconstrained Delaunay mesh.
- Every returned triangle is finite, strictly positive in algebraic XY orientation and checked against the whole domain polygon. Full coverage uses robust DE-9IM, with a fallback difference-area allowance of `(8q + arithmetic allowance)*triangle_perimeter` because that boolean check itself uses the bounded adapter. Open-edge provenance, all original constraint chains, total area and a single edge-connected component are checked. A failed check clears its nodes/elements/terminals and mesh area; source/area/readiness/reason remain visible. No tiny component or short finding is filtered out.
- Default mesh parameters are area **1 mm²**, requested minimum angle **10°**, **500000** nodes, **1000000** triangles, **8** refinement calls. Each call inserts at most 4096 additional vertices; insertion and intersection/subdivision checks also enforce remaining budgets. Allowed ranges are area `1e-8..1e6`, angle `0..30`, nodes `3..2000000`, triangles `1..4000000`, calls `0..64`. Angle zero explicitly disables the quality target; it does not certify solver conditioning. Bounded failure is `mesh_unbuilt` or `refinement_incomplete`, never an empty complete mesh.
- Known drill/contact topology ambiguity blocks `topology_confirmed`. Missing/excluded copper, unknown/stale/unavailable CAD fills and unknown independent-hole data block `copper_complete`; an unconfirmed pad-drill span conservatively blocks copper readiness across the job because its affected layers are not established. This last case has its own coupon: an inner-layer pad cannot be declared ready merely because an unrelated top pad caused only a global `partial`.
- Global invalid import/options/profile yields no ready domains or barrels. Otherwise independent built domains survive a partial report. Unknown net retains copper; physical shorts and input diagnostics remain in the report. `complete` is not a universal topology guarantee near geometric tolerance and is not electrical accuracy/convergence.

Spade's documented precision and refinement limitations are relevant to this boundary: [CDT and splitting](https://docs.rs/spade/2.15.1/spade/struct.ConstrainedDelaunayTriangulation.html), [refinement parameters](https://docs.rs/spade/2.15.1/spade/struct.RefinementParameters.html). No alternative mesher or solver dependency was added.

## Physical profile and coefficients

`profile.stackup` is an ordered array of `{kind:"copper",layer,thickness_mm}` / `{kind:"dielectric",name,thickness_mm}`; `materials` maps actual layer names to `{conductivity_s_m,state}`. `barrel` optionally supplies `plating_thickness_mm`, the same material record and `diameter_convention`. Unknown/duplicate/reversed layers, nonpositive/nonfinite dimensions, adjacent copper entries with no explicit dielectric separation and invalid material state/conductivity are diagnosed. Explicit profile stackup/plating override board stackup/plating; unprovided values stay absent. No oz fallback, conductivity catalogue, dielectric or plating default is introduced. Resolved values and `board`/`profile` origins are retained. Incomplete stack order does not produce guessed Z values.

For Z, copper contacts lie at layer midplanes. All SI conversions live in `profile.rs`:

- Sheet coefficient: `sigma[S/m] * thickness[mm] * 1e-3`, in siemens.
- Finished-hole convention: `A = pi*((d/2+t)^2-(d/2)^2)` in mm², then `*1e-6` to m².
- Before-plating convention: `A = pi*((d/2)^2-(d/2-t)^2)`; a closed/nonpositive or unrepresentable bore is invalid.
- Adjacent participating-layer barrel segment: length is the midplane Z difference `*1e-3` m; prepared coefficient is `sigma*A/length` siemens. This is a local coefficient, not a solved board resistance.

Only geometry-confirmed contacts in the confirmed span participate. No XY/node collapse occurs. Physically absent intermediate-layer contact is not invented; segments can span that layer's Z position. A geometry-confirmed contact lost during meshing instead blocks full barrel readiness. NPTH creates no barrel. Plated slots keep planar copper/cutouts and are explicitly unsupported for the interlayer model (no circular substitute section). Unknown span, missing plating/conductivity/diameter convention/Z and unsupported slots keep affected domains/barrels unready. Underflow/overflow in prepared coefficients is invalid, rather than a zero/ideal conductor.

## Runtime and reports

`prepareAnalysisConduction(boardJson, options?)` is the only new binding in the existing production addon. `runAnalysisProblem({kind:"prepare-conduction", boardJson, geometry?, mesh?, profile?})` uses the existing FIFO/process worker and admission deadline/abort/shutdown. Nested options/profile are validated and deeply snapshotted on admission. No queue, handle, cache, GPU path or DSL was added.

The report contains mesh arrays once, terminal/barrel mappings, profile, provenance, findings and diagnostics. It contains no embedded GeometryReport. Timers separate geometry and conduction/material preparation; whole-job worker/IPC time and actual Node compact JSON bytes are measured by the fixture helper. `serialized_bytes` is the compact **Rust** encoding size, obtained with a counting writer; JS number formatting can change the actual Node byte count.

`tests/pcb-analyze/*/mesh.ts` reuse the existing named launcher. Runs save detailed `mesh.json`, a smaller `report.json` with per-domain readiness/counts/reasons, and per-layer mesh/constraint/terminal SVGs. There are no demonstration material assumptions in these eight runs: profile is absent, area is 1 mm² and minimum angle is explicitly zero for a diagnostic geometric mesh. This is not a measured stackup and not a validated physical model. Missing physics/freshness stays missing.

## Original stage verification commands (before M1/M2 corrections)

From the Backend repository root (PowerShell):

```powershell
$env:PCB_ANALYZE_CONDUCTION_REPORT_DIR = "$PWD/debugging/conduction-mesh/rust"
$env:PCB_ANALYZE_RUST_REPORT_DIR = "$PWD/debugging/conduction-mesh/geometry-rust"
cargo test --release --locked --manifest-path native/analyze/Cargo.toml -- --nocapture
npm run native:build
npm run typecheck
npm run build
$env:PCB_BLOCK_BACKEND = 'cpu'
$env:PCB_BOARD_BACKEND = 'cpu'
$env:PCB_POST_PLACE_BACKEND = 'cpu'
node --import tsx --test --test-concurrency=1 tests/pcb-native-addon.test.ts tests/pcb-analysis-conduction.test.ts tests/pcb-analysis-geometry.test.ts tests/pcb-analysis-runtime.test.ts tests/pcb-board-packer-native.test.ts tests/public-api.test.ts
node --test scripts/test-pcb-analyze.test.mjs
npm run test:package
npm run test:pcb-analyze -- --all --name mesh
node scripts/check-pcb-analysis-mesh.mjs
node debugging/conduction-mesh/compatibility.mjs
node debugging/conduction-mesh/coupons.mjs
python debugging/conduction-mesh/check-geos.py
python debugging/conduction-mesh/plot.py
cargo fmt --manifest-path native/analyze/Cargo.toml --check
git diff --check
```

The GEOS/plot scripts are local evidence tools, not package/Rust runtime dependencies. Reproduce standalone JSON preparation with `cargo run --release --locked --manifest-path native/analyze/Cargo.toml --example conduction -- <board.json> <report.json> [options.json]`.

## Original stage check results (before M1/M2 corrections)

Final check results and measurements are recorded below after the last build. This section is not an acceptance certificate; no commit or DC stage is performed.

- **50 Rust tests passed**: 16 conduction (including the eight-board corpus), 21 unchanged geometry and 13 importer tests. The supplied R1 geometry regression is unchanged.
- Additional targeted plating-origin assertions passed: board `50 um` resolves to `0.05 mm`; explicit `0.025 mm` overrides it and records profile origin (`plating-origin-test.log`).
- **47 Node tests passed**, zero skips: exact native exports/versions, conduction, geometry, runtime, native board packer and public API together under CPU selectors. Active meshing abort, deadline, shutdown, deep profile snapshot and subsequent fresh jobs passed.
- `native:build`, `typecheck`, `build`, formatting/whitespace checks and four launcher/hash/inventory tests passed. The installed archive really called the new operation in its packaged process worker and typechecked its declaration; normal, GPU-disabled and unavailable-driver smoke modes passed.
- All eight named mesh runs completed; complete Rust/native-worker semantic reports match (only clocks and encoded byte counts excluded). The previous eight full geometry reports and eight legacy full import reports are unchanged. The short CPU replay solution/quality match the saved baseline exactly, including after a conduction call; F32 contract remains `f32-rte-ftz-v1`, rank hardCount=0, hardSeverity=0, score=772.9915771484375.
- Independent JS checks read actual elements/edge incidences, sample coverage with arithmetic boundary handling, verify captured native narrow tracks/pad terminals, via spans/bore boundaries and native fill voids on BandAmpV5 and ESPower. Three ray samples landed within floating-point boundary roundoff; distances/areas are recorded, not silently counted as strictly interior.
- Independent local GEOS verification checks all **347741 returned triangles**: union coverage/outside area and overlap, using a coordinate-scale arithmetic allowance (no curve-tolerance allowance). Details and exact input hashes are in `geos-check.json`. Rejected domains are not included in that certificate.

Native binary SHA-256: `be1606422eccd74400b6318add284f57deae4f1d14602649f741c4809c6631f5`.

| Fixture | Built / all domains | Nodes | Triangles | Geometry s | Conduction s | Worker/IPC s | Detailed MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| BandAmpV5 | 8 / 8 | 10180 | 11942 | 0.476 | 1.924 | 2.691 | 1.85 |
| ESPower | 93 / 104 | 19680 | 20646 | 2.292 | 3.285 | 6.017 | 3.12 |
| NanoScope | 454 / 622 | 96899 | 106870 | 34.252 | 66.628 | 102.240 | 14.68 |
| NanoSpec | 213 / 253 | 48815 | 53932 | 14.276 | 54.007 | 68.999 | 8.49 |
| STM32G0_CAN_RS485 | 115 / 154 | 15211 | 16928 | 0.734 | 1.718 | 2.761 | 2.37 |
| TPS63060DSCR | 11 / 12 | 5004 | 6596 | 0.120 | 0.258 | 0.588 | 0.80 |
| ThunderF722 | 733 / 760 | 110169 | 116386 | 12.528 | 24.472 | 38.359 | 16.12 |
| USB_PD_Trigger_5A | 33 / 38 | 11609 | 14441 | 0.677 | 1.317 | 2.266 | 1.92 |

These are observed local timings, not a controlled cross-version benchmark. Worker/IPC includes import, preparation, native report conversion/transport/startup; helper JSON formatting/writes and SVG generation follow it. Native stage timers exclude counting-writer/report transport costs.

**1660 / 1951 domains are built; 291 are explicitly unbuilt. All eight board reports remain `partial`, with zero ready domains.** Unknown physical profiles and CAD freshness alone already prevent claiming eight DC models. Failed constraints and incomplete refinement are additional, separate limitations. Their source paths and reasons are in the evidence, not hidden behind aggregate status.

Fully ready physical examples are `ready-strip-quality.json` (8 mm², requested angle 20°, measured minimum 33.69°, maximum element area 0.5 mm², sheet coefficient 2030 S), `ready-annulus.json` and `ready-barrel.json` (four distinct layer domains and three finite barrel segments). Annulus/barrel examples explicitly disable the minimum-angle target; their measured minimum is about 2.499°. Their material state, thickness/conductivity/plating and diameter convention are scenario inputs, not measured board facts. `r1-coarse.json`, `r1-0.0001.json`, and `r1-fine.json` retain all three outcomes described below.

Artifacts (repository-local, ignored):

- [Machine evidence and exact run paths](../../../debugging/conduction-mesh/evidence.json), [GEOS coverage](../../../debugging/conduction-mesh/geos-check.json), [compatibility comparison](../../../debugging/conduction-mesh/compatibility.json).
- [Ready physical coupon](../../../debugging/conduction-mesh/ready-strip-quality.json), [plated barrel coupon](../../../debugging/conduction-mesh/ready-barrel.json), [coupon overlays](../../../debugging/conduction-mesh/coupons.png), [real board / via overlays](../../../debugging/conduction-mesh/BandAmpV5-mesh.png).
- [Full diff, including new files](../../../debugging/conduction-mesh/conduction-mesh.patch), [changed-file list](../../../debugging/conduction-mesh/changed-files.json).
- `debugging/conduction-mesh/{rust-final-tests,node-tests,native-build,typecheck,build,package,launcher,launcher-tests,artifact-check,compatibility,geos-check,plots}.log` retain commands/results. `rust/` contains the full standalone conduction corpus; `geometry-rust/` contains unchanged geometry reports.

Changed files:

- `CHANGELOG.md`
- `docs/architecture-map.md`
- `docs/experimental/analyze/conduction-mesh-verification.md`
- `native/analyze/Cargo.lock`
- `native/analyze/Cargo.toml`
- `native/analyze/README.md`
- `native/analyze/examples/conduction.rs`
- `native/analyze/index.d.ts`
- `native/analyze/src/conduction/mesh.rs`
- `native/analyze/src/conduction/mod.rs`
- `native/analyze/src/conduction/profile.rs`
- `native/analyze/src/lib.rs`
- `native/analyze/tests/conduction.rs`
- `native/node-addon/Cargo.lock`
- `native/node-addon/README.md`
- `native/node-addon/src/lib.rs`
- `scripts/check-package.mjs`
- `scripts/check-pcb-analysis-mesh.mjs`
- `src/analyze/README.md`
- `src/analyze/analysis-pool.ts`
- `src/analyze/run-analysis.ts`
- `src/analyze/run-analysis.worker.ts`
- `src/analyze/types.ts`
- `tests/helpers/pcb-analysis-mesh.ts`
- `tests/pcb-analysis-conduction.test.ts`
- `tests/pcb-analyze/BandAmpV5/mesh.ts`
- `tests/pcb-analyze/ESPower/mesh.ts`
- `tests/pcb-analyze/NanoScope/mesh.ts`
- `tests/pcb-analyze/NanoSpec/mesh.ts`
- `tests/pcb-analyze/README.md`
- `tests/pcb-analyze/STM32G0_CAN_RS485/mesh.ts`
- `tests/pcb-analyze/TPS63060DSCR/mesh.ts`
- `tests/pcb-analyze/ThunderF722/mesh.ts`
- `tests/pcb-analyze/USB_PD_Trigger_5A/mesh.ts`
- `tests/pcb-native-addon.test.ts`

## Limitations retained

- Some real domains are explicitly unbuilt due to terminal constraint-chain loss, disconnected triangle components, zero-area triangulator faces or invalid union boundaries. Detailed source paths/reasons are retained in `evidence.json` and each report. These are failed supported-boundary checks, not successful meshes. No source/capture was edited to make a run pass.
- Even built diagnostic meshes can miss requested size/refinement targets. With angle target zero, very skinny positive-area elements remain visible (including areas below `1e-15 mm²`); no tiny features are deleted to improve statistics. They are not a claim of suitability for a well-conditioned DC solve. Use an explicit quality target and later electrical convergence checks.
- The R1 coupon at `0.0001 mm` resolves the geometry ambiguity but one domain can still fail the edge-connected mesh check. At `0.00001 mm` both domains are built/ready with the explicit coupon profile. Coarse `0.001 mm` has no ready domain. The original geometry regression test was not changed or weakened.
- The single-geometry model's accepted approximation limit remains: no general topology proof near tolerance and no reconstructed fills. CAD source/capture differences (via dimensions, fragment IDs, sequential capture and unknown freshness) remain exactly those documented in copper geometry verification.
- Plated slots/unknown drilled spans and absent physical data are not completed by inference. Contacts/elements are prepared for a future DC solver; loads, boundary conditions, matrix assembly, solves, physical error estimation, refill, temperature dependence, anisotropy, GPU and cache remain outside this stage.
- Tested locally on Windows x64 / Node 26.5.0. Linux/macOS, other Node versions, remote CI and a full repository `npm test` were not rerun. Existing targeted public API/placement tests, installed-package fallback modes and short CPU replay are the compatibility evidence; no claim of new GPU numerical benchmarking is made.
