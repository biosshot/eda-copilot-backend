// TOP parallel SWCLK 5d83ad1362a81e70 and SWDIO f91dce6599297e96.
// Width .2 mm, x=-4.5101 / -4.0099; center spacing .5002, gap .3002.
// Common parallel section y=-.4559..2.5951, length 3.051 mm. Not a differential bus.
const a = analysis();
const line = a.line({kind:'coupled-microstrip',widthMm:0.2,gapMm:0.3002,
  copperThicknessMm:0.035,dielectricHeightMm:0.3,lengthMm:3.051,
  material:{relativePermittivity:4.2,lossTangent:0,state:'Board-derived parallel geometry; explicit ideal stackup hypothesis, PEC, no mask; reference plane not verified'}});
const z = await a.impedance().target(line).method('cross-section')
  .frequency({startHz:1e6,stopHz:1e9,points:41,scale:'linear'}).run();
a.plot({title:'Modal impedances',data:z.characteristicImpedance});
a.plot({title:'RLGC',data:z.rlgc});
a.plot({title:'Modal delays',data:z.delay});
const r = await a.networkParameters().network(z.network)
  .frequency({startHz:1e6,stopHz:1e9,points:41,scale:'linear'}).representation('S').referenceOhm(50).run();
for (const [out,input,label] of [['positiveInput','positiveInput','S11'],['positiveOutput','positiveInput','S21'],
  ['negativeInput','positiveInput','Near crosstalk'],['negativeOutput','positiveInput','Far crosstalk']]) {
  const p = r.parameter({out,in:input});
  a.plot({title:label+' magnitude',data:p.magnitude()});
  a.plot({title:label+' phase',data:p.phaseDeg()});
}
a.check({name:'expected fail: transmission not zero',actual:r.parameter({out:'positiveOutput',in:'positiveInput'}).magnitude().min(),target:0,tolerance:0.01,unit:'1'});
a.check({name:'continued: nonzero transmission',actual:r.parameter({out:'positiveOutput',in:'positiveInput'}).magnitude().min(),min:0.1,unit:'1'});
