// Explicit task example: test coefficients, no PCB extraction.
const a = analysis();
const circuit = a.circuit({
  nodes: ["gnd", "src", "mid", "load"], reference: "gnd",
  ports: [{ id: "output", positive: "load", negative: "gnd" }],
  elements: [
    { id: "V1", kind: "voltage-source", nodes: ["src", "gnd"], voltageV: 3.3, model: "ideal" },
    { id: "R1", kind: "resistor", nodes: ["src", "mid"], resistanceOhm: 0.05 },
    { id: "L1", kind: "inductor", nodes: ["mid", "load"], inductanceH: 10e-9, seriesResistanceOhm: 0.01 },
    { id: "C1", kind: "capacitor", nodes: ["load", "gnd"], capacitanceF: 100e-6, esrOhm: 0.02, eslH: 1e-9 },
    { id: "I1", kind: "current-source", nodes: ["load", "gnd"], currentA: 0.1,
      waveform: { kind: "step", from: 0.1, to: 1, atS: 100e-6, riseS: 1e-6 } },
  ],
});
const out = circuit.port("output");
const dc = await a.dc().circuit(circuit).run();
a.check({ name: "DC at 100 mA", actual: dc.voltage(out), min: 3.2939, max: 3.2941, unit: "V" });
const ac = await a.pi().circuit(circuit).domain("frequency")
  .frequency({ startHz: 100, stopHz: 1e7, points: 201, scale: "log" }).run();
a.plot({ title: "Output impedance magnitude", data: ac.impedance(out).magnitude() });
a.plot({ title: "Output impedance phase", data: ac.impedance(out).phaseDeg() });
const transient = await a.pi().circuit(circuit).domain("time").initial("steady")
  .time({ stopS: 500e-6, sampleStepS: 0.5e-6 }).run();
a.plot({ title: "Load step voltage", data: transient.voltage(out) });
a.plot({ title: "Inductor current", data: transient.current("L1") });
a.check({ name: "Droop budget", actual: transient.voltage(out).min(), min: 3, unit: "V" });
const continued = await a.pi().circuit(circuit).domain("time").initial(transient.finalState)
  .time({ stopS: 1e-3, sampleStepS: 0.5e-6 }).run();
a.plot({ title: "Continuation on original physical time axis", data: continued.voltage(out) });
