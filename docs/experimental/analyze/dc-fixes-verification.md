# DC fixes: D1/D2/D3 and directly authored DSL

2026-10-22. **Implemented and verified; independent acceptance pending. No commit.**
Task: [dc-fixes-task.md](dc-fixes-task.md). The pre-fix [implementation report](dc-analysis-verification.md)
and [review](dc-analysis-review-2026-10-22.md) remain historical evidence.

## Review matrix

| Requirement | Correction | Saved regression / evidence | Result |
| --- | --- | --- | --- |
| D1: unused descriptions change DC | Only current sources/loads/resolved IC assignments/explicit elements activate native terminals. Passive first-surface area integration reads existing FEM without `join_all` or selecting domains. | `tests/pcb-analysis-dc-ports.test.ts`: ordinary/aggregate/unready descriptor invariance; unsupported active aggregate still rejected; wide tap; bore and missing surface coverage; separate gauges; standalone conductor. | All pass; same fields, losses, active terminals, participating domains and reusable factor before/after unused declarations. |
| D2: new domains lose geometry | Compact mesh reference requires every computed domain to exist in its compatible target. Otherwise retain the current compact mesh and update the geometry owner. Native preparation/matrix reuse remains separate. | Same test file: A → B → B raised → A+B → A+B raised, after JSON round-trip, immutable first result, resolved field/mesh dimensions and plots. | All pass. B owns its mesh; raised B references B. A+B owns its expanded mesh; next RHS references A+B. Changed contacts invalidate matrix. |
| D3: corpus assertions were ignored files | `tests/helpers/pcb-analysis-dc-audit.ts` validates the current serialized output and explicit expectations. Named launcher saves results and audit failure reason before returning nonzero. | `tests/pcb-analysis-dc-audit.test.ts`: 31 positive/negative validator tests; baseline audited offline; all new real-board runs audited by their launchers. | Pass. No search for an earlier successful output, no solver inside validator. |
| Direct examples | Eight authored `dc.js` files; exact text/filename passed to existing production runner. Thin `dc.ts`; test-only expectations JSON. | All eight named launches, exact saved scenario bytes/hash, normalized physical-input comparison; `--list` lists only four actual launchers. Cancellation regression reads the authored NanoScope file. | Pass. Old scenario interface/files and string generator removed. |

The initial pre-fix regressions reproduced the wide-tap field change, standalone
aggregate refusal and missing B mesh. The unready-layer test initially needed a
matching two-layer stack in its test setup; this was corrected before successful
regression runs. No production tolerance/readiness rule was changed.

## D1 measurement rule

An active pad returns its native equipotential electrode value. A passive
single-pad side is the area-weighted integral of the existing piecewise-linear
voltage over all prepared copper terminal triangles on its first physical surface.
Triangle mean = mean of its three node potentials, weighted by triangle area.
Bores are absent from these triangles. All prepared surface parts must be computed
and their terminal area covered, with a common mathematical reference. No point-node
fallback, added conductivity, selected region, second layer fallback or hidden solve.
Aggregated observations, missing/uncomputed/partial coverage and incompatible gauges
remain unavailable with a reason. A port declared after solve can observe the same
immutable field. Source/load aggregate electrodes remain explicitly unsupported.
The semantics are documented in `spec-doc.d.ts` and the DSL reference.

The linear wide-tap coupon reads 0.999014778325 V and retains a potential gradient
across the pad instead of shorting its 4 mm length. The bore regression checks
remaining contact area; deliberately incomplete coverage stays unavailable.

## Preservation of physical inputs

Baseline was captured **before edits**, using existing completed outputs, hashes,
requests and scenarios in `debugging/analysis-dsl/dc-fixes/baseline.json`. No old
corpus rerun was used to manufacture a baseline. Its eight reports passed the saved
audit offline after an explicit in-memory adaptation of the old log/check labels;
the historical JSON files were not rewritten.

`compare.py` / `compare.log` / `corpus-matrix.json` in that directory compare all
32 new cases to those exact files. They assert equality of normalized full DSL
physical configurations, full native elements/profile/mesh/limits requests and
resolved IC pin-current distributions. Only incidental port/context IDs and the
new case display name are excluded. Ports are resolved to their actual pad-pair
descriptors before comparison, not counted. Measurements and references retain
identity. F1 remains 0.02 Ohm; all rails, sources, return pins, currents, stack/material
hypotheses, meshes and runtime/native budgets are unchanged, including Thunder's
six-layer stack.

**All eight real scenarios already measured active source/load pads.** Consequently
no observation-only electrode was removed from these particular native requests;
the matrix records `removedObservationElectrodes: []` for every case. Every measured
voltage and total conductor loss agrees exactly with baseline. D1's physical change
is demonstrated by the independent wide-tap/unused-port coupons, not by inventing
numerical changes in this corpus. No golden value was forced by changing physics.

