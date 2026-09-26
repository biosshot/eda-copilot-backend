# EDA Copilot Backend

Independent Node.js library for component search, schematic assembly and PCB placement. The EasyEDA extension and MCP live in a different repository; copper routing belongs to `eda-copilot-router`.

```ts
import { componentSearch, extractCircuit, makePcbLayout, disposeBackend } from 'eda-copilot-backend';

const components = await componentSearch({ MPN: 'STM32F103C8T6' });
try {
  const layout = await makePcbLayout({ code: placementDsl, circuit, footprints }, {
    onProgress: progress => console.error(progress.content),
  });
} finally {
  await disposeBackend();
}
```

Entry points: `.`, `/components`, `/schematic`, `/pcb`, `/types`. All runtime dependencies belong to this package. Component/UUID resolution uses public EasyEDA APIs; supplied footprints allow offline placement. No private Copilot server, LLM credentials or database is required.

## Development

```sh
npm ci
npm run native:build
npm run check
npm run test:package
```

Node >=20.19 is required. Rust/Cargo and the platform C/C++ toolchain are needed only to build the native solver from source. `npm run dev` watches TypeScript and copies its assets; rebuild Rust separately when changing native code.

The repository contains its own `src/types`. `component.ts`, `circuit.ts`, `lcsc.ts` and `reused.ts` were copied from the original `copilot-server/src/types` during extraction. They are normal source files maintained here, not aliases or generated links. The existing lightweight Zod preprocess helpers are local. There is no `@copilot/shared` dependency and no EasyEDA repository checkout is required to build or test this package.

`npm run test:package` builds a local npm archive from existing build outputs, installs only that archive into an isolated consumer with install scripts disabled, and exercises public APIs, declarations, workers, assets and the native solver. API calls use local fixtures. It does not publish anything.

Real board examples are in `tests/pcb-layout`; run `npm run test:pcb-layout -- --list`, selected fixture names, or `--all`. These optional examples query the public footprint catalogue and require network access.

## Native releases

`.github/workflows/ci.yml` builds and tests Windows x64, Linux x64 on Ubuntu 22.04 (glibc 2.35 baseline), macOS Intel and macOS arm64 on Node 20/24. `.github/workflows/publish.yml` runs only on a `v*` tag. It waits for CI, collects all four `.node` binaries into one package, runs `npm run check:release` and tests the archive before publishing. There is no install-time Rust compilation.

`npm run check:release` deliberately fails in a checkout containing only its host binary. It verifies all four binaries, dependency independence, declarations and the release tag/version. `npm pack` is available for local single-platform development archives, but those are not universal releases.

Releases are published on [npm](https://www.npmjs.com/package/eda-copilot-backend). npm Trusted Publishing (OIDC) is configured for `biosshot/eda-copilot-backend`, workflow `publish.yml`. Version `0.1.0` was published manually; subsequent versions are published from matching `v*` tags after the portability matrix, archive checks and release checks pass. npm versions are immutable, so bump the package and lockfile versions before creating a new tag. No tag or publish command is needed for local development. Linux/macOS support is conditional on successful CI, not inferred from a Windows test. Windows ARM64, Linux ARM64 and Alpine/musl are not currently part of the supported complete Copilot stack.

## EasyEDA Copilot integration

Place this repository beside `easyeda-copilot` and `copilot-router`. In EasyEDA Copilot, `npm run deps:local` selects sibling file dependencies; `npm run deps:release` selects pinned published versions. MCP does not bundle this package or duplicate its runtime dependencies. Read EasyEDA Copilot's `docs/local-development.md` for the full workflow.

`PCB_BOARD_PACKER_NATIVE_PATH` is an optional developer override. Normal assets resolve relative to this package, independently of the current directory. Always call `disposeBackend()` when the host shuts down to close worker pools. `PCB_LAYOUT_WORKERS`, `PCB_LAYOUT_WORKER_QUEUE_SIZE`, `PCB_LAYOUT_WORKER_TIMEOUT_MS` and `EDA_BACKEND_LOG_LEVEL` configure runtime behavior.

`PCB_BOARD_PACKER_THREADS` controls native board-level beam-search parallelism
(default: all available logical CPUs; `1` selects serial execution). Independent
beam states use native Rust threads with private geometry and micro-router caches.
Results are merged in input order to preserve deterministic tie-breaking. Local
orientation scoring and repair-variant scoring are also parallel; movement commits
and lazy route refinement remain sequential. Set `PCB_BOARD_PACKER_PROFILE=1` to
log separate beam, local-improvement and repair elapsed times. More threads require additional cache
memory; this setting is separate from `PCB_LAYOUT_SUBTREE_WORKERS`, which uses
isolated Node processes for block/module placement.

Post-placement refinement makes one TypeScript-to-Rust call for the entire
search. TypeScript compiles invariant geometry, constraints and JavaScript
collation ranks once; Rust generates candidates, checks hard constraints,
scores/routes them, caches route baselines, selects moves and emits diagnostics.
Each native worker reuses a private world and updates/rolls back only changed
components. Candidate groups sharing a baseline stay on one worker. Results are
merged in original order; accepted moves and iterations remain sequential.
`PCB_POST_PLACE_THREADS` selects native refinement threads (fallback:
`PCB_BOARD_PACKER_THREADS`, then `PCB_LAYOUT_SUBTREE_WORKERS`, then available
logical CPUs). `0`/`1` selects one thread. The synchronous API uses one thread.
Routing jobs within each candidate remain sequential. No Node process pool,
per-candidate addon conversion or TypeScript callback is used in this loop.
The subtree pool remains process-based because loading the addon in multiple
Node worker threads caused heap corruption on Windows.

With `PCB_BOARD_PACKER_PROFILE=1`, post-place logs include candidate counts,
hard/bound/feasibility/improvement rejections, baseline cache hits, and timings
for geometry, scoring, routing and iteration wall time. Worker timings are
summed work time, not elapsed wall time. Top-level `encodingMs` measures the
one-time TypeScript compilation; native `totalMs` excludes addon conversion.
Per-iteration encoding fields are zero because there is no TypeScript/native
boundary there. These metrics are also saved in post-place stage data.
Parallel batches only use the fixed
minimum-improvement bound, so they can route more candidates than an incumbent-
pruned serial search; speedup must be measured. The default 16 iteration limit
and early stop remain; there is no separate candidate-count budget. The enclosing
placement worker timeout still applies; subtree task timeouts do not limit refine.

Run `node --import tsx scripts/benchmark-post-place.ts [output.json]` from this
repository to compare the frozen TypeScript reference and one/two native threads on a synthetic 256-part
route-obstacle fixture. It verifies identical placements, diagnostics and moves;
its timings do not predict a particular production board.

`scripts/benchmark-native-post-place.ts snapshot.json [output.json]` uses a
saved `{ input, placements }` board snapshot, bounds candidate generation to six
named components and two passes, and verifies identical output against the
TypeScript reference. It does not apply placement or run the global solver.
The reference implementation exists for parity tests/benchmarks only and is
not imported at runtime by the production refiner.
