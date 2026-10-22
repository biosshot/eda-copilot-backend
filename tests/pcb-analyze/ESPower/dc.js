// Explicit logic and VDC power loads. All numerical inputs below are explicit TEST hypotheses.
const a = analysis({ board });

a.log({ level: "warning", message: "Authored DC test hypotheses", data: [
  "Separate ideal 3.3 V and 5 V sources at U6 output/input; no conversion model",
  "U1 150 mA, U13 30 mA, U7 input 20 mA; raised ×2",
  "Test stack: total 1.6 mm, each copper 35 µm, equal dielectric gaps; not a measured stackup",
  "Constant isotropic conductivity 58 MS/m at 20°C, plating 25 µm finished-hole convention; test hypotheses",
  "CAD freshness remains unknown; explicitly conditional captured snapshot"
] });

const source1 = a.port({ positive: pad("U6", "5"), negative: pad("U6", "2") });
const source2 = a.port({ positive: pad("U6", "1"), negative: pad("U6", "2") });
const consumers = [
  { label: "U1.2", port: a.port({ positive: pad("U1", "2"), negative: pad("U1", "33") }), referenceV: 3.3 },
  { label: "U13.5", port: a.port({ positive: pad("U13", "5"), negative: pad("U13", "6") }), referenceV: 3.3 },
];

const stack = stackup({
  boardThicknessMm: 1.6,
  viaPlatingThicknessUm: 25,
  platingMaterial: "test-copper",
  diameterConvention: "finished_hole",
  layers: [
    { kind: "copper", name: "TOP", thicknessMm: 0.035, material: "test-copper" },
    { kind: "dielectric", name: "assumed-gap-1", thicknessMm: 1.53 },
    { kind: "copper", name: "BOTTOM", thicknessMm: 0.035, material: "test-copper" },
  ],
});

const base = a.dc()
  .components([
    ic({ designator: "U1" }),
    ic({ designator: "U13" }),
    ic({ designator: "U7" }),
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
    { port: source2, voltageV: 5, model: "ideal" },
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
        .mesh({ maxAreaMm2, minAngleDeg: 0, maxRefinementSteps: 64 })
        .loads([
          { designator: "U1", rail: net("3V3"), currentA: 0.15 * scale },
          { designator: "U13", rail: net("3V3"), currentA: 0.03 * scale },
          { designator: "U7", rail: net("VDC"), currentA: 0.02 * scale },
        ])
        .run();

      const measurements = consumers.map(({ label, port, referenceV }) => {
        const voltage = dc.voltage(port);
        const drop = typeof voltage.value === "number"
          ? { ...voltage, value: Math.abs(referenceV - voltage.value) }
          : voltage;
        a.check({ name: `${caseId}/${label}/drop`, actual: drop, max: 0.005, unit: "V" });
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
