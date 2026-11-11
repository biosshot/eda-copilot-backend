// P7 v1 stage 5. Benchmark hypotheses: stack, materials and termination are not measured. The runner replaces the BAND_HZ token; the file runs as is.
// pdn-multiport: three ports on the VOUT rail against GND (IC pin U1.9, output caps C3 and C5), one joint passive extraction.
const bandHz=/*BAND_HZ*/1e6;
const a=analysis({board});
const p1=a.port({positive:pad('U1','9'),negative:pad('U1','7')});
const p2=a.port({positive:pad('C3','1'),negative:pad('C3','2')});
const p3=a.port({positive:pad('C5','1'),negative:pad('C5','2')});
const ch12=a.channel({from:p1,to:p2});
const ch13=a.channel({from:p1,to:p3});
const layers=stackup({"boardThicknessMm": 1.6, "viaPlatingThicknessUm": 25, "platingMaterial": "cu", "diameterConvention": "finished_hole", "layers": [{"kind": "copper", "name": "TOP", "thicknessMm": 0.035, "material": "cu"}, {"kind": "dielectric", "name": "assumed-gap-0", "thicknessMm": 1.53, "material": "d"}, {"kind": "copper", "name": "BOTTOM", "thicknessMm": 0.035, "material": "cu"}]});
const materials={cu:{conductivitySM:58e6,state:'Benchmark copper 25 C'},d:{relativePermittivity:4.2,state:'Benchmark lossless dielectric; actual data missing'}};
// The source only sets the spectral demand: a unilateral sine with 1% L1 tail needs F=f0/tanh(2*pi*0.01), so f0 gives the required upper band.
const f0=bandHz*Math.tanh(2*Math.PI*.01);
const driver=a.model({kind:'linear-driver',lowV:0,highV:1,outputResistanceOhm:50});
const receiver=a.model({kind:'rc-input',capacitanceF:1e-12});
const stimulus={kind:'waveform',voltageV:{kind:'sine',offset:0,amplitude:.5,periodS:1/f0,phaseDeg:0}};
const z=await a.si().channels([ch12,ch13]).stackup(layers).materials(materials)
 .drivers([{port:p1,model:driver,stimulus}]).receivers([{port:p2,model:receiver},{port:p3,model:receiver}])
 .time({stopS:10/f0,sampleStepS:1/(20*f0)}).budget({maxSeconds:300,maxMemoryMiB:2048}).run();
a.plot({title:'PDN multiport network',kind:'table',data:z.solution});
