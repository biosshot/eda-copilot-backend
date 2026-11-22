// USB connector → F1 → output connector, including GND return. All numerical inputs below are explicit TEST hypotheses.
const a = analysis({ board });

a.log({ level: "warning", message: "Authored DC test hypotheses", data: [
  "20 V, nominal 2 A / raised 4 A; test operating points only",
  "Fixed path drop threshold 0.1 V = 0.5% of the authored 20 V source",
  "F1 is explicitly represented by 20 mOhm; assumed cold fuse resistance, not extracted from BOM",
  "Test stack: total 1.6 mm, each copper 35 µm, equal dielectric gaps; not a measured stackup",
  "Constant isotropic conductivity 58 MS/m at 20°C, plating 25 µm finished-hole convention; test hypotheses",
  "CAD freshness remains unknown; explicitly conditional captured snapshot"
] });

const source1 = a.port({ positive: pad("J1", "A9"), negative: pad("J1", "A12") });
const load1 = a.port({ positive: pad("J2", "2"), negative: pad("J2", "1") });
const consumers = [
  { label: "J2.2", port: a.port({ positive: pad("J2", "2"), negative: pad("J2", "1") }), referenceV: 20 },
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
    resistor({"designator": "F1", "resistanceOhm": 0.02}),
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
    { port: source1, voltageV: 20, model: "ideal" },
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
          { port: load1, currentA: 2 * scale },
        ])
        .run();

      const measurements = consumers.map(({ label, port, referenceV }) => {
        const voltage = dc.voltage(port);
        const drop = typeof voltage.value === "number"
          ? { ...voltage, value: Math.abs(referenceV - voltage.value) }
          : voltage;
        a.check({ name: `${caseId}/${label}/drop`, actual: drop, max: 0.1, unit: "V" });
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
