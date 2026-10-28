# P7 direct-FEM status — 2026-10-28

Current contract: [single-method v1](next-task-p7-single-method-v1.md).
One existing direct sparse FEM local candidate, CPU/F64, shared **300 s / 2 GiB**,
physical observable error at most5%. Speed optimization is currently deferred.
P7 is **open**; extracted-board P8/P11 dependencies are **not ready**.

## Actual 300 s run

The unchanged STM32G0_CAN_RS485 via/actual-return ROI used
`entity:6:8:16:3 train`. A single monotonic start feeds remaining time to builders,
full solves and P6; the former per-solve30s clamp was removed. No method, grid,
polynomial configuration or physical guard was changed. A separate supervisor
bounds the complete process family to300s/2GiB, including uninterruptible factor
calls. This is a diagnostic example, not a worker factory.

**Exit1 at220.2373207s; peak family RSS1025572864 bytes (978.06 MiB).**
The supervisor did not terminate it: the original numerical guard rejected the
opposite drive on the first1kHz holdout:
`LOCAL_VOLUME_RESIDUAL: full KCL=1.1087457960519757e-16 KVL=1.2482831421062513e-7`.
KVL limit remains1e-8; it was not replaced by the physical5% tolerance.

All four10k/100k training solves completed (7,7,12,12 iterations). P6 built28
voltage/35 current modes and returned its original17-frequency impedance call;
two individual1kHz full holdouts passed before the opposite drive refused. The
example emits its final report after all holdouts, so no complete model/response
artifact was returned. No successful cold result, warm benchmark or physical
observable error table is claimed. No spatial refinement or configuration search
followed this refusal. Worker integration remains gated on physical acceptance.

Evidence: `debugging/p7-board-integration/single-method-300s-v1/direct-fem/`.
Small exact run metadata and the unchanged request are committed under
`tests/fixtures/analysis-local-extraction/board-integration/single-method-300s-run.json`,
`single-method-300s-plan.json` and `single-method-via-request.json`. The300s plan
records the new policy; observable floors/configuration are inherited unchanged
from the earlier predeclared baseline plan. Executable SHA256:
`f19c61bf1789fe19b13b94a8230d9054975637ae744b78caf52b1140b03287e4`.

## Remaining completion matrix

| Obligation | Actual state |
| --- | --- |
| P7a/P7b sections/RLGC, accepted P6 classes | Preserved |
| Direct local FEM only | Alternative production modules/dispatch removed |
| Shared lifetime budget | Diagnostic300s start/remaining budgets plus family supervisor; worker path absent |
| Real ROI physical observable error <=5% | Not demonstrated; hard KVL refusal precedes admission |
| Both ports/opposite drive, original17 frequencies | P6 sweep computed internally; full opposite-drive check refuses at first frequency |
| Regional/integrated-current/power error tables | Not delivered |
| Geometric/domain independent convergence | Not admitted; same-grid agreement is insufficient |
| Actual board -> worker -> direct DSL | Real compiled-worker scenario refuses SECTION_UNSUPPORTED; extraction not delivered |
| Lines/transitions/PDN/reduction/loss completion matrix | Incomplete |
| STM32G0_CAN_RS485 / TPS63060DSCR / BandAmp accepted extraction DSL | 0/3 |
| Final accepted geometry/current/loss/frequency visualizations | Not delivered; historical diagnostics retained |
| P8/P11 extracted-board readiness | Not ready; no implementation of those stages |

## Retained and retired code

`volume_magnetic` uses direct sparse factorization only. `board_volume` accepts
only direct entity FEM and geometry/field/full/train modes. The existing sparse
`Field`/`ElectricApply` solves requested original-space RHS; full C is no longer a
prerequisite. Complete-C checkpoint loading is a diagnostic option. P6 receives
remaining memory after conservative concurrent reservations, not a fixed512MiB.

Tensor/tensor_potential/sparse_iterative candidates were removed. Independent
integral/surface/cache/matrix references live only in `native/analyze/tests/reference/local`.
Shared geometry/current/field/arithmetic helpers remain. Initial inventory and
pre-cleanup source copies remain in ignored `debugging/p7-single-method-audit/`.

Dependency review found73 unused untracked preparation/reference experiment
sources (including the unused factor-server example); exact bytes/hashes were
archived under `retired-preparation/` before removal from the working tree.
36 generated pyc files were removed after verifying workspace paths. The only
name matches in tracked callers were unrelated same-name current-space and
structural-gauge scripts, not dependencies on the retired directory. Committed
frozen-fixture generators and the actual-board via/return DSL input remain; its resource limits and frequency count follow the current contract. Historical ledger paths are historical;
producer snapshots/results were not rewritten. User `native/analyze/debugging/`
was not changed and no ignore rule was added to hide it.

## Checks and historical measurements

Previous focused validation:56 Rust tests, typecheck, examples/reference cargo
check. Direct frozen source3 energy error0.309465% plus inherited empirical
reference uncertainty0.678064%=0.987529%; this is a primitive check only.
Final ordinary `npm run native:build` passed after all Rust/reference moves in
9m37s. `npm run build` rebuilt the compiled workers and documentation assets;
`npm run typecheck` passed. All31 fresh DSL tests passed, including via contact
creation and missing-via refusal through the actual compiled process worker.

P6 validation passed4 unit tests and26 integration tests. The conditioning test
uses every exact original17 frequency, both ports individually, opposite drive,
and repeated load changes; corrupt cached coordinates refuse against original
physical residuals. Structural gauges and existing energy negatives also pass.
These regressions do not turn the failed real-board calculation into admission.

The preserved real-board `real-via-return-scenario.js` was actually submitted to
the freshly rebuilt worker: it refused in0.2869661s with `SECTION_UNSUPPORTED`
(cross-section supports explicit a.line; layered channel extraction absent).
`single-method-real-worker-refusal.json` binds its board/scenario/worker hashes.
No authored equivalent circuit or dummy extraction output was used.

| Historical cold run (not current deadline) | Seconds | Peak family RSS bytes | Outcome |
| --- | ---: | ---: | --- |
| Direct FEM/full C,120s policy |120.0787639|1025654784|Wall refusal |
| Tensor candidate,120s policy |111.4839997|94924800|Internal deadline; removed candidate |
| Direct requested RHS,120s policy |120.0989665|1025327104|Wall refusal |

These records remain immutable. The viewed historical timing comparison is
`debugging/p7-single-method-audit/cold-run-comparison.png`; it labels both direct
120s timeouts and is not a physical accuracy plot.

Reproduction (compilation separately; run inside one300s/2GiB family supervisor):

```powershell
$env:CARGO_BUILD_JOBS='1'
cargo build --release --manifest-path native/analyze/Cargo.toml --example board_volume
python tests/fixtures/analysis-local-extraction/board-integration/single-method-run.py unique-packet-name
```

The retained supervisor is single-method only and never overwrites an existing
packet. It enforces300s/2GiB for the whole child family. AST validation passed;
the recorded300s run used its same fixed configuration and cap.
The viewed diagnostic `debugging/p7-single-method-audit/direct-300s-refusal.png`
shows actual wall/RSS and the unchanged KVL guard, explicitly without physical
acceptance. Its plot does not substitute for missing current/loss/frequency maps.
