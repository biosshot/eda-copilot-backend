const a = analysis();
const referenceOhm = 50;
const frequenciesHz = [1e9, 2e9];
const reflection = { real: 0, imaginary: 0 };
const transmission = { real: 0.5, imaginary: 0 };
const attenuatorModel = a.model({
  id: "my-attenuator",
  kind: "network",
  ports: ["p1", "p2"],
  data: {
    format: "network-parameters",
    representation: "S",
    referenceOhm,
    referenceTopology: "common",
    nature: "passive",
    frequenciesHz,
    parameters: frequenciesHz.map(() => [
      [reflection, transmission],
      [transmission, reflection],
    ]),
  },
});
const first = a.network({
  model: attenuatorModel, ports: { input: "p1", output: "p2" },
});
const second = a.network({
  model: attenuatorModel, ports: { input: "p1", output: "p2" },
});
const cascade = a.connect({
  networks: [first, second],
  connections: [{ from: first.port("output"), to: second.port("input") }],
  ports: { input: first.port("input"), output: second.port("output") },
});
const r = await a.networkParameters().network(cascade)
  .frequency({ startHz: 1e9, stopHz: 2e9, points: 101, scale: "linear" })
  .representation("S").referenceOhm(50).run();
a.plot({ title: "Передача каскада", data: r.parameter({ out: "output", in: "input" }).magnitude() });
a.plot({ title: "Отражение входа", data: r.parameter({ out: "input", in: "input" }).magnitude() });
a.check({ name: "S21 в исходной точке", unit: "1",
  actual: r.parameter({ out: "output", in: "input" }).at({ frequencyHz: 1e9 }).magnitude(),
  target: 0.25, tolerance: 1e-10 });
