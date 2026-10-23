# PCB analysis fixtures

Captured, routed EasyEDA PCBs for analysis development. Each directory is one
selected PCB, following the per-board structure of `tests/pcb-layout`. Different
analysis scripts share the same input. Each fixture now includes `import.ts`
for structural import, `geometry.ts` for copper preparation and `mesh.ts` for
diagnostic conduction meshing through the isolated process runtime. `dc.ts` runs
production linear DC by reading the directly authored `dc.js` without transformation. Captured
boards are not independently measured physical references.

## Saved inputs

```text
tests/pcb-analyze/
  NanoScope/
    NanoScope.json          # RoutingBoardV2 input, without the capture envelope
    fixture.json            # Exact project/PCB IDs, versions, inventory and hashes
    source.epro2            # Native project archive, including supporting libraries
    source-pcb.txt          # Native source of the selected PCB
    export-evidence.json    # Native autoroute JSON, Raw PCB, DRC and diagnostics
    README.md               # Selected revision and capture notes
    import.ts               # Shared-helper structural import in a process worker
    geometry.ts             # Single copper geometry / contacts / islands
    mesh.ts                 # Diagnostic mesh; no assumed physical material profile
    dc.js                   # Authored Analysis DSL: physical inputs, sweep/check/plot
    dc.ts                   # Thin launcher for the unchanged --name dc command
    dc.expectations.json    # Test case IDs, checks and convergence; no physical models
  NanoSpec/
  ESPower/
  BandAmpV5/
  USB_PD_Trigger_5A/
  STM32G0_CAN_RS485/
  TPS63060DSCR/
  ThunderF722/
```

NanoScope and NanoSpec come from the same **NanoSpec** project. NanoScope uses the
copied `NanoScopePCB`; NanoSpec uses the original `PCB1`. The original restricted
NanoScope project is not used. ThunderF722 selects **Thunder_REVC_PCB_1**. Other
documents inside a project archive are not implicitly part of its fixture.
Exact selections and limitations are recorded in each `fixture.json` and README.

Inputs are captured without routing, refill or PCB edits. Native PCB records are
compared before and after capture; only volatile DOCHEAD client/version/updateTime
fields are excluded from that comparison. Text hashes use UTF-8 with LF line
endings, so Windows checkout conversion does not invalidate them; archive hashes
use the exact binary bytes. Diagnostics and unknown CAD fill freshness are retained.
Capture integrity is not an independently verified electrical-analysis reference.

The project archive represents the persisted project, while `source-pcb.txt` and
`export-evidence.json` describe the selected board as loaded in the editor. These
representations are not record-identical: observed differences include equivalent
rotation representations, reversed polygon paths, presentation metadata and records
present in only one representation. Each manifest retains the archive/source
comparison separately; physical equivalence is not assumed. The captured editor
data is the input used for the saved V2 board; the project archive provides native
project and library context, not a byte-identical replacement for that capture.

## Named analyses

The standalone Rust importer can already read all eight saved boards without
per-fixture scripts:

```bash
cargo test --locked --manifest-path native/analyze/Cargo.toml eight_saved_boards -- --nocapture
```

See [the import library](../../native/analyze/README.md) for diagnostic semantics
and individual JSON reading commands. This checks structural import, not physical
analysis results or independent CAD fidelity.

Run commands from the Backend repository root:

```bash
npm run test:pcb-analyze -- --list
npm run test:pcb-analyze -- NanoScope --name import
npm run test:pcb-analyze -- --all --name import
npm run test:pcb-analyze -- --all --name mesh
npm run test:pcb-analyze -- NanoScope --name dc
npm run test:pcb-analyze -- NanoScope --name emi
npm run test:pcb-analyze -- NanoScope NanoSpec --name dc
npm run test:pcb-analyze -- --all --name dc
```