No PCB JSON, capture/evidence/manifest, native project/source, exporter, Rust solver,
addon binary or placement/F32 implementation was edited in this correction turn.
The saved capture-integrity test had retained the old raw-hole count; it now strictly
matches preserved drill IDs and the exact per-pad committed exporter warnings for
the 12 historical API holes, rather than requiring those known false holes again.

## Current real corpus

One final full production corpus run after D1/D2 and authored migration:
**8/8 scripts, 32 complete native results, no failed sweep attempts or unknown
measurements/checks**. Every expected case/consumer exists; all computed domains have
finite fields and resolvable geometry/plot references. Accepted snapshot conditional
validity remains visible. Guards: residual ≤1e-8; KCL ≤1e-12 A OR normalized ≤1e-8;
power error ≤1e-15 W OR normalized ≤1e-8. Each quality value is independently finite.

For each observed port at both load levels, relative drop/loss convergence is ≤2%.
Floors stay separately 1e-9 V and 1e-9 W. Conductor loss = copper + finite barrel loss,
excluding external load power. The largest normalized KCL is 3.5527e-14.

| Fixture | Cases | Largest port drop change | Conductor loss change | Four-case wall s | dc.json MiB |
| --- | --- | ---: | ---: | ---: | ---: |
| BandAmpV5 | 4 complete / audit pass | 0.632% | 0.633% | 14.092 | 14.40 |
| ESPower | 4 complete / audit pass | 1.003% | 0.979% | 24.246 | 18.48 |
| NanoScope | 4 complete / audit pass | 0.392% | 0.392% | 261.852 | 60.73 |
| NanoSpec | 4 complete / audit pass | 0.382% | 0.086% | 181.474 | 82.22 |
| STM32G0_CAN_RS485 | 4 complete / audit pass | 0.680% | 0.610% | 12.210 | 16.80 |
| ThunderF722 | 4 complete / audit pass | 1.491% | 1.401% | 85.033 | 77.94 |
| TPS63060DSCR | 4 complete / audit pass | 0.847% | 0.839% | 5.145 | 6.87 |
| USB_PD_Trigger_5A | 4 complete / audit pass | 0.295% | 0.550% | 9.450 | 15.17 |

Wall times include four calls, serialization, audit and HTML. These are local
verification timings with some independent package/tests running concurrently,
not a controlled performance benchmark. Native/runner budgets were not increased.
Memory remains sampled RSS/planned allocation, not a hard OS ceiling.

USB keeps the same 0.100 V limit on each mesh: nominal 2 A passes, raised 4 A fails,
then the same case's loss check and three maps are present. Fine drops remain
0.086607280868 / 0.173214561736 V. Expected check failures are preserved requirements
outcomes, not rejected native calculations. Thunder has finite nonzero real barrel
segment currents in every case; missing barrel lists cannot pass the audit.

Current implementation fingerprint: `7bedca5813987237df86e12f2fca86d34a89d30f133233f62b77e1340f8555f3`.
Native SHA256: `088324da0ff23e41ebc72c7103aceb17d427ac6a6bb61b6ee26e5381e7d90ddd`
(identical to baseline). Every result includes exact authored script path/hash,
expectations hash, board hash, full `implementation.json`, native hash and per-case
physical-input/profile hashes. No external model/profile resources are used; all
explicit hypotheses are in the hashed script and recorded configuration/request.

## Commands and results

From backend root:

```powershell
npm run typecheck
npm run build
$env:PCB_BLOCK_BACKEND='cpu'
$env:PCB_BOARD_BACKEND='cpu'
$env:PCB_POST_PLACE_BACKEND='cpu'
node --import tsx --test --test-concurrency=1 tests/analysis-dsl.test.ts tests/pcb-analysis-dc.test.ts tests/pcb-analysis-dc-ports.test.ts tests/pcb-analysis-dc-audit.test.ts
node --import tsx --test --test-concurrency=1 tests/pcb-analysis-conduction.test.ts tests/pcb-analysis-geometry.test.ts tests/pcb-native-addon.test.ts tests/public-api.test.ts
node --test scripts/test-pcb-analyze.test.mjs
cargo test --release --locked --manifest-path native/analyze/Cargo.toml --test dc --test conduction
npm run test:package
npm run test:pcb-analyze -- --list
npm run test:pcb-analyze -- --all --name dc
```

- **77 focused Node tests pass**, including M1 unit safety, cancellation/deadline
  of real native DC and recovery, old load rules, D1/D2 and 31 audit tests.
- **17 compatibility Node tests pass**: geometry/conduction, strict addon exports
  and public APIs. **4 launcher/capture-integrity tests pass**. Total distinct
  Node/script tests in these groups: **98**, no failures/skips.
