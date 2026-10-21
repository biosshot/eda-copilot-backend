# PCB analysis fixtures

Captured, routed EasyEDA PCBs for analysis development. Each directory is one
selected PCB, following the per-board structure of `tests/pcb-layout`. Different
analysis scripts may later share the same input. No analysis `.ts` files or
numerical reference results are included yet.

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

Run commands from the Backend repository root:

```bash
npm run test:pcb-analyze -- --list
npm run test:pcb-analyze -- NanoScope --name dc
npm run test:pcb-analyze -- NanoScope --name emi
npm run test:pcb-analyze -- NanoScope NanoSpec --name dc
npm run test:pcb-analyze -- --all --name dc
```

`--name dc` selects **`tests/pcb-analyze/<fixture>/dc.ts`**, not an output label.
`--name dc.ts` is also accepted. A fixture can contain any number of independently
named analyses. Currently `--list` reports data-only fixtures; requesting an absent
analysis returns a nonzero exit code without creating a run directory.

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

The launcher's own infrastructure tests run without committed analysis scripts:

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
