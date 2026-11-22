// dcc17162579803b7, TOP RF_IN_PORT, (-15.8299,0) -> (-5.2,0).
// Only this uniform 10.6299 mm straight section; reference/material are hypotheses.
const a = analysis();
const line = a.line({kind:'microstrip',widthMm:0.6736,copperThicknessMm:0.035,
  dielectricHeightMm:0.4,lengthMm:10.6299,
  material:{relativePermittivity:4.2,lossTangent:0,state:'Board-derived geometry; explicit ideal stackup hypothesis, PEC, no mask; reference plane not verified'}});
const z = await a.impedance().target(line).method('cross-section')
  .frequency({startHz:1e6,stopHz:1e9,points:41,scale:'linear'}).run();
a.plot({title:'Z0',data:z.characteristicImpedance});
a.plot({title:'RLGC',data:z.rlgc});
a.plot({title:'Delay',data:z.delay});
a.check({name:'expected fail: not zero impedance',actual:z.characteristicImpedance,target:0,tolerance:1,unit:'Ohm'});
a.check({name:'continued: finite positive Z0',actual:z.characteristicImpedance,min:1,max:500,unit:'Ohm'});
const r = await a.networkParameters().network(z.network)
  .frequency({startHz:1e6,stopHz:1e9,points:41,scale:'linear'}).representation('S').referenceOhm(50).run();
for (const [out, input, label] of [['input','input','S11'],['output','input','S21']]) {
  const p = r.parameter({out,in:input});
  a.plot({title:label+' magnitude',data:p.magnitude()});
  a.plot({title:label+' phase',data:p.phaseDeg()});
}
// A supplied Attenuator Pad is an explicit circuit model, not a PCB pad/extraction.
const pad = a.network({model:models.electrical.attenuatorPad,ports:{input:'p1',output:'p2'}});
const extracted = z.network;
const cascade = a.connect({networks:[extracted,pad],connections:[{from:extracted.port('output'),to:pad.port('input')}],
  ports:{input:extracted.port('input'),output:pad.port('output')}});
const c = await a.networkParameters().network(cascade)
  .frequency({startHz:1e6,stopHz:1e9,points:41,scale:'linear'}).representation('S').referenceOhm(50).run();
a.plot({title:'Line + Attenuator Pad S21 magnitude',data:c.parameter({out:'output',in:'input'}).magnitude()});
a.plot({title:'Line + Attenuator Pad S21 phase',data:c.parameter({out:'output',in:'input'}).phaseDeg()});
