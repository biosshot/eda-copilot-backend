# Shared native addon verification — 2026-10-21

Implemented only the boundary from `next-task-native-addon.md`, with the user's
subsequent clarification: move the loader/binary into `native/node-addon`, and
aggregate library-owned declarations there. No compatibility loader is retained
in the packer directory. No commit or subsequent analysis stage is included.

## Final structure and diff scope

```text
native/pcb-board-packer/          ordinary Rust library; existing operation API
  src/api_error.rs               Node-independent InvalidInput / Failure errors
  index.d.ts                     declarations of the 42 placement operations
native/analyze/                   unchanged F64 import implementation
  index.d.ts                     importAnalysisBoard + diagnostic report types
native/node-addon/                only cdylib / N-API crate
  src/lib.rs                     forwards placement; calls analyze::import_json
  index.cjs                      loads the platform binary
  platform.cjs                   eda-copilot-node.<platform>.node
  index.d.cts                    re-exports both library declaration files
```

Packer `src/lib.rs` retains the existing operations, coordinate adapters, float
guards and execution order. Only registration attributes/N-API imports are
removed and error construction is changed to library errors. Solver modules
remain private and unmodified. The host maps errors to the previous
`InvalidArg`/`GenericFailure` statuses and preserves messages. The packer no longer
depends on napi/napi-derive/napi-build; its build script retains the kernel build
key. Host defaults forward `gpu`; `placement-bench` forwards the optional probe.
Release profiles, vendor patch, versions, Windows MSVC target and static CRT
settings are retained at the applicable Cargo root.

Runtime path changes also cover the TS resource loader, native binary identity
lookup in the existing solve cache, tests, development/replay scripts and
CI/publishing paths. The cache's behavior is unchanged. Both Rust libraries ship
only declaration files; runtime assets live in `native/node-addon`. Binary ignore
rules are at the repository root so npm's explicit `files` entries include the
binary; the host's nested ignore covers Cargo `target` only.

All old registry package versions remain in the host lock. The only added
registry package is `serde_path_to_error 0.1.20`, already used by analyze. The
standalone analyze lock is unchanged; compatible serde versions in the combined
graph follow the previous packer lock. Full Node/Rust report parity is checked
against the standalone reader, not an independent reimplementation in TS.

The opt-in GPU probe required one boundary correction: N-API does not implement
`FromNapiValue` for `Vec<f32>`. Its host wrapper accepts a JS number array as
`Vec<f64>` and narrows it to the existing Rust probe's `Vec<f32>`. The probe,
float guard and production mathematical paths are unchanged. This feature now
passes compilation; hardware execution is not claimed.

## Baseline and comparison

Baseline revision: `17f9bc3` before this task. Saved inputs, original binary and
full results are under ignored `debugging/native-addon/baseline/`.

The accepted synthetic CPU case contains three primitives, one locked pose,
rotation choices and ordinary-net relations. All three backend selectors are
`cpu`; board and post-place thread counts are 1; GPU is disabled for this replay.
An initial draft accidentally put the locked primitive inside the edge-clearance
margin. That draft's input/result is preserved separately and excluded from the
accepted comparison. Correcting the synthetic input did not modify a fixture.

| Comparison | Result |
| --- | --- |
| Production exports | Previous 42 retained, only `importAnalysisBoard` added: 43 |
| Contract/numeric functions | All 10 returned values unchanged |
| Full serialized CPU solution | Exact equality |
| Board quality | Exact equality |
| 32 other bindings with the same probe arguments | Returned values or error codes/messages equal |
| packer → analyze → packer | Full replay result unchanged |
| Hard count / severity | 0 / 0 |
| Rank score | 772.9915771484375 |
| Baseline / final replay wall time | 152.4427 / 158.9606 ms; not a performance claim |

Contract values remain: Board 7, Block 4, Passive Island 1, Placement Score 13,
PostPlace Score 1, Refiner 3, Signal Path 1, Block Search Stages 1;
`numericContract = f32-rte-ftz-v1`;
`placementGeometryEpsilon = 0.0010000000474974513`.

Raw binary SHA-256:

- Baseline: `a27eb58ec9b4695eec64187eab7b959fd3b03d54e3f4eecf9c6f6034a80dfae8`.
- Final Windows x64: `db16f4da4391a0d68e67119a0a451c8ba159844d7e1dbcd34fbc4df6e4c00096`.
- Final binary size: 25,564,672 bytes.

Full comparison: `debugging/native-addon/comparison.json`. The repeated replay
after import intentionally checks state/float-environment isolation. A replay was
also repeated after the user-approved path migration and final rebuild; those
times are service measurements, not placement benchmarks.

## Import parity

Every `accepted` field and the entire ordered diagnostics array match standalone
Rust, including severity, message, path and `source_id`. All eight have
`imported: true`; missing stackup and unknown CAD fill freshness remain explicit.
The report does not serialize the internal Board.

