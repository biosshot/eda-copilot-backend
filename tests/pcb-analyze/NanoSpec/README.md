# NanoSpec

This is the original **PCB1** of NanoSpec. The copied NanoScope PCB is a separate fixture.

- Fixture: `NanoSpec`.
- Project UUID: `68e8515756854180a5480226148d80b6`.
- Selected PCB: **PCB1** (`2395b4129dfc4025acb1fa48c0154d5f`). Other PCB revisions in the archive are not this fixture.
- V2 input: `NanoSpec.json`. Capture time, versions, normalized file hashes and verification: `fixture.json`.
- Native project context: `source.epro2`. Selected editor PCB source: `source-pcb.txt`. Native autoroute JSON, Raw PCB, DRC and diagnostics: `export-evidence.json`.

| V2 inventory | Count |
| --- | ---: |
| Copper layers | 2 |
| Components | 128 |
| Copper pads | 509 |
| Pad drills (slots included) | 28 |
| Independent holes | 0 |
| Tracks (arcs tessellated) | 823 |
| Vias | 553 |
| Pours / explicit fills | 7 / 18 |

No PCB edits, routing or refill were performed during capture. Selected PCB source records were compared before and after capture and remained unchanged (apart from volatile DOCHEAD fields). Inventory checks and replay of the saved capture passed.

The project archive passed ZIP CRC verification and contains the selected PCB. Its persisted records differ from the editor serialization; see the archive comparison in `fixture.json`. V2 uses the captured editor data, and equivalence to a reimported project archive has not been established.

CAD fill freshness is unknown. Diagnostics retained: `EASYEDA_V2_SEQUENTIAL_CAPTURE`, `EASYEDA_V2_STROKE_JOINS_UNVERIFIED`. Stroke expansion, where reported, still requires a physical CAD comparison. No stackup/material reference or independent numerical-analysis reference has been accepted.

No analysis scripts are included yet. See [the fixture-bank README](../README.md) for future named launches and run output directories.