`--name dc` selects **`tests/pcb-analyze/<fixture>/dc.ts`**, not an output label.
`--name dc.ts` is also accepted. A fixture can contain any number of independently
named analyses. `--list` reports only runnable `import.ts`, `geometry.ts`, `mesh.ts`
and `dc.ts`. The `.js` and expectation JSON are not additional analyses. An absent
analysis returns a nonzero exit code without creating a run directory. `emi`
remains a future example, with no placeholder solver.

Read/edit sources, loads, explicit stack/material assumptions, mesh, sweeps,
measurements and checks directly in each `dc.js`. The helper reads its exact text
into `runAnalysisDsl({board, code}, {filename: authoredPath, ...})`; there is no
scenario generator or secondary physical input contract. Ordinary JS functions
and builders remain available. Launch budgets stay explicit in the thin `dc.ts`.
`dc.expectations.json` contains only expected cases/measurement labels/checks,
quality/convergence test policy and required barrel-current coverage.

DC runs retain exact executed JS and its authored path/hash, input/profile/implementation/native hashes, JSON and self-contained per-layer HTML in the usual
named run directory. `debugging/pcb-analyze/dc/index.html` links all eight reports.
Every board attempts nominal/raised current and 1/0.5 mm² meshes; failed sweep
callbacks, missing cases/measurements, invalid geometry/plot references, nonfinite fields, failed numerical guards or >2% convergence fail the launcher after saving evidence. `audit.json` records the exact failure/pass; expected authored voltage-check failures do not abort execution. An explained native rejection
is a completed diagnostic attempt, not a solved PCB. All eight DC scenarios now
require four successful physical results. NanoScope, NanoSpec and TPS63060DSCR V2
JSONs were replayed from the unchanged captures through Copilot c7b40f2; per-fixture
reexport metadata, inventories and hashes record the 12 omitted API phantom holes.
The original native captures, source text and project archives remain unchanged. See the complete
[DC verification](../../docs/experimental/analyze/dc-analysis-verification.md).

Mesh runs save compact detailed `mesh.json` once, a small `report.json` with readiness,
counts, missing/uncertain data and measurements, and per-layer SVG overlays. Gold is
mesh, cyan is constraints and pink is pad terminal boundaries. These eight runs supply
no demonstration material profile; missing physics stays missing. Explicit complete
physical coupons and independent checks are documented in
[mesh verification](../../docs/experimental/analyze/conduction-mesh-verification.md).

Each selected analysis runs sequentially in its own Node process with `tsx`, with
the Backend root as its working directory and a 10-minute timeout, as in the PCB
layout launcher. A failed case does not stop the remaining batch; any failure
makes the overall command return a nonzero exit code.

## Runs

Every actual invocation creates a new directory, including repeated invocations
of the same analysis:

```text
debugging/pcb-analyze/NanoScope/runs/
  2026-10-21T14-30-00-000Z-dc-<unique>/
    run.json                # Name, input/script hashes, times, status and exit code
    run.log                 # stdout and stderr
    ...                     # Outputs written by dc.ts
  2026-10-21T14-31-00-000Z-emi-<unique>/
  2026-10-21T14-32-00-000Z-dc-<unique>/
```

The launcher passes three environment variables to each analysis:

| Variable | Meaning |
| --- | --- |
| `PCB_ANALYZE_FIXTURE_DIR` | Absolute directory containing the saved inputs |
| `PCB_ANALYZE_RUN_DIR` | Absolute output directory for this invocation |
| `PCB_ANALYZE_NAME` | Selected filename stem, such as `dc` |

Analysis scripts read saved fixture inputs and write their results under
`PCB_ANALYZE_RUN_DIR`. `debugging/` is ignored by Git. Inputs stay in the test bank;
new runs do not overwrite earlier runs or become expected reference results.

`import.ts` delegates to `tests/helpers/pcb-analysis-import.ts`. The helper reads
the saved `<fixture>.json`, calls [the analysis runtime](../../src/analyze/README.md),
writes the full `report.json` into the run directory and disposes the runtime in
`finally`. Invalid import or execution failure returns a nonzero exit code;
`missing`/`limitation` diagnostics with `imported: true` remain successful
structural imports. Neither result is a physical-analysis reference. Build the
shared addon (`npm run native:build`) and JS workers (`npm run build`) first.

