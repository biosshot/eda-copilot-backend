// P7 v1 stage 5. Benchmark hypotheses: stack, materials and termination are not measured. The runner replaces the BAND_HZ token; the file runs as is.
// band-step: one short two-port channel (VIN decoupling caps C1 to C2) at a single frequency step.
const bandHz=/*BAND_HZ*/1e4;
const a=analysis({board});
const input=a.port({positive:pad('C1','1'),negative:pad('C1','2')});
const output=a.port({positive:pad('C2','1'),negative:pad('C2','2')});
const layers=stackup({"boardThicknessMm": 1.6, "viaPlatingThicknessUm": 25, "platingMaterial": "cu", "diameterConvention": "finished_hole", "layers": [{"kind": "copper", "name": "TOP", "thicknessMm": 0.035, "material": "cu"}, {"kind": "dielectric", "name": "assumed-gap-0", "thicknessMm": 1.53, "material": "d"}, {"kind": "copper", "name": "BOTTOM", "thicknessMm": 0.035, "material": "cu"}]});
const materials={cu:{conductivitySM:58e6,state:'Benchmark copper 25 C'},d:{relativePermittivity:4.2,state:'Benchmark lossless dielectric; actual data missing'}};
const z=await a.impedance().target(a.channel({from:input,to:output})).method('layered').stackup(layers).materials(materials).frequencyHz(bandHz).budget({maxSeconds:300,maxMemoryMiB:2048}).run();
a.plot({title:'Band step channel',kind:'table',data:z.solution});
