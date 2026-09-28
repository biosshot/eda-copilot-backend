# PortableScope 200MSPS 1CH layout fixture

Source: EasyEDA project `PortableScope 200MSPS 1CH` (`5081c2ba7f764745999647a017cd8813`), Board1, schematic `PortableScope` (`9a17385eacd14e25`), PCB1 (`a95458e3f1ee858f`). Exported 2026-09-28. The schematic has five pages and 246 physical components. `PortableScope.json` is the complete circuit netlist returned by EasyEDA Copilot.

`existing-placement.json` stores the existing 120 × 60.833 mm outline and eight parts already placed inside it: display U6, keys SW1–SW4, USB-C J4, RF1 and power switch SW5. Their PCB origin and rotation were read from native EasyEDA component primitives and converted to placement coordinates. The outline was read from the native board polyline. `pcb-snapshot.json` retains the full read-only PCB overview as a reference. The DSL preserves the eight poses and the outline. All other components are available for placement. The display footprint reports a 110 × 89.98 mm body, so its overhang is declared explicitly; the finished layout will still need mechanical inspection.

The DSL separates MCU/display control, ADC and 200 MHz clock, RF input and compensation ladder, protected analog front end, battery/charging/power conversion, FPGA configuration, and DDR. The known direct RF-to-ADC path, differential ADC drive and clock legs are expressed as electrical placement paths. Only selected power switch-node pairs receive explicit distance hints; other nets rely on the solver's connectivity metric. The board is two copper layers in the current EasyEDA document; routing feasibility for the BGA FPGA/DDR is outside this placement-only fixture.

Static validation, without running placement:

```powershell
cd D:\MyProject\NN\projects\Agents\easyeda-copilot\eda-copilot-backend
node --import tsx tests/pcb-layout/PortableScope/validate.mjs
```

Run placement when ready (this can take a long time):

```powershell
cd D:\MyProject\NN\projects\Agents\easyeda-copilot\eda-copilot-backend
node --import tsx tests/pcb-layout/PortableScope/PortableScope.ts
```

The runner writes `placement.svg`, reports and `board.assemble.json` under `.test-output/pcb-layout/PortableScope/`; it does not modify the EasyEDA document.
