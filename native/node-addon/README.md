# Shared Node addon

```text
Node -> native/node-addon/index.cjs
          -> eda-copilot-node.<platform>.node (one installed binary)
               native/node-addon (cdylib / N-API)
                 +-- pcb-board-packer (ordinary Rust library)
                 +-- pcb-analyze      (ordinary Rust library)
```

`src/lib.rs` registers the existing placement bindings and forwards their arguments
to the packer's narrow operation API. Private solver modules remain private.
Library `InvalidInput`/`Failure` errors map to the previous N-API
`InvalidArg`/`GenericFailure` statuses without rewriting messages. Coordinate
localization/restoration, F32 thread guards, CPU/GPU recovery and batch ordering
remain inside the packer calls in their original order.

`importAnalysisBoard(json: string)` forwards directly to `pcb_analyze::import_json`
and returns `{ imported, accepted, diagnostics }`. Field names, source IDs and
severities match the Rust report. Invalid JSON produces a report; an invalid JS
argument type remains a N-API argument error. The internal Board is not serialized.
This call does not enter the packer's F32 guard or GPU runtime; analyze remains
F64. Import success is not physical-analysis readiness.

## Build boundary

From the repository root:

```sh
npm run native:build
cargo test --locked --manifest-path native/pcb-board-packer/Cargo.toml
cargo test --locked --manifest-path native/analyze/Cargo.toml
node --import tsx --test tests/pcb-native-addon.test.ts
npm run typecheck
npm run build
npm run test:recovery
npm run test:package
```

`native:build` targets this host manifest with release LTO, one codegen unit and
unwinding preserved. It retains the Windows explicit MSVC target and static CRT,
then copies the host library to `eda-copilot-node.<platform>.node` beside the
loader in `native/node-addon`. `CARGO_TARGET_DIR` still overrides the build
directory. Resource resolution, native binary identity and CI/package checks use
this path. There is no loader or binary copy in either Rust library directory.

The three crates remain independent Cargo roots; no workspace/compute runtime is
introduced. The host owns the effective release profile and vendor patch for its
build. The packer retains its standalone profile/patch for its own Rust tests.
Host default `gpu` forwards to the packer, and `placement-bench` forwards the
existing opt-in probe feature. CPU-only host builds use `--no-default-features`;
production builds keep GPU support. `napi-build` runs only for the host; the
packer's build script still computes its kernel source/build key.

The host lockfile starts from the previous addon lock and adds only the analyze
path dependency and `serde_path_to_error`. Existing registry versions, including
N-API, serde and GPU crates, are preserved. Analyze's standalone lock stays
unchanged; its compatible serde dependencies resolve to the packer's existing
locked versions in the combined graph. Node/Rust parity tests compare against
the standalone Rust reader, including full diagnostic messages.

Each library owns its declarations: `native/pcb-board-packer/index.d.ts` describes
the placement functions; `native/analyze/index.d.ts` describes import and its
diagnostic report. `native/node-addon/index.d.cts` re-exports both groups for the
single CommonJS loader. The package includes all three declaration files.
These are internal bindings, not a public analysis DSL or new package entry point.
The optional benchmark-only GPU probe is not a production declaration.
Its JS number array is received as F64 and narrowed to the existing F32 probe
input at the host boundary, since N-API does not implement `FromNapiValue` for
`Vec<f32>`. The probe's Rust operation and numeric guard are unchanged.

Installed package checks load this same binary, compare all eight imported boards
and declarations, and exercise existing placement workers. CI builds each target;
foreign platform/GPU behavior cannot be inferred from a local CPU replay.

No analysis worker, queue, cancellation, cache, DSL, contacts, meshing, solver or
refill is implemented here.