The launcher's own infrastructure tests also cover data-only fixtures:

`geometry.ts` delegates to `tests/helpers/pcb-analysis-geometry.ts` through the
same process runtime. Run `npm run test:pcb-analyze -- --all --name geometry`.
Every run saves `report.json`, `geometry.json`, `measurements.json`, and one
`layer-<index>-<name>.svg` per copper layer. SVG uses mm/Y-down, source IDs in
attributes/tooltips, red copper for shorts and blue outlines for nets occurring
in separate physical islands. It is debug evidence, not the product PCB Preview.
`invalid` exits nonzero; `partial` is printed explicitly and retains limitations;
short findings do not discard the snapshot. Cleanup runs in `finally`.
See [geometry acceptance evidence](../../docs/experimental/analyze/copper-geometry-verification.md).

```bash
node --test scripts/test-pcb-analyze.test.mjs
```

## Captured boards

Eight PCBs from seven projects; both NanoScope and NanoSpec use the NanoSpec project.

| Fixture | Layers | Components | Pads | Tracks | Vias | Pour / Fill |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| BandAmpV5 | 2 | 9 | 29 | 21 | 133 | 2 / 0 |
| ESPower | 2 | 53 | 172 | 326 | 74 | 2 / 0 |
| NanoScope | 4 | 203 | 724 | 1135 | 249 | 8 / 2 |
| NanoSpec | 2 | 128 | 509 | 823 | 553 | 7 / 18 |
| STM32G0_CAN_RS485 | 2 | 68 | 213 | 472 | 66 | 6 / 0 |
| TPS63060DSCR | 2 | 15 | 39 | 73 | 24 | 6 / 0 |
| ThunderF722 | 6 | 116 | 340 | 494 | 136 | 31 / 0 |
| USB_PD_Trigger_5A | 2 | 17 | 65 | 144 | 80 | 2 / 0 |

The saved validator is `tests/helpers/pcb-analysis-dc-audit.ts`; regression tests:

```bash
node --import tsx --test tests/pcb-analysis-dc-audit.test.ts tests/pcb-analysis-dc-ports.test.ts
```

Every observed port is compared between both meshes at each load. Conductor loss
includes copper + finite barrels, excluding external load power. Relative limit:
2%; separate floors: 1e-9 V and 1e-9 W. Original solver residual/KCL/power guards
remain unchanged. Port declarations alone do not connect electrodes; passive
pad measurements read the existing finite-area field. Missing coverage stays
unavailable. `dc.case` is ordinary `a.log` data consumed by the helper/HTML, not a
new DSL method or hidden physical configuration.

## Explicit electrical network examples (P6a)

`rlc.js` / thin `rlc.ts` are available for STM32G0_CAN_RS485,
USB_PD_Trigger_5A and ThunderF722: `npm run test:pcb-analyze -- <fixture> --name rlc`.
They retain dc.js, use explicit RLC test hypotheses tied to verified pad/net labels,
and do not extract PCB frequency parameters. The launcher audits exact current-run
results, copies reproducible board DC evidence and saves source/inputs/hashes,
resource measurements, JSON and self-contained HTML/SVG. Numerical/reference and
negative audit assertions live in versioned tests. See the electrical-network
verification and semantics under docs/experimental/analyze.

P6b explicit frequency scenarios: `npm run test:pcb-analyze -- BandAmpV5 STM32G0_CAN_RS485 --name network`.
Edit adjacent `network.js` directly; `network.ts` only launches it. Versioned
`network.resources.json` contains the section hypothesis; standalone RF resources
are under `tests/fixtures/analysis-frequency`. Every run captures exact source
bytes/hashes, model mapping, numerical audit and HTML in its own runs directory.
