const a = analysis();
const line = a.line({
  kind: "microstrip",
  widthMm: 0.30,
  copperThicknessMm: 0.035,
  dielectricHeightMm: 0.18,
  lengthMm: 20,
  material: {
    relativePermittivity: 4.2,
    lossTangent: 0,
    state: "Explicit ideal dielectric hypothesis; PEC conductors, no mask",
  },
});
const z = await a.impedance()
  .target(line)
  .method("cross-section")
  .frequency({ startHz: 1e6, stopHz: 1e9, points: 101, scale: "log" })
  .run();
a.plot({ title: "Характеристический импеданс идеальной линии", data: z.characteristicImpedance });
a.plot({ title: "Параметры сечения", data: z.rlgc });
const r = await a.networkParameters()
  .network(z.network)
  .frequency({ startHz: 1e6, stopHz: 1e9, points: 101, scale: "log" })
  .representation("S").referenceOhm(50).run();
a.plot({ title: "Отражение линии при нормировке 50 Ом", data: r.parameter({ out: "input", in: "input" }).magnitude() });
a.plot({ title: "Передача линии", data: r.parameter({ out: "output", in: "input" }).phaseDeg() });
