// Six-layer 3V3 MCU/flash PDN, finite vias and multiple returns. All numerical inputs below are explicit TEST hypotheses.
const a = analysis({ board });

a.log({ level: "warning", message: "Authored DC test hypotheses", data: [
  "Ideal 3.3 V LDO output; MCU 180 mA, flash 20 mA, raised ×2",
  "All explicit source/load pads verified against the saved board",
  "Test stack: total 1.6 mm, each copper 35 µm, equal dielectric gaps; not a measured stackup",
  "Constant isotropic conductivity 58 MS/m at 20°C, plating 25 µm finished-hole convention; test hypotheses",
  "CAD freshness remains unknown; explicitly conditional captured snapshot"
] });

const source1 = a.port({ positive: pad("U9", "5"), negative: pad("U9", "2") });
const consumers = [
  { label: "U2.64", port: a.port({ positive: pad("U2", "64"), negative: pad("U2", "63") }), referenceV: 3.3 },
  { label: "U4.8", port: a.port({ positive: pad("U4", "8"), negative: pad("U4", "4") }), referenceV: 3.3 },
];

const stack = stackup({
  boardThicknessMm: 1.6,
  viaPlatingThicknessUm: 25,
  platingMaterial: "test-copper",
  diameterConvention: "finished_hole",
  layers: [
    { kind: "copper", name: "TOP", thicknessMm: 0.035, material: "test-copper" },
    { kind: "dielectric", name: "assumed-gap-1", thicknessMm: 0.278 },
    { kind: "copper", name: "INNER_1", thicknessMm: 0.035, material: "test-copper" },
    { kind: "dielectric", name: "assumed-gap-2", thicknessMm: 0.278 },
    { kind: "copper", name: "INNER_2", thicknessMm: 0.035, material: "test-copper" },
    { kind: "dielectric", name: "assumed-gap-3", thicknessMm: 0.278 },
    { kind: "copper", name: "INNER_3", thicknessMm: 0.035, material: "test-copper" },
    { kind: "dielectric", name: "assumed-gap-4", thicknessMm: 0.278 },
    { kind: "copper", name: "INNER_4", thicknessMm: 0.035, material: "test-copper" },
    { kind: "dielectric", name: "assumed-gap-5", thicknessMm: 0.278 },
    { kind: "copper", name: "BOTTOM", thicknessMm: 0.035, material: "test-copper" },
  ],
});

const base = a.dc()
  .components([
    ic({ designator: "U2" }),
    ic({ designator: "U4" }),
  ])
  .stackup(stack)
  .materials({ "test-copper": {
    conductivitySM: 58e6,
    state: "constant isotropic at 20 C; test assumption",
    source: "explicit test profile, not BOM",
  } })
  .temperatureC(20)
  .inputPolicy({ cadFill: "snapshot" })
  .sources([
    { port: source1, voltageV: 3.3, model: "ideal" },
  ])
  .budget({ maxSeconds: 500, maxMemoryMiB: 1024 });

// Maps refer to the completed solution instead of copying its field arrays.
function plotField(title, field) {
  const { values, ...reference } = field;
  a.plot({ title, data: reference });
}

for (const maxAreaMm2 of [1, 0.5]) {
  await a.sweep({
    parameter: { name: "load-scale", unit: "1", values: [1, 2] },
    evaluate: async (scale) => {
      const caseId = `mesh-${maxAreaMm2}/load-${scale}`;
      const dc = await base
        .name(caseId)
        .mesh({ maxAreaMm2, minAngleDeg: 0, maxRefinementSteps: 64, maxNodes: 300000, maxTriangles: 600000 })
        .loads([
          { designator: "U2", rail: net("+3.3V"), currentA: 0.18 * scale },
          { designator: "U4", rail: net("+3.3V"), currentA: 0.02 * scale },
        ])
        .run();

      const measurements = consumers.map(({ label, port, referenceV }) => {
        const voltage = dc.voltage(port);
        const drop = typeof voltage.value === "number"
          ? { ...voltage, value: Math.abs(referenceV - voltage.value) }
          : voltage;
        a.check({ name: `${caseId}/${label}/drop`, actual: drop, max: 0.002, unit: "V" });
        return { label, portId: port.id, referenceV, voltage, drop };
      });
      a.check({ name: `${caseId}/copper-loss`, actual: dc.losses, min: 0, unit: "W" });
      plotField(`${caseId}/voltage`, dc.voltageMap);
      plotField(`${caseId}/current-density`, dc.currentDensityMap);
      plotField(`${caseId}/loss`, dc.lossMap);
      a.log({ level: "info", message: "dc.case", data: {
        caseId, resultId: dc.id, maxAreaMm2, loadScale: scale, measurements,
      } });
      return dc.losses;
    },
  });
}
