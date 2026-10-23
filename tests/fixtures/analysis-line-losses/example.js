// Standalone hypotheses; no PCB, imported resources or hidden material defaults.
const a=analysis();
const copper={conductivitySM:5.8e7,state:'Smooth copper hypothesis at fixed temperature'};
const band={startHz:5e8,stopHz:1e9,points:11,scale:'linear'};
const line=a.line({kind:'microstrip',widthMm:0.3,copperThicknessMm:0.035,dielectricHeightMm:0.18,lengthMm:20,
  material:{relativePermittivity:4.2,lossTangent:0.015,state:'Constant dielectric hypothesis within requested band; no mask'},
  conductors:{signal:copper,referencePlanes:{bottom:{thicknessMm:0.035,material:copper}}}});
const z=await a.impedance().target(line).method('cross-section').frequency(band).run();
a.plot({title:'Frequency-dependent RLGC and contributions',data:z.rlgc});
a.plot({title:'Zc real',data:z.characteristicImpedance.real()});
a.plot({title:'Zc imaginary',data:z.characteristicImpedance.imaginary()});
a.plot({title:'Phase delay beta length / omega',data:z.delay});
const response=await a.networkParameters().network(z.network).frequency(band).representation('S').referenceOhm(50).run();
a.plot({title:'S21 magnitude (includes mismatch)',data:response.parameter({out:'output',in:'input'}).magnitude()});
a.plot({title:'S21 phase',data:response.parameter({out:'output',in:'input'}).phaseDeg()});
