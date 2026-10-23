// BandAmpV5: dcc17162579803b7 / RF_IN_PORT / TOP
// Exact selected segment [[[-15.8299, 0], [-5.2, 0]]] mm; no artificial extension.
const a = analysis();
const band = {startHz:5e8,stopHz:1e9,points:41,scale:'linear'};
const copper = {conductivitySM:5.8e7,state:'Explicit smooth copper hypothesis at fixed temperature; no roughness'};
const conductors = {signal:copper,referencePlanes:{bottom:{thicknessMm:0.035,material:copper}}};
let finalResponse;
for (const mode of [
  {name:'PEC/lossless',tangent:0},
  {name:'dielectric-only',tangent:0.015},
  {name:'conductor-only',tangent:0,conductors},
  {name:'combined',tangent:0.015,conductors},
]) {
  const line = a.line({...{"kind":"microstrip","widthMm":0.6736,"copperThicknessMm":0.035,"dielectricHeightMm":0.4,"lengthMm":10.6299},
    material:{relativePermittivity:4.2,lossTangent:mode.tangent,state:'Board-derived straight geometry; explicit constant dielectric/infinite reference hypothesis within band; no mask; physical PCB reference not verified'},
    ...(mode.conductors?{conductors:mode.conductors}:{})});
  const z = await a.impedance().target(line).method('cross-section').frequency(band)
    .mesh({maxNodes:300000,maxTriangles:600000,maxEdgeMm:0.016666666666666666,domainScale:16}).budget({maxSeconds:120,maxMemoryMiB:1024}).run();
  a.plot({title:mode.name+' RLGC',data:z.rlgc});
  a.plot({title:mode.name+' complex Zc',data:z.characteristicImpedance});
  a.plot({title:mode.name+' phase delay',data:z.delay});
  const p = z.rlgc.data;
  const axis = Array.from({length:41},(_,i)=>5e8+i*5e8/40);
  const modal = p.samples ? p.samples.map(s=>s.modes) : axis.map(f=>p.modes.map(m=>({
    name:m.name,resistanceOhmPerM:0,internalInductanceHPerM:0,conductanceSPerM:0,
    alphaNpPerM:0,betaRadPerM:2*Math.PI*f*m.delaySPerM,
    characteristicImpedanceOhm:{real:m.impedanceOhm,imaginary:0},phaseDelayS:m.delaySPerM*0.0106299})));
  for(let k=0;k<modal[0].length;k++) {
    for(const [label,unit,get] of [
      ['R','Ohm/m',m=>m.resistanceOhmPerM],['Linternal','H/m',m=>m.internalInductanceHPerM],
      ['G','S/m',m=>m.conductanceSPerM],['alpha','Np/m',m=>m.alphaNpPerM],
      ['beta','rad/m',m=>m.betaRadPerM],['Zc real','Ohm',m=>m.characteristicImpedanceOhm.real],
      ['Zc imaginary','Ohm',m=>m.characteristicImpedanceOhm.imaginary],['phase delay','s',m=>m.phaseDelayS],
    ]) a.plot({title:mode.name+' '+modal[0][k].name+' '+label,data:{kind:'series',unit,axis:{name:'frequencyHz',unit:'Hz',values:axis},values:modal.map(ms=>get(ms[k])),validity:z.rlgc.validity,diagnostics:[]}});
  }
  const r = await a.networkParameters().network(z.network).frequency(band).representation('S').referenceOhm(50).run();
  for(const [out,input,label] of [['input','input','S11'],['output','input','S21']]) {
    const s=r.parameter({out,in:input});
    a.plot({title:mode.name+' '+label+' magnitude',data:s.magnitude()});
    a.plot({title:mode.name+' '+label+' phase',data:s.phaseDeg()});
  }
  // Explicit model of Attenuator Pad; not an inferred PCB pad model.
  const pad=a.network({model:models.electrical.attenuatorPad,ports:{input:'p1',output:'p2'}});
  const extracted=z.network;
  const cascade=a.connect({networks:[extracted,pad],connections:[{from:extracted.port('output'),to:pad.port('input')}],ports:{input:extracted.port('input'),output:pad.port('output')}});
  const c=await a.networkParameters().network(cascade).frequency(band).representation('S').referenceOhm(50).run();
  a.plot({title:mode.name+' Line + Attenuator Pad S21 magnitude',data:c.parameter({out:'output',in:'input'}).magnitude()});
  a.plot({title:mode.name+' Line + Attenuator Pad S21 phase',data:c.parameter({out:'output',in:'input'}).phaseDeg()});
  finalResponse=r;
}
const transmission=finalResponse.parameter({out:'output',in:'input'}).magnitude();
a.check({name:'expected fail: transmission is not zero',actual:transmission.min(),target:0,tolerance:0.01,unit:'1'});
a.check({name:'continued: finite transmission',actual:transmission.min(),min:0.1,max:1,unit:'1'});
a.plot({title:'continued transmission magnitude',data:transmission});
