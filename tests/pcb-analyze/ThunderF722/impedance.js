// e216 BOTTOM UART5_RX, (28.69,27.0605) -> (28.69,19.0596), width .254 mm.
// One 8.0009 mm section; ideal opposite plane/epsilon/thickness are hypotheses.
const a = analysis();
const line = a.line({kind:'microstrip',widthMm:0.254,copperThicknessMm:0.035,
  dielectricHeightMm:0.2,lengthMm:8.0009,
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
for (const [out,input,label] of [['input','input','S11'],['output','input','S21']]) {
  const p = r.parameter({out,in:input});
  a.plot({title:label+' magnitude',data:p.magnitude()});
  a.plot({title:label+' phase',data:p.phaseDeg()});
}
