// Benchmark hypotheses: not measured stack, materials or termination. Complete selected copper channel.
const a=analysis({board});
const input=a.port({positive:pad('RF1','2'),negative:pad('RF1','1')});
const output=a.port({positive:pad('C1','1'),negative:pad('U1','2')});
const layers=stackup({"boardThicknessMm": 1.6, "viaPlatingThicknessUm": 25, "platingMaterial": "cu", "diameterConvention": "finished_hole", "layers": [{"kind": "copper", "name": "TOP", "thicknessMm": 0.035, "material": "cu"}, {"kind": "dielectric", "name": "assumed-gap-0", "thicknessMm": 1.53, "material": "d"}, {"kind": "copper", "name": "BOTTOM", "thicknessMm": 0.035, "material": "cu"}]});
const z=await a.impedance().target(a.channel({from:input,to:output})).method('layered').stackup(layers).materials({cu:{conductivitySM:58e6,state:'Benchmark copper 25 C'},d:{relativePermittivity:4.2,state:'Benchmark lossless dielectric; actual data missing'}}).frequency({startHz:1000,stopHz:10000,points:3,scale:'log'}).budget({maxSeconds:300,maxMemoryMiB:2048}).run();
a.plot({title:'Selected full copper channel',kind:'table',data:z.solution});
