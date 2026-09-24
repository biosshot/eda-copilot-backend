# PortableScope schematic layout fixtures

Snapshots of the open EasyEDA project **PortableScope 200MSPS 1CH**, exported on
2026-09-24 through the EasyEDA Copilot Skill CLI. The five `*-full.json` cases
cover all 246 physical components on the five schematic pages. The other 19
cases isolate functional blocks for layout review.

The FPGA `U3` is a multipart library symbol. The nine sections present in the
EasyEDA export are represented as `U3.1` through `U3.10`, omitting unused
section `U3.7`; all 348 exported pins are preserved. The public-library display
`U6` uses its exact symbol UUID as `part_uuid`. No electrical pin assignments
were changed.

Run the visual regression bank from `eda-copilot-backend`:

```sh
npm run test:schematics -- --workers 4 --timeout 300
```

The runner writes `before.png`, `after.png`, SVGs, serialized ASM, metrics,
and an `index.html` gallery to `.test-output/new-circuit-layout/`. A `failed` layout
case can still have valid connectivity; inspect `report.json` for the exact
visual regression.
