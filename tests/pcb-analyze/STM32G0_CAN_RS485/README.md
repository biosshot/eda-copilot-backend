# STM32G0 CAN RS485 Controller 60x40

- Fixture: `STM32G0_CAN_RS485`.
- Project UUID: `96d4453b7c2146a18b64b7d70910a28c`.
- Selected PCB: **PCB1** (`b988759eb94f13da`). Other PCB revisions in the archive are not this fixture.
- V2 input: `STM32G0_CAN_RS485.json`. Capture time, versions, normalized file hashes and verification: `fixture.json`.
- Native project context: `source.epro2`. Selected editor PCB source: `source-pcb.txt`. Native autoroute JSON, Raw PCB, DRC and diagnostics: `export-evidence.json`.

| V2 inventory | Count |
| --- | ---: |
| Copper layers | 2 |
| Components | 68 |
| Copper pads | 213 |
| Pad drills (slots included) | 27 |
| Independent holes | 0 |
| Tracks (arcs tessellated) | 472 |
| Vias | 66 |
| Pours / explicit fills | 6 / 0 |

No PCB edits, routing or refill were performed during capture. Selected PCB source records were compared before and after capture and remained unchanged (apart from volatile DOCHEAD fields). Inventory checks and replay of the saved capture passed.

The project archive passed ZIP CRC verification and contains the selected PCB. Its persisted records differ from the editor serialization; see the archive comparison in `fixture.json`. V2 uses the captured editor data, and equivalence to a reimported project archive has not been established.

CAD fill freshness is unknown. Diagnostics retained: `EASYEDA_V2_SEQUENTIAL_CAPTURE`. Stroke expansion, where reported, still requires a physical CAD comparison. No stackup/material reference or independent numerical-analysis reference has been accepted.

Named import, geometry, mesh and DC analyses share this fixture. See [the fixture-bank README](../README.md) for commands and run output directories.

DC example: edit [`dc.js`](dc.js) directly; [`dc.ts`](dc.ts) only launches it unchanged. [`dc.expectations.json`](dc.expectations.json) holds test expectations, not physical inputs. Run `npm run test:pcb-analyze -- STM32G0_CAN_RS485 --name dc` from the backend root. Exact code/provenance, audit and HTML are saved per run.

### Explicit RLC section model

Run `npm run test:pcb-analyze -- STM32G0_CAN_RS485 --name rlc` after current `dc` evidence.
Direct `rlc.js` binds verified fixture pads/nets to an explicitly given RLC test
model; all coefficients are hypotheses, no PCB frequency extraction. The launcher
saves source/inputs/hashes/quality/audit/HTML, copies current matching board DC
and compares load/ESR, integrator refinement and continuation.