| Fixture | Pads | Tracks | Vias | Zones | Diagnostics | Node/Rust |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| BandAmpV5 | 29 | 21 | 133 | 2 | 9 | equal |
| ESPower | 172 | 326 | 74 | 2 | 31 | equal |
| NanoScope | 724 | 1135 | 249 | 10 | 72 | equal |
| NanoSpec | 509 | 823 | 553 | 25 | 76 | equal |
| STM32G0_CAN_RS485 | 213 | 472 | 66 | 6 | 53 | equal |
| TPS63060DSCR | 39 | 73 | 24 | 6 | 16 | equal |
| ThunderF722 | 340 | 494 | 136 | 31 | 111 | equal |
| USB_PD_Trigger_5A | 65 | 144 | 80 | 2 | 18 | equal |

Full reports: `debugging/native-addon/import-comparison.json`.
Negative cases cover malformed JSON, unknown field/layer, and invalid JS argument
type. A positive synthetic case without stackup retains an unavailable CAD fill
and its diagnostic. Mandatory binding tests fail on a missing/stale addon.

The numeric isolation test uses a nonzero F64 subnormal polygon area (~1e-320):
it remains importable before/after packer F32 FTZ calls. A leaked FTZ guard would
make that area zero. Packer FTZ and RTE sums retain their previous results.
Fresh Node processes also load/import with GPU disabled and with CUDA disabled
plus nonexistent Vulkan driver paths, with no GPU initialization diagnostics.
The import binding calls only the F64 importer, not a placement/GPU entry point.

## Commands and results

Host: Windows x64, Node v26.5.0, Rust 1.98.1. Commands run from the Backend root.
For local builds/checks, `CARGO_TARGET_DIR` was set to the absolute existing
`native/pcb-board-packer/target` directory to reuse compilation artifacts.
The normal script default is `native/node-addon/target`; the installed `.node`
is always copied into `native/node-addon` regardless of Cargo's target directory.

| Command | Result |
| --- | --- |
| `npm run native:build` | Pass; default GPU feature, MSVC target/static CRT, one final host binary |
| `cargo test --locked --manifest-path native/pcb-board-packer/Cargo.toml` with CPU backend selectors | 103 passed; 4 explicitly ignored GPU/capture tests |
| `cargo test --locked --manifest-path native/analyze/Cargo.toml` | 13 passed, including eight saved boards |
| `cargo check --offline --manifest-path native/node-addon/Cargo.toml --no-default-features` | Pass |
| `cargo check --locked --manifest-path native/node-addon/Cargo.toml --features placement-bench` | Pass after the probe's host argument correction |
| `npm run typecheck` | Pass |
| `npm run build` | Pass |
| `node --import tsx --test tests/pcb-native-addon.test.ts` | 5 passed, no skips |
| Focused TS command below | 61 passed, 7 hardware GPU cases skipped |
| `npm run test:recovery` | 11 passed, 12 hardware cases skipped |
| `npm run test:package` | Pass after final path/type migration |
| `node debugging/native-addon/compare.mjs` | Exact baseline/quality/error/mixed-order equality |
| `node debugging/native-addon/import-parity.mjs` | Eight complete Node/Rust reports equal |

Focused TS command (CPU backend selectors):

```sh
node --import tsx --test --test-concurrency=1 \
  tests/pcb-board-packer-native.test.ts tests/pcb-native-score-authority.test.ts \
  tests/pcb-post-place-refiner.test.ts tests/pcb-native-solve-cache.test.ts \
  tests/pcb-condition-semantics.test.ts tests/pcb-geometry-tolerance.test.ts \
  tests/public-api.test.ts
```

The archive was installed with scripts disabled into an isolated consumer:
`C:/Users/kiril/AppData/Local/Temp/eda-backend-package-vt29il`.
Checks load the new host path, verify the old contracts/export set, import all
eight boards, typecheck the aggregate declarations and exercise existing public
component/schematic/PCB-worker paths in normal, GPU-disabled and unavailable-driver
modes. The local archive contains one host `.node`, with module-owned declarations
and the aggregate declaration. Archive size: 9,339,453 bytes.
Package evidence: `.artifacts/package-check.json`.

## Limits and stop point

- Linux/macOS binaries and Node 20/24 were not run on this Windows/Node 26 host;
  the updated CI/publish paths require those target runners. A universal release
  check is not claimed from this single-platform archive.
- Real GPU kernels, fault injection and the benchmark probe were not executed.
  Feature compilation and no-driver/disabled recovery do not replace hardware
  parity testing. Existing Rust compiler warnings remain outside this boundary
  change.
- Old local platform binaries are preserved under ignored
  `debugging/native-addon/legacy-binaries-20261021T154956/`, not renamed into new
  foreign release binaries or included in the package.
- No transport/exporter/fixture data, placement algorithm, scoring or numerical
  policy changed. There is no new worker/queue/cancellation/cache/runtime, DSL,
  contact geometry, mesh, physical solver or refill. This does not complete the
  broader P3/P4 plan. Work stops for review without a commit.
