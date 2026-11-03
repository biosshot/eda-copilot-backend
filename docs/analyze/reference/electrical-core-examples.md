# Explicit circuit AC and supplied sampled models

These examples run without a board or a complete BOM. They exercise authored
linear models. A board context can supply exact component nominals, but neither
example extracts PCB copper or creates an SI channel from geometry. See the
[supported classes and limits](electrical-core-completion-semantics.md).

## Loaded AC response

A Spectrum uses peak complex phasors with `exp(+jωt)`. Its frequency axis must
match the builder's axis exactly; for a linear grid generate it with
`start + (stop - start) * (i / (points - 1))`. The finite source resistance and
receiver R/C affect the response. Transfer is the observed voltage ratio under
this single source, independently of the unit-current impedance calculation.

```js
const a = analysis();
const c = a.circuit({
  nodes: ['in', 'out', 'gnd'], reference: 'gnd',
  ports: [
    {id: 'input', positive: 'in', negative: 'gnd'},
    {id: 'output', positive: 'out', negative: 'gnd'},
  ],
  elements: [
    {id: 'R', kind: 'model',
      model: resistor({designator: 'R-authored', resistanceOhm: 1000}),
      pins: {p: 'in', n: 'out'}},
    {id: 'C', kind: 'capacitor', nodes: ['out', 'gnd'],
      capacitanceF: 1e-9, ideal: true},
  ],
});
const frequencyHz = Array.from({length: 41},
  (_, i) => 1000 + (100000 - 1000) * (i / 40));
const result = await a.pi().circuit(c).domain('frequency')
  .frequency({startHz: 1000, stopHz: 100000, points: 41, scale: 'linear'})
  .sources([{
    port: c.port('input'),
    model: {kind: 'linear-driver', lowV: 0, highV: 1, outputResistanceOhm: 50},
    voltageSpectrum: {frequencyHz, unit: 'V',
      values: frequencyHz.map(() => ({real: 1, imaginary: 0.5}))},
  }])
  .loads([{port: c.port('output'),
    model: {kind: 'rc-input', capacitanceF: 1e-9, resistanceOhm: 10000}}])
  .run();
a.plot({title: 'Loaded output', data: result.voltage(c.port('output')).magnitude()});
a.plot({title: 'Own circuit transfer phase',
  data: result.transfer({from: c.port('input'), to: c.port('output')}).phaseDeg()});
a.check({name: 'Finite observed output',
  actual: result.voltage(c.port('output')).magnitude().max(), min: 0.5, unit: 'V'});
```

An explicit authored parameter takes precedence over the selected component's
nominal. Missing ESR/ESL/DCR must be supplied or explicitly declared ideal; a
nominal does not supply parasitics. A model Resource must authorize the actual
operation (`dc`, `pi` or `si`), when its `analyses` field is present.

## A nonconstant imported/supplied model to time

This example samples an independent one-pole admittance formula before fitting.
A real imported S/Z/Y or Touchstone Resource uses the same temporal metadata,
provided it meets the declared class and checks. The DC row and high-frequency
conductance/capacitance closure are authored physical assumptions. Here `gInf`
is the conducting part after subtracting `s*CInf`; it is not the entire complex
high-frequency admittance. SI-only authorization is sufficient for this operation.

```js
const a = analysis();
const frequenciesHz = [0, ...Array.from({length: 161},
  (_, i) => 10 ** (-1 + i / 20))];
const model = a.model({
  id: 'supplied-rc', kind: 'network', ports: ['p'], analyses: ['si'],
  source: 'Independent one-pole Y samples with explicitly authored closure',
  data: {
    format: 'network-parameters', representation: 'Y',
    referenceTopology: 'common', nature: 'passive', frequenciesHz,
    parameters: frequenciesHz.map(f => {
      const q = f / 73;
      return [[{real: 0.002 + 0.01 * q * q / (1 + q * q),
        imaginary: 0.01 * q / (1 + q * q)}]];
    }),
    temporal: {
      method: 'passive-relaxation', normalizationOhm: 50,
      highFrequency: {conductanceS: [0.012], capacitanceF: [0]},
    },
  },
});
const n = a.network({model, ports: {output: 'p'}});
const result = await a.si().network(n)
  .drivers([{
    port: n.port('output'),
    model: {kind: 'linear-driver', lowV: 0, highV: 1, outputResistanceOhm: 50},
    stimulus: {kind: 'waveform', voltageV:
      {kind: 'step', from: 0, to: 1, atS: 0, riseS: 1e-4}},
  }])
  .receivers([{port: n.port('output'),
    model: {kind: 'rc-input', capacitanceF: 1e-12}}])
  .time({stopS: 0.02, sampleStepS: 1e-5})
  .numerics({relativeTolerance: 1e-5, absoluteTolerance: 1e-9})
  .run();
a.plot({title: 'Conditional supplied-model response',
  data: result.voltage(n.port('output'))});
a.check({name: 'Late response under these assumptions',
  actual: result.voltage(n.port('output')).at({timeS: 0.02}), min: 0.9, unit: 'V'});
```

The result remains conditional even when fit, holdout and time checks pass.
Finite samples cannot rule out unseen narrow resonances. The front screen does
not prove the source has no spectrum outside the band. General resonant/active
fitting and finite-band lossy P7b tables to time are unsupported. This example
makes no BER, eye-mask, timing or TDR claim.

The [complete verified scenario](../../../tests/fixtures/analysis-electrical-core/scenario.js)
also includes exact real-K delay models, matched/reflected port traces, a passive
pad/line frequency cascade and an expected failed check followed by a successful
check. Its standalone and two board-context artifacts are described in
[verification](../evidence/electrical-core-completion-verification.md).


## Mixed distributed delay and discrete filters

`tests/fixtures/analysis-electrical-mixed/scenario.js` is a direct standalone or
board-context DSL: matched delay to RC, delay to RLC to delay, and finite-driver/RC-receiver
reflections, each with independent time/history refinement and full 2-port S
frequency calculations. Run the existing evidence runner with `--mixed`:

```powershell
node --import tsx tests/helpers/electrical-core-verification.ts --mixed debugging/p6-mixed-verification/direct-dsl
```

The default contexts are standalone, STM32G0_CAN_RS485 and ThunderF722. Actual
R1/U2 pad identities/nets are recorded as provenance; all electrical models are
authored hypotheses, not extracted traces. Saved JSON contains V/I, separate
storage/loss/work traces, original source Resources and port mapping. The shared
static renderer recognizes these saved mixed cases. See
[evidence and remaining limits](../evidence/electrical-mixed-verification.md).
