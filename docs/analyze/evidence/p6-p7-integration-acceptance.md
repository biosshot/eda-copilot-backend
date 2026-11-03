# P6/P7 integration acceptance ledger — 2026-10-29



Base HEAD `2366190f363a25b10e4b744767f39f9a804dc53c`. Initial status: parent-authored modified `p7-to-p8-p11-plan.md`, two untracked next-task documents, and user-owned `native/analyze/debugging/`. These are preserved. Active assignment: [integration/cache](../archive/tasks/next-task-p6-p7-integration-cache.md). The refined electric v1 candidate is accepted; historical 88.8% is not an integration veto.



The initial library-only checkpoint below is retained as history. Current status is the [final obligation matrix](#final-obligation-matrix) and final-binary section: full original51-drive verification, real integration/cache measurements and finite remaining physical scope. Whole P6/P7 closure requires the complete matrix, not a successful cache hit.



## Original obligations and implementation inventory



| Obligation | Existing code/evidence | Initial status / remaining gap |

|---|---|---|

| P6 DC/AC/transient MNA; initialization/error control | electrical core/completion verification; electrical solve/ac/waveform | implemented+tested for declared linear class |

| P6 supply/load/driver/receiver and PartUuid/local model resources | electrical-models.ts, electrical-core-adapter.test.ts | implemented+tested; parasitics require authored data |

| P6 multiport S/Z/Y/Touchstone, physical terminals, composition | frequency-model/executor, electrical frequency/coupled | implemented+tested in existing finite physical topology; arbitrary infinity joins remain implementation gap |

| P6 immutable a.model network resource | resource propagation and P6b verification | implemented+tested |

| P6 FFT/convolution, normalization/windows | electrical signal.rs / signal_tests.rs | internal implemented+tested; public integration remains gap |

| P6 temporal frequency resources | relaxation/resonant/mixed verification | implemented+tested for declared passive RC/authored resonant/real-K delay classes; arbitrary active/resonant fitting explicitly future |

| P7 numerical sections and coupled ideal lines | P7a section acceptance | implemented+tested in explicit section scope |

| P7 skin/dielectric frequency losses | P7b loss acceptance and propagation/losses | implemented+tested weak-loss quasi-TEM band; low-frequency transition/full arbitrary channel gap |

| P7 physical via/return/local PDN | board_mesh, volume_current/field/magnetic/solve/reduction | library primitives exist; orchestration previously diagnostic-only; complete actual-board validation pending |

| P7 reduction, gauges and charge recovery | volume_reduction, coupled, structural-gauges verification | coupon implemented+tested; real-ROI 17-frequency/51-drive/current/loss admission pending |

| P7 automatic board PDN/channel extraction and line/local composition | existing DSL descriptors, explicit sections and supplied networks | integration/implementation gaps; do not substitute authored equivalent circuit |

| Worker/addon/direct DSL local extraction | execute-problem, electrical-executor, cross-section-executor | integration gap; existing dispatch explicitly refuses this board path |

| Worker factors/model reuse and disk v1 | placement solve-cache is reference only | implementation gap, no extraction cache present |

| Three real boards / visually inspected reports | exact fixtures below | no new end-to-end report admitted |



## Library checkpoint



`native/analyze/src/extraction/local/board_prepare.rs` owns the former board-volume prepare/extract/train/reduce/recover orchestration. `examples/board_volume.rs` is now an argument/loading/printing wrapper, with no duplicate physics implementation. Library default electric preparation exactly matches `field-refinement-request-v1.json`: 8 mm domain, 0.5 mm vertical step, 25 um interface step, growth 2. Original current mesh/physical contacts/stack/plating/source mapping are retained. Explicit diagnostic field overrides and old complete-field checkpoint mode remain available.



`solve` performs required training at 10/100 kHz (each physical unit port), then only requested P6 frequencies. Full 17-frequency/three-drive comparison is an explicit `verify`/legacy `full` mode, never implicit in `solve`. Training remains inside the 300 s cold deadline. The 1800 s developer validation allowance remains separate. No diagnostic executable is launched by a worker/backend. The library returns candidate/recovery/evidence data; an addon/DSL binding is still missing.



Exact board inputs: `tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json`, `tests/pcb-analyze/TPS63060DSCR/TPS63060DSCR.json`, `tests/pcb-analyze/BandAmpV5/BandAmpV5.json`.



## Downstream readiness



P8: supplied passive multiport P6 resource and transient engine are available, but automatically extracted real PDN multiport with verified DC/AC/temporal applicability, source mapping and recovery has not been delivered. P8 implementation is not started.



P11: explicit coupled sections with supported losses/delays and existing driver/receiver resources are available. Full extracted channels/local discontinuity composition, return continuity and signal-wide coverage are not verified. No arbitrary USB/DDR/2 GHz RF readiness follows from this low-frequency via model. P11 implementation is not started.



Cold/warm/restart/cache stage evidence and real-board full/reduced validation: pending measured entries below. Warm/restart are N/A until real cache integration exists, not zero seconds. No new visual report has yet been generated or inspected.



## Completed checks

- `cargo check --manifest-path native/analyze/Cargo.toml --example board_volume -j1`: PASS (37.01 s after correction during extraction of orchestration).
- `cargo build --release --manifest-path native/analyze/Cargo.toml --example board_volume -j1`: PASS (7m41s). Builds are separate from runtime accounting.
- `cargo test --release --manifest-path native/analyze/Cargo.toml --test board_prepare --test volume_field_refinement -j1 -- --test-threads=1`: 9 PASS; field suite 0.18 s (build/lock 5m48s). The new check pins every accepted electric fixture setting and unchanged physical signature/current request.
- `npm run native:build`: PASS, ordinary shared addon, Cargo 7m13s; inherited compiler warnings remain.
- `npm run typecheck`, `npm run build`: PASS.
- `node --import tsx --test --test-concurrency=1 tests/electrical-core-adapter.test.ts tests/electrical-mixed-audit.test.ts`: 4 PASS, 1 SKIP (current mixed saved evidence absent).
- `node --import tsx --test --test-concurrency=1 tests/electrical-core-examples.test.ts tests/electrical-physical.test.ts tests/electrical-resource-propagation.test.ts`: 3 PASS, 1 SKIP (current physical saved evidence absent). Documentation AC/SI scenarios execute through freshly built compiled workers/addon. These regressions are supplied-network checks, not extracted-board acceptance.
- Full/reduced gates were declared before completion: 5%, direct-drive voltage floor 1e-6 V at 1 A, total loss floor 1e-18 W; coordinate/cell weak floors stay the original 1e-12 maximum amplitude/loss from the diagnostic. No numerical guard was relaxed. The packet failed before final serialization, so these observable comparisons cannot be claimed.


## Bounded actual-board developer result: NOT COMPLETE

Command (after release build):

```powershell
python tests/fixtures/analysis-local-extraction/board-integration/single-method-run.py integration-accepted-electric-v1 verify
```

Saved packet: `debugging/p7-board-integration/integration-accepted-electric-v1/direct-fem/` (`run.json`, `stderr.txt`, empty `stdout.json`). Binary SHA256 `c71c8d680907c2db4a9e445dbf29f29a11b2ab5a8b4502fdfc95d0d47d7a1cef`. Original board/request hashes are retained by the supervisor. Accepted electric preparation is now the compiled library default; no altered original ports or current mesh.

Process-family wall **2004.602565 s**, peak RSS **1040351232 bytes (992.15625 MiB)**, native exit 1: `LOCAL_DEADLINE: developer verification deadline`. Supervisor did not kill the process; its family cap remained 2100 s/2 GiB. The explicit internal validation cap remained 1800 s. This is a real failed full-verification run, not a complete 51-drive PASS and not evidence against the accepted electric convergence result.

All four training RHS completed, basis 28 voltage / 35 current modes. Logged stages: geometry 0.900791 s, current assembly 0.002400 s, harmonic basis 0.001384 s, electric assembly/factor 0.022158 s, magnetic assembly/factor 16.945776 s, initial magnetic basis 6.165877 s, full-solver preparation 0.089091 s, training 131.298318 s, reduction 42.336466 s, P6 response for legacy diagnostic 17 requested frequencies 0.111163 s. These stages sum to 197.873424 s; electric mesh preparation and orchestration are also inside wall time. Exact cold elapsed was not emitted before refusal, so the sum must not be presented as exact cold wall or a worker runtime measurement. The default library preparation reached P6 before the cold deadline; a complete worker cold calculation remains unmeasured.

**44/51 full direct drives** completed their strict numerical audits, through both unit ports at 56234.13251903491 Hz; the opposite drive at that frequency and remaining two frequencies were not completed. Final logged KCL 1.437e-15, KVL 6.251e-11, complex-power error 3.864e-16. No successful full/reduced current/region-loss audit is claimed: the inherited diagnostic accumulates comparison rows until final serialization, and that output was not reached. This output-on-success limitation is a concrete remaining evidence/recovery gap.

No second long run, changed ports, larger deadline, solver search or new convergence campaign was started. No new visualization is claimed or inspected because no final numerical packet exists. Memory/disk cache are **not implemented**, so cold-worker/warm-worker/fresh-process-restore and cache hit/miss/eviction/corruption results are **N/A**, not PASS or zero time. The three required extracted-board direct DSL scenarios remain **0/3**. This assignment is **unfinished**; the delivery is solely the tested library-orchestration checkpoint plus this refusal ledger. Failure of this developer run does not remove authorization to implement independent worker/cache/channel work; those remain explicit implementation gaps, not permission blockers.

Source checkpoint commit: `81ce6e6` (`refactor(analyze): move direct FEM board preparation into library`), no push. User `native/analyze/debugging/` remains untouched.


## Continued implementation after review (current delivery)

The earlier library-only checkpoint above is historical. The implementation now includes the following paths; final measured gates are recorded below when the acceptance packet is complete.

- Existing `a.channel` / `a.port` / rectangular `region` / `stackup` / `materials` -> `local-request.ts` -> the existing layered impedance builder -> process worker -> `AnalysisElectricalSession.prepareLocal` -> shared direct-FEM preparation -> reduced P6. No new public builder, SavedScenario wrapper, diagnostic subprocess or authored replacement circuit was introduced. The frozen STM32 request is tested for complete equality, including contact order and signal-BOTTOM / return-TOP.
- `board_response.rs` validates and restores the actual RLC model, reconstructs full cell currents/Joule losses from the retained current basis, and uses the existing P6 port-admittance operation for external loads. This operation preserves physical charge/gauge semantics; appending an electrical branch while retaining stale component charge modes was rejected during implementation. The native session retains one compiled model and its existing bounded factor cache, keyed by immutable coefficients/ports plus load state. Excitation changes reuse it; load changes and clear invalidate it. Full FEM operators/factors are reused during preparation and released after condensation; they are never serialized.
- One process-wide model LRU, shared across existing executors: 8 entries / 64 MiB serialized payload budget. Disk v1: 24 MiB entry cap, 256 MiB / 512 owned files; capacity admission and eviction, atomic unique temporary writes, SHA256 payload checksum, schema/shape/size checks and native RLC or S/passivity validation before restored use. Default location is `~/.cache/eda-copilot/local-v1`, overridable by `PCB_ANALYSIS_CACHE_DIR`. Failed preparation/validation does not create a successful entry; failed I/O remains observable and does not invalidate the completed in-memory calculation. Cancelled workers are terminated by the existing process lifecycle; stale owned temporary files are cleaned on later cache initialization.
- Local extraction identity includes the actual native binary SHA256, full captured board JSON hash, complete physical/numerical request (ROI/finite contacts/stack/material/plating), fixed supported band and temperature when represented. Excitation/load/report changes are result state. Unsupported bands are refused before field preparation. Section extraction also uses this cache with complete line/mesh/sample-grid/band and captured board identity, retaining conditional weak-loss applicability and SI units. Later frequency evaluation uses the existing P6 resource path without field assembly.
- Disk payloads retain reduced C/L/R/incidence, physical terminals/references, source mapping, geometry, current-basis/resistance recovery, explicit units, band, assumptions, core/schema versions and conditional validation status. No opaque factor dump, zero loss map or invented current distribution is emitted.
- Internal compatible DC probes operate only on an already prepared RLC model; they never invoke AC extraction. Incompatible DC charge injection remains an explicit refusal. The existing board/conductor DC dispatcher is unchanged. The local response bounds recovered output before allocation; regular requests solve only their requested frequencies. Required 10/100 kHz unit-port training remains inside cold preparation.

The direct scenarios are `tests/pcb-analyze/{STM32G0_CAN_RS485,TPS63060DSCR,BandAmpV5}/local-extraction.js`. TPS uses a deliberately authored smaller PDN ROI retaining both VIN pads and a real GND via after the wide ROI's unchanged 2 GiB refusal. This changes the separate TPS scenario, not the original STM32 regression. BandAmp independently exercises the supported captured RF straight section at 500 MHz–1 GHz and explicitly records the finite ground-via class's resource refusal. Its `section-cache.js` restore scenario evaluates that same supported section without repeating the unrelated failed local probe. No successful local model is manufactured for the rejected class.

Developer persistence is separate: `developer_checkpoint.rs` writes an identity/checksum-bound prepared basis/model/training-RHS journal and each completed full/reduced comparison. Identity binds the actual executable, complete board/request/field request and magnetic configuration. The developer checksum is FNV64 for corruption detection; the wrapper records SHA256 evidence as well. Resume reloads the prepared model, rebuilds only the required field operators, skips training/reduction and completed points, and retains the original 1800 s validation / 2100 s family caps for each bounded attempt. A deadline produces an explicit incomplete partial JSON result and nonzero CLI exit, instead of discarding completed comparisons. These developer files are not production cache entries.


## Final implementation checks — 2026-10-29

- Ordinary `npm run native:build`: PASS (10m39s), final shared addon; CPU/F64 analysis and existing GPU/F32 placement contract unchanged. `cargo build --release --manifest-path native/analyze/Cargo.toml --example board_volume -j1`: PASS. Build time is excluded from runtime measurements.
- `cargo test --release --manifest-path native/analyze/Cargo.toml --test board_response --test developer_checkpoint --test board_prepare --test volume_field_refinement --test volume_galerkin -j1 -- --test-threads=1`: **32 PASS**. This includes exact accepted electric defaults, physical charge/load/DC/recovery/factor lifecycle, checkpoint identity/corruption, unchanged electric controls, and bit-for-bit immutable magnetic-action reuse with wrong-space/nonfinite/eviction/failure tests.
- Existing P6 `--test coupled --test mixed`: **22 PASS** (0.61s + 1.12s test execution); physical gauges, energy, initial state, finite-front/DC admission and mixed delay/RLC regressions remain enforced.
- `npm run typecheck` and `npm run build`: PASS.
- Focused JS command with `analyze-local-cache`, `analyze-local-adapter`, `analysis-dsl`, `pcb-analysis-runtime`, `electrical-core-adapter`, `electrical-core-examples`, `electrical-resource-propagation`, `electrical-mixed-audit`: **61 PASS, 1 SKIP**, plus one spoiled concurrent-build runtime test. The latter failed because our build cleaned `dist` while it loaded a placement worker, then **PASS** when repeated after the build (`--test-name-pattern='disposeBackend stops analysis'`). The skip requires old saved mixed audit evidence; current native mixed tests and direct supplied-network DSL tests passed. No unrun audit is counted as PASS.
- `git diff --check`: PASS.

The exact-action reuse introduced after the measured 300s refusal caches only complete current-coordinate bit patterns for the same immutable native operator and `Space`. Its 16MiB payload/scratch reservation is deducted before magnetic admission; it changes no operator, mesh, quadrature, ports, basis selection, tolerance or solver. Every miss still executes the existing validated action; failed/nonfinite actions are not cached. It removes repeated identical work across initial basis/training/reduction inside one preparation and exposes measured hit/miss/bytes. It is released after condensation and is unrelated to disk factor serialization.


## Measured worker/cache delivery — packet before the journal precision fix

Packet `debugging/p6-p7-integration/acceptance-v2/`; all six run manifests verify unchanged board/scenario/compiled worker/addon SHA256 before/after. Ordinary addon SHA256 `275046a47ffd5eb394817c1b4d7e426ed670f5d77d4552ff82d4f39ef52b0fb9`; compiled DSL worker `3e220db44bb8f81cab1fc171c1e8650c8c374c3b8b3aa8d44d37fb9282c7a312`. Each cold starts an empty extraction key in a dedicated cache directory and a fresh compiled worker. All preparation/training and report execution are charged to family wall/RSS. No developer verification is hidden in these requests.

| Captured board / declared class | Family cold s | Family peak MiB | Same-worker warm s | Fresh-process restore s / MiB | Result |
|---|---:|---:|---:|---:|---|
| STM32G0_CAN_RS485 | 251.786038 | 1185.121 | 0.127043 | 2.515897 / 457.996 | complete-conditional |
| TPS63060DSCR | 195.532625 | 1421.988 | 0.134457 | 2.300541 / 511.801 | complete-conditional |
| BandAmpV5 | 9.744716 | 1242.879 | RF cache restore 0.044342 | 3.398317 / 835.754 | rejected-2GiB + supported RF section |

STM32 and TPS have cold assembly=1/training=4; memory/restore hits have assembly=0/training=0. STM32 warm native stage counts: p6Compiled=false, numericalFactorizations=0, reusedFactors=1. Plot/check-only follow-up remains the same memory key and also performs no field training. Restored `candidate.coldPrepareSeconds` is the original producer provenance, not new work: exact STM32 native cold246.146771s. STM32 immutable action reuse has23 hits/126 misses,4,824,288 retained bytes,16,834,584 conservatively reserved bytes. Observed wall times are not universal guarantees.

Model keys: STM32 `6210ea84ac5367bb4d073bd7f6f226d8e25f2646a5382e6984cc66d3a2a606a9`; TPS `c3d6114f679e40951d96b912e60cb272504a70521ba247c7bd06b55ecfaab2c3`; BandAmp supported section `ee54d1f9e5915cb3dbc853e365ce6c29c5914ee0786a106d53fe63d1b36fbcf3`. BandAmp restore uses `section-cache.js` for the same supported extraction; the separately rejected ground-via model is not retried or counted as a cache hit.

The BandAmp local ground-via request is a concrete failure: sparse numeric-factor plan **3,868,749,435 bytes > 2,084,945,980 admitted bytes**. Its ROI/contacts/diagnostic are retained in the real direct DSL report. No local current/loss map or successful cache entry is made for it. The supported RF track uses its actual captured length/width with an explicit authored section stack hypothesis;500MHz–1GHz, weak-loss quasi-TEM, minimum |S21|0.9959748265 at50Ohm. This is a section result, not whole-board or full-channel RF acceptance. The local1kHz–100kHz and P7b500MHz–1GHz examples have no certified common band, so their discontinuity cascade remains a gap; arbitrary RF extrapolation was not attempted.

Both actual STM32 and TPS models were restored by `integration-model-reuse.ts`, then excitation/load state changed with unchanged extraction keys and assembly/training=0. TPS uses its two actual VIN pads and real GND via, retained source mapping and10/20Ohm external loads. This is real condensed PDN reuse, not an authored equivalent circuit. STM32 compatible opposite-drive DC reuses the prepared model with no field solve: source power0.001053726375038612W versus cell loss0.0010537263750386105W; unit-port DC remains rejected by physical charge compatibility. These checks do not extend the production1kHz–100kHz validity band.

The same actual RLC resource was passed to the existing compiled-worker transient/finite-driver/receiver path. The saved result is **failed TEMPORAL_BAND** because its approved band excludes DC. This is a useful finite applicability refusal, not a successful time response or P8 admission. We retained the failure in `model-reuse/temporal.json`; no DC coverage or unverified temporal assumption was silently added. `model-reuse/recovery.json` and `pdn.json` retain the independent successful load/current/loss/DC checks. The first PDN helper invocation safely refused its missing exact key (Rust's echoed request includes extra null defaults); reconstructing the original authored request fixed the harness without any new field preparation.

Each real scenario contains a physically expected failing check (positive copper loss or |S21| compared with zero), a subsequent passing check and continued plots/cache reuse. Three final `integration-report.png` files under the board folders were opened and visually inspected. The STM32/TPS reports show actual finite copper/contact locations and recovered tetrahedral Joule losses, direct complex-port Z magnitudes and measured reuse timing. Large near-common capacitive impedances are shown as such; they are not subtracted to manufacture through accuracy. BandAmp shows captured RF geometry, R/G/S21 and the explicit local refusal. A clipped initial title was shortened, all three final images were regenerated and viewed again. No fabricated field map is shown.

A final-addon JS rerun (`analyze-local-cache`, `analyze-local-adapter`, `electrical-core-adapter`, `electrical-core-examples`, `electrical-resource-propagation`, `electrical-physical`) gives13PASS/1SKIP,0fail; the skip is the optional old physical saved-result audit, not the current native physical guards.

Reproduction:

```powershell
$env:PYTHONUTF8='1'
# Use a fresh packet name for a new cold run; do not overwrite admitted evidence.
python tests/fixtures/analysis-local-extraction/board-integration/integration-worker-run.py STM32G0_CAN_RS485 unique-packet cold
python tests/fixtures/analysis-local-extraction/board-integration/integration-worker-run.py STM32G0_CAN_RS485 unique-packet restart
# Repeat TPS cold/restart, BandAmp cold + restart section-cache.js.
node --import tsx tests/fixtures/analysis-local-extraction/board-integration/integration-model-reuse.ts unique-packet
python tests/fixtures/analysis-local-extraction/board-integration/integration-report.py unique-packet
```

The older measured300.151999s STM32 `acceptance-v1` attempt remains a failed cold run (peak1,280,233,472bytes) with no successful entry; it motivated the bounded exact-action reuse. Earlier191.994s/269.355s prototypes are historical and are not substituted for final immutable-hash measurements above.


### DC limit and dynamic compatibility follow-up

`board_dc_limit.rs` is a validation-only wrapper over the existing full conductive `Space.dc_factor`, gradient and Joule recovery. It rebuilds only the original current geometry, uses the actual restored P6 model and compares the component-balanced opposite drive at DC. No magnetic/electric AC assembly or training is run, no alternative solver is added, and island gauge constants are not confused with physical common potentials. Both cases PASS the predeclared5% coordinate/loss and1e-8 numerical-power gates:

| Board | Full / reduced Joule W | Max current coordinate relative error | Total loss relative error | Wall / peak RSS |
|---|---|---:|---:|---|
| STM32 | 0.0010537263750366968 / 0.0010537263750386105 | 0.0010004138902160791 | 1.8160472e-12 | 1.246013s /35,487,744bytes |
| TPS | 0.00008521411346763075 /0.0000852141134676268 | 0.00008990535073045127 | 4.6280895e-14 | 0.281401s /26,177,536bytes |

These are compatible DC endpoints, not a certification of every drive or of the unsampled DC..1kHz interval. The original production-band transient refusal remains in `temporal.json`. A separate explicitly authored **developer DC-extension hypothesis**, using the identical extracted RLC matrices and existing finite-driver/receiver engine, is saved in `temporal-dc-hypothesis.json`. Missing physical initial charge first refused (`temporal-dc-hypothesis-initial-refusal.json`); providing explicit zero potentials/currents for every actual physical model coordinate is required, not a hidden ground or discarded charge mode.

Under that declared numerical probe the compiled-worker transient and check PASS:1mV/100us input ramp,1000Ohm driver/load,1pF receiver; output at1ms **0.0004999997365685452V** versus0.0005V ±0.000025V. Normalized energy balance5.421666893e-17, relative residual4.131756722e-16, native worker high-water121,184,256bytes. `temporal-compatibility.png` was generated and visually inspected: actual extracted-model samples and quasi-static loaded target coincide at this plot resolution, with the hypothesis/band limitation printed beside them. This exercises a genuine dynamic RLC model and physical initialization through P6, while leaving production extraction validity at1kHz..100kHz. It does not by itself close physical transient applicability for P8.

Reproduce the small independent DC endpoint checks after the cold packet with `cargo build --release --manifest-path native/analyze/Cargo.toml --example board_dc_limit -j1`, `python tests/fixtures/analysis-local-extraction/board-integration/integration-dc-limit.py acceptance-v2`, then the optional `integration-model-reuse.ts acceptance-v2 --dc-validation-hypothesis`. Ordinary addon build was rechecked after adding the example (incremental13.58s, same exact addon SHA256). The main full51-drive verification binary and production addon were not changed by this validation-only example.


## Final binary, completed verification and remaining scope — 2026-10-29

The final ordinary addon is SHA256 `f706f4e43a560b8e89ccf62de23130c099010b38929808ea81fc2289939ee80c` (ordinary rebuild PASS5m06s). The compiled DSL worker remains `3e220db44bb8f81cab1fc171c1e8650c8c374c3b8b3aa8d44d37fb9282c7a312`. Numerical operators, physical request, training band, tolerances and original17 frequencies/ports were not changed by the developer journal fix below. Final raw packet is **`debugging/p6-p7-integration/acceptance-v3/`**. All production manifests compare input/core hashes before/after.

| Board/class | Cold family wall s / peak MiB | Same-worker warm wall s | Fresh-process disk restore s / family peak MiB | Status |
|---|---|---|---|---|
| STM32G0_CAN_RS485 | 147.805856 / 1187.996 | 0.140411 | 2.373755 / 461.020 | complete-conditional |
| TPS63060DSCR | 198.620415 / 1421.773 | 0.141807 | 2.225339 / 516.969 | complete-conditional |
| BandAmpV5 | 8.544210 / 1240.527 | 0.765 (separate same-section DSL warm probe) | 3.260734 / 840.168 | rejected-2GiB + RF section PASS |

STM32's first final-binary cold attempt **FAILED300.055389s /1,241,321,472bytes**. All four training solves completed (269.279805s), but reduction did not finish before the supervisor killed the owned family; no successful model cache entry was written. An unrelated workspace cargo process was observed on the shared host and subsequently ended; its exact causal contribution is not established. One explicitly recorded reproducibility retest used **identical input/core/limits**, still a true cold extraction, and completed147.805856s. Both attempts are retained; the report prints the earlier refusal. No further retest or limit increase was used. These observations prove a bounded successful run, **not guaranteed300s latency under arbitrary host contention**. Existing deadline enforcement still returns a failed/incomplete request when exceeded.

Final keys: STM32 `2637063501c5afe764ff6438c5e2ba7d1912ba97e4018bd6481b6d5a08aefd99`; TPS `afa4bc2abe3c92ba3e731ab47e9317a6591cf0f9299164e6c30c7759e50cf1e6`; RF section `cda0c432b30a07aa52339b5b1b1a3bcb808bfb02688cc5bde0baf71d806b699b`. The binary identity change caused new extraction identities; old entries were not relabelled or restored under a new key. Every final memory/disk hit retains assembly=0/training=0 and native numerical validation. STM32/TPS changed-drive/load, DC endpoints and the explicitly conditional RLC time probe were repeated on this final cache/core and PASS; the unextended production-band time request remains failed `TEMPORAL_BAND` as intended.

`integration-section-warm.ts` runs the existing captured BandAmp `section-cache.js` in a fresh compiled worker with only an elapsed-time table added. Its first call is a checked disk hit; the second complete same-worker run and existing warm plot take0.765s (cache decode/native validation alone0.029999s), with reported native-return worker high-water358,125,568bytes. This is worker high-water including prior calls, not family or incremental warm memory. Original scenario warm high-water values are retained in calculation results: STM321,160,069,120bytes, TPS1,413,730,304bytes, RF368,189,440bytes; these include prior cold work. The family RSS column above is the separate20ms supervisor measurement. No warm memory reduction is invented from OS high-water counters.

Final three board PNGs were regenerated and viewed, including the STM32 retest/refusal caption. The separate current-binary `integration-refusal.png` and `model-reuse/temporal-compatibility.png` were also opened and inspected. The refusal contains only actual original ROI/contact selections and completed stage timings, no field/loss map. The earlier initial-charge refusal is retained in the v2 packet; the final time probe supplies the required explicit physical initial state.

### Complete original full/reduced comparison

`integration-dev-v1-a/direct-fem/`: **51/51 COMPLETE/PASS**, original17 frequencies × both unit ports/opposite drive, same direct FEM and accepted electric v1. Process family943.114213s /1,041,465,344bytes; native cold preparation150.513933s and explicit developer validation792.213336s. The original1800s validation/2100s family and2GiB guards remain. No developer sweep is run by a user request.

Independent NumPy reassembly/recovery audit with predeclared5% tolerance and unchanged floors:

| Observable maximum relative error | Result |
|---|---:|
| Direct-excitation complex port V (1e-6V floor) | 1.024316663e-13 |
| Recovered current coordinates (original1e-12 weak floor) | 6.787843570e-5 |
| Total mean Joule loss (1e-18W floor) | 4.841289985e-14 |
| Recovered tetrahedron Joule loss (original1e-12 weak floor) | 3.113224840e-5 |
| Disjoint top / barrel / bottom regional loss | 2.412823018e-7 |

All51 numerical full-solver audits also pass unchanged guards: maximum KCL4.50617e-15, KVL9.80429e-10, complex-power error2.04527e-15, Joule-power error6.47902e-15. Huge common capacitive unit impedances were not subtracted to infer through accuracy. These comparisons establish reduction/recovery accuracy relative to the existing full operator on this exact ROI; the previously accepted electric spatial/analytic controls retain their stated limits and are not replaced by this comparison.

The audit of the actual final production cache (`integration-dev-v2-resume/direct-fem/integration-audit.json`) confirms **exact model, voltage basis and current basis equality** with the worker model and records final native core/hash. Production payload SHA256 `d4bc20fa6d5529e90cfaadb8a3d3dd0468baabbb273fe241ea44f552e3590df9`. This links the explicit developer comparison to the real DSL/cache result rather than a different diagnostic candidate.

### Lossless developer journal correction and actual restore

The first schema1 restore failed `developer point mapping mismatch` after21.301914s: default Rust decimal parsing changed some floats by1ulp. Strict mapping/identity guards correctly refused; no full solve was repeated. It also demonstrated why a successful serialization test of integer frequencies alone was inadequate. Schema2 stores every f64 as complete IEEE hexadecimal bits, restores through typed numeric values, rejects legacy schema and keeps checksums/actual executable identities. Production JSON parsing/cache schema1 is unchanged. New tests round-trip4096 off-grid values plus signed zero/extreme values and the original problematic frequencies, checking every bit.

One fresh schema2 **prepared** checkpoint was necessary because the schema1 restore had already reserialized its old prepared state. It took149.600843s /1,040,519,168bytes, with the same300s/2GiB preparation cap; this is explicitly separate developer work. To preserve the51 already solved comparisons, an explicit validation-only migration requires: complete independent old audit, every original point checksum, unchanged numerical sources/dependencies except the journal codec, exact equality of19 geometry/material/source/field arrays, model and both bases, and exact current arrays/audits for all four freshly computed full training solutions. It records old/new executable identities and every source/destination SHA. Any mismatch refuses migration; production cache is never migrated. The final `migration-reaudit.json` passes all these checks.

Actual schema2 restore (`integration-dev-v2-resume`) **PASS26.647880s /1,042,644,992bytes**,51before/51after, resumedPrepared=true, **zero full-drive solves**, zero training (1e-7s timer noise), developerVerification0.169186s. Prepared SHA256 stays exactly `8b11c97faaf3b632e81c61fe9aeb5ffa8a8a62a0c0e6df7dc9d968e6a83ff34c` before/after. The complete51-point independent audit passes again after restore with identical errors. Completed points are not retrained/resolved; operators are rebuilt only for this explicit developer tool, not on production cache hits. Future deadlines still emit incomplete JSON/nonzero exit and leave the prepared/finished point files; malformed/changed-identity snapshots are refused.

Final CLI SHA256 `142f41594c6aec96989a88a4ed961b98b35adbf6b72b400d1e61f0811483e143`; original completed51-point producer `5800f123c33a2fc8c49fc1ec1ff0c56299132ce9cfad27ca26730b27ad65da7a`. The migration lineage remains visible rather than pretending the old executable identity was unchanged.

| Separate developer attempt | Family wall s | Peak MiB | Result |
|---|---:|---:|---|
| integration-accepted-electric-v1 | 2004.602565 | 992.156 | failed/incomplete |
| integration-dev-v1-a | 943.114213 | 993.219 | PASS |
| integration-dev-v1-resume | 21.301914 | 993.859 | failed/incomplete |
| integration-dev-v2-prepare | 149.600843 | 992.316 | PASS |
| integration-dev-v2-resume | 26.647880 | 994.344 | PASS |

Cumulative process-family wall of these bounded developer attempts: **3145.267414s**. This includes the historical unresumable deadline, successful full verification, failed schema1 restore, replacement prepared checkpoint and successful lossless restore. It is not one user cold run or one enlarged validation allowance.

### Final obligation matrix

| Original P6/P7 obligation | Current code/test/evidence | Status / finite remaining scope |
|---|---|---|
| P6 MNA DC/AC/time, initialization, sources/loads, resources | electrical core; coupled9/mixed13 regressions, existing direct DSL | implemented+tested for declared linear/passive classes; no P6 rewrite |
| P6 S/Z/Y/Touchstone, ports/references/composition | existing frequency/resources/physical topology regressions | implemented+tested for supported authored physical topology; arbitrary infinity joins remain unsupported |
| P6 immutable model Resource and driver/receiver binding | resource-propagation/adapter tests; actual extracted RLC time probe | integrated+tested; physical source/initial-state/band permissions retained |
| P6 temporal sampled/relaxation/resonant/mixed delay classes | existing accepted P6 evidence; final coupled/mixed regressions | declared classes retained; arbitrary active/resonant fitting is future |
| P6 internal FFT/convolution/normalization/windows | existing signal.rs/signal tests and accepted ledger | internal implementation retained; general public FFT/convolution workflow remains an original integration gap, outside this local extraction supply |
| P7 section C/L/coupled modes and P7b R/G losses/delays | existing accepted P7a/P7b; actual BandAmp captured section | implemented+tested in explicit weak-loss quasi-TEM/skin-depth band; low-frequency skin transition remains gap |
| P7 local direct FEM finite foil/via/return, accepted electric defaults | board_prepare; exact original STM32 request test, worker scenarios | integrated+tested original STM32 and separate TPS finite PDN ROI,1kHz..100kHz; insulated cuts/material/fill hypotheses explicit |
| P7 reduction, physical charge/gauges, full current/region loss recovery | complete51-drive independent audit, full conductive DC endpoints | PASS in original ROI; reduction is not a certificate of all spatial directions/laboratory board accuracy |
| P7 automatic PDN multiport extraction/P6 use | two actual TPS VIN pads + actual GND via; load/source reuse and mapped RLC | finite two-port patch integrated; whole-board/arbitrary multilayer PDN scaling not admitted |
| P7 automatic full channels and line/local discontinuity composition | existing typed resources; local and RF models remain physically separate | **gap**: no certified common band for local1k..100k and RF500M..1G, BandAmp ground-via factor plan3,868,749,435>2,084,945,980bytes; unsupported does not close this obligation |
| Worker/addon/existing direct DSL, per-request frequencies/DC separation | local-request/local-executor/execute-problem/native session | implemented+tested; no diagnostic subprocess or new public DSL |
| Memory model/factor lifecycle and disk v1 | bounded LRU; checksums/shape/version/core/physics keys; atomic writes/eviction; final real restore | implemented+tested, including changed loads/drive, plots/checks, corrupt/truncated/invalid coefficients, failed preparation/write, concurrent writers and eviction |
| Real scenarios and inspected visual reports | STM32, TPS, BandAmp direct scripts; final PNGs and expected failed checks | **3/3 executable direct reports;2/3 local volume extractions plus supported RF section**. The rejected third local class and missing discontinuity cascade remain explicit |
| User runtime/resource budget | final147.806/198.620/8.544s successful families, measured RSS<2GiB | successful bounded examples; retained300.055s failure demonstrates contention sensitivity; hard refusal remains, no universal latency promise |
| Developer checkpoint/validation lifecycle | lossless schema2, strict migration lineage, actual51-point restore | implemented+tested; historical failed attempts preserved, no repeated51-solve campaign |

**P8 dependency:** real passive finite-ROI two-port RLC with SI units, physical port/reference/source mapping, conditional1kHz..100kHz AC and exact compatible DC endpoint is available. Actual P6 time/initialization/finite driver/receiver compatibility is demonstrated only under the explicit developer DC-extension hypothesis. The default extraction does not silently promise continuousDC..1kHz or arbitrary PDN transient coverage. Physical transient-band admission and larger/whole-board PDN remain gaps; P8 itself was not implemented.

**P11 dependency:** explicit captured straight sections and existing coupled-line resources retain tested losses/delays/returns and typed driver/receiver compatibility in their declared bands. Full extracted signal channels with local discontinuities/common-band return continuity are not delivered; this task does not enable arbitrary USB/DDR/2GHz RF, SI/eye/timing. P11 itself was not implemented.

This is the completed conditional local integration/cache and validation supply, with the finite physical/runtime gaps above. **Whole P6/P7 is not closed.** No solver search, new convergence campaign, active fitting, P8/P11 implementation or larger limits were introduced to hide those gaps.

Final focused release command covering board_prepare, board_response, developer_checkpoint, volume_field_refinement, volume_galerkin, coupled and mixed: **55PASS**,0fail (47.84s incremental build; test execution≈4.5s). Final ordinary native build, typecheck/build and compiled worker execution PASS. Final JS cache/adapter/P6 examples/resource regressions: **12PASS**,0skip,0fail; earlier broad runtime61PASS/1optional saved-audit SKIP and the corrected concurrent-dist-build test are recorded above. No optional absent saved audit is counted as current evidence.

Source commits: `81ce6e6` library, `74f12d1` initial checkpoint docs, `a9f23dc` integration/cache/scenarios, `e312e3c` DC/dynamic compatibility, `157a8c2` lossless developer journal, `7e87bf8` complete section warm measurement. No push. User-owned `native/analyze/debugging/` and other pre-existing inputs are preserved. Review the final matrix and raw manifests rather than interpreting historical0/3/88.8% paragraphs as current vetoes.

Final-binary user supervisor records contain four cold attempts (including the retained failed STM32 attempt), cumulative **655.025870s**, and three fresh-process restores, cumulative **7.859828s**. These are separate bounded requests, not one300s cold request or free prefilling. All field work in each cold is included.


**Actual section/local continuation:** [implementation and independent evidence](p7-segmented-channel-evidence.md) records the real no-scope numerical slab/P6 path and retained delay failure. Historical statements that no assembler exists are superseded; complete P7 and P8/P11 physical admission remain open.
