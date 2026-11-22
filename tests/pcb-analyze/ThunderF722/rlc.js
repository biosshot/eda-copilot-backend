// Заданная RLC-модель участка; частотное извлечение PCB не выполнялось.
// U9.5/U9.2 to U2.64/U2.63 MCU and U4.8/U4.4 flash on +3.3V. Every coefficient is a TEST hypothesis.
const a = analysis();
const binding = {
  "rail": "+3.3V",
  "source": [
    "U9",
    "5",
    "+3.3V"
  ],
  "sourceReturn": [
    "U9",
    "2",
    "GND"
  ],
  "consumers": [
    [
      "U2",
      "64",
      "+3.3V"
    ],
    [
      "U4",
      "8",
      "+3.3V"
    ]
  ],
  "returns": [
    [
      "U2",
      "63",
      "GND"
    ],
    [
      "U4",
      "4",
      "GND"
    ]
  ]
};
for (const [designator, number, net] of [binding.source, binding.sourceReturn, ...binding.consumers, ...binding.returns]) {
  const pads = board.pads.filter(p => p.component === designator && p.number === number);
  if (pads.length !== 1 || pads[0].net !== net) throw Error(`Unverified board binding ${designator}.${number}/${net}`);
}
a.log({ level: "warning", message: "rlc.binding", data: {
  ...binding,
  assumptions: "Ideal 3.3 V source; R=0.05 Ohm, L=10 nH, DCR=0.01 Ohm, C=100 uF, ESL=1 nH, ESR=0.02/0.08 Ohm; explicit test hypotheses, not BOM/extraction. Aggregate 0.1 A initial → 0.2/0.4 A at 100 us, 1 us front. Board DC evidence is saved alongside, with its own conditional CAD/material assumptions.",
} });
let refined;
for (const [label, peakA, esrOhm] of [["nominal", 0.2, 0.02], ["raised", 0.4, 0.08]]) {
  const circuit = a.circuit({
    nodes: ["gnd", "src", "mid", "load"], reference: "gnd",
    ports: [{ id: "output", positive: "load", negative: "gnd" }],
    elements: [
      { id: "V1", kind: "voltage-source", nodes: ["src", "gnd"], voltageV: 3.3, model: "ideal" },
      { id: "R1", kind: "resistor", nodes: ["src", "mid"], resistanceOhm: 0.05 },
      { id: "L1", kind: "inductor", nodes: ["mid", "load"], inductanceH: 10e-9, seriesResistanceOhm: 0.01 },
      { id: "C1", kind: "capacitor", nodes: ["load", "gnd"], capacitanceF: 100e-6, esrOhm, eslH: 1e-9 },
      { id: "I1", kind: "current-source", nodes: ["load", "gnd"], currentA: 0.1,
        waveform: { kind: "step", from: 0.1, to: peakA, atS: 100e-6, riseS: 1e-6 } },
    ],
  });
  const out = circuit.port("output");
  const dc = await a.dc().circuit(circuit).run();
  a.check({ name: `${label}/DC`, actual: dc.voltage(out), target: 3.294, tolerance: 1e-8, unit: "V" });
  a.log({ level: "info", message: "rlc.case", data: { caseId: `${label}/dc`, resultId: dc.id } });
  const ac = await a.pi().circuit(circuit).domain("frequency")
    .frequency({ startHz: 100, stopHz: 1e7, points: 201, scale: "log" }).run();
  a.plot({ title: `${label}/ac:magnitude`, data: ac.impedance(out).magnitude() });
  a.plot({ title: `${label}/ac:phase`, data: ac.impedance(out).phaseDeg() });
  a.log({ level: "info", message: "rlc.case", data: { caseId: `${label}/ac`, resultId: ac.id } });
  const transient = await a.pi().circuit(circuit).domain("time").initial("steady")
    .numerics({ relativeTolerance: 1e-6, absoluteTolerance: 1e-9 })
    .time({ stopS: 500e-6, sampleStepS: 0.5e-6 }).run();
  a.check({ name: `${label}/strict droop`, actual: transient.voltage(out).min(), min: 3.3, unit: "V" });
  a.check({ name: `${label}/after fail`, actual: transient.losses.max(), min: 0, unit: "W" });
  a.plot({ title: `${label}/time:voltage`, data: transient.voltage(out) });
  a.plot({ title: `${label}/time:current`, data: transient.current("L1") });
  a.log({ level: "info", message: "rlc.case", data: { caseId: `${label}/time`, resultId: transient.id } });
  if (label === "nominal") refined = { circuit, out };
}
const fine = await a.pi().circuit(refined.circuit).domain("time").initial("steady")
  .numerics({ relativeTolerance: 1e-7, absoluteTolerance: 1e-10 })
  .time({ stopS: 500e-6, sampleStepS: 0.5e-6 }).run();
a.plot({ title: "refined/time:voltage", data: fine.voltage(refined.out) });
a.plot({ title: "refined/time:current", data: fine.current("L1") });
a.log({ level: "info", message: "rlc.case", data: { caseId: "refined/time", resultId: fine.id } });
const continued = await a.pi().circuit(refined.circuit).domain("time").initial(fine.finalState)
  .numerics({ relativeTolerance: 1e-7, absoluteTolerance: 1e-10 })
  .time({ stopS: 1e-3, sampleStepS: 0.5e-6 }).run();
a.plot({ title: "continued/time:voltage", data: continued.voltage(refined.out) });
a.plot({ title: "continued/time:current", data: continued.current("L1") });
a.log({ level: "info", message: "rlc.case", data: { caseId: "continued/time", resultId: continued.id } });