- **32 release Rust DC/conduction tests pass** (12 + 20), including strict/snapshot,
  M1/M2 contact readiness, budgets, material/matrix invalidation and numerics.
  Rust/addon code was unchanged: native rebuild was not necessary for these fixes.
- Typecheck/build pass. Installed package smoke passes all three standard modes
  with actual board FEM, passive observation and unused aggregate port, plus
  preserved placement/worker APIs. Consumer:
  `C:/Users/kiril/AppData/Local/Temp/eda-backend-package-j2gfPw`.
- Eight final named scripts pass saved audit. Baseline offline audit and full
  normalized before/after comparison pass. `git diff --check` passes.

Logs: `debugging/analysis-dsl/dc-fixes/`: `focused-final.log`, `ports-final.log`,
`audit-tests.log`, `compatibility.log`, `launcher-tests-final.log`, `rust-guards.log`,
`typecheck-final.log`, `build-final.log`, `package.log`, `list.log`, `corpus.log`,
`baseline-audit.log`, `compare.log`. The initial launcher log records its stale
raw-hole expectation failure; the corrected strict check passes in the final log.

## Artifacts and visual QA

- [Current eight-board overview](../../../../debugging/pcb-analyze/dc/index.html).
  Each linked run contains exact `scenario.js`, `dc.json`, `audit.json`,
  `implementation.json`, timing/logs, self-contained HTML and thumbnail.
- [Full uncommitted DC diff](../../../../debugging/analysis-dsl/dc-fixes/changes.patch) and [review manifest](../../../../debugging/analysis-dsl/dc-fixes/manifest.json). This diff is against DSL HEAD `5b07c33` and includes the earlier DC implementation, not only this correction turn; the manifest names excluded preexisting task/review files. All 42 solver/runner/audit implementation files still match their final-run hashes. The one later renderer-only edit is independently recorded by `rendering.json` (see below).
- [Current corpus matrix / hashes / before-after comparison](../../../../debugging/analysis-dsl/dc-fixes/corpus-matrix.json).
- [Baseline provenance](../../../../debugging/analysis-dsl/dc-fixes/baseline.json)
  and [offline baseline audit](../../../../debugging/analysis-dsl/dc-fixes/baseline-audit.json).
- [Synthetic D2 report](../../../../debugging/pcb-analyze/dc-fixes-d2/index.html)
  (A/B/combined domain switching; not a real corpus member).
- [Screenshots and QA notes](../../../../debugging/pcb-analyze/dc/screenshots/dc-fixes/README.md).

Actual browser views verified STM32's MCU/CAN/RS485 table and checks, USB nominal /
high-load maps and pass→fail→following pass, Thunder TOP and INNER_1 computed maps,
and synthetic D2 B/combined A+B maps. B is coloured with A gray in B-only mode;
both appear in the combined mode. Copper, bores and active source/return labels
use the same absolute-mm/Y-down frame. Shared scales remain enabled across loads
and meshes; legends state the layer/field units. Missing domain geometry is an
explicit invalid report, not silently omitted in rendering. Excluded copper is gray.

A final HTML-only correction makes `Numerical quality → input` show the current
request, with `meshOwner` separately identified, rather than displaying the mesh
owner's earlier RHS. The calculation JSON and its original execution fingerprint
remain unchanged. All eight saved results were re-audited and their HTML regenerated
offline without another FEM solve. Each run's `rendering.json` records the current
renderer hash and the unchanged `dc.json` hash; the HTML includes that provenance.
`rerender.log` and `debugging/analysis-dsl/dc-fixes/rendering.json` record this step.
Thus the original 43-file execution snapshot remains historical truth, while 42
solver/runner/audit files match current source and the one presentation edit has its
own current hash. Physical input/result comparison remains exactly equal.
[Renderer integrity](../../../../debugging/analysis-dsl/dc-fixes/renderer-integrity.json)
checks the current-request expression/provenance in all eight HTML files and unchanged
calculation JSON bytes. A subsequent `file://` browser recheck was blocked by the
browser protocol policy; no workaround was attempted. The actual rendered maps,
mode/layer switching and tables had already been verified before this one-line
hidden quality-panel input correction, which does not change canvas rendering.

The new scope is JS/TS assembly/observation, report references, authored fixtures and
verification. F64 Rust solver, sparse QR, tolerances, strict snapshot/readiness,
finite barrels and runtime isolation remain unchanged. Full `npm test`, Linux/macOS
and remote CI were not rerun. Real boards use explicit test material/stack/current
hypotheses with unknown CAD fill freshness; convergence does not certify original
CAD fidelity, peak-J accuracy or product current ratings. Other analyzers remain
outside this correction. No automatic commit and no next stage.
