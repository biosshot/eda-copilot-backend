// P7 v1 stage 5. Benchmark hypotheses: stack, materials and termination are not measured. The runner replaces the BAND_HZ token; the file runs as is.
// coupled-pair: two adjacent signal nets SW1 (L1.1 to U1.1) and SW2 (L1.2 to U1.10), four oriented ports over the GND pads, layered differential target.
const bandHz=/*BAND_HZ*/1e6;
const a=analysis({board});
const sw1=a.channel({from:a.port({positive:pad('L1','1'),negative:pad('U1','11')}),to:a.port({positive:pad('U1','1'),negative:pad('U1','4')})});
const sw2=a.channel({from:a.port({positive:pad('L1','2'),negative:pad('U1','11')}),to:a.port({positive:pad('U1','10'),negative:pad('U1','7')})});
const pair=a.differential({positive:sw1,negative:sw2});
const layers=stackup({"boardThicknessMm": 1.6, "viaPlatingThicknessUm": 25, "platingMaterial": "cu", "diameterConvention": "finished_hole", "layers": [{"kind": "copper", "name": "TOP", "thicknessMm": 0.035, "material": "cu"}, {"kind": "dielectric", "name": "assumed-gap-0", "thicknessMm": 1.53, "material": "d"}, {"kind": "copper", "name": "BOTTOM", "thicknessMm": 0.035, "material": "cu"}]});
const materials={cu:{conductivitySM:58e6,state:'Benchmark copper 25 C'},d:{relativePermittivity:4.2,state:'Benchmark lossless dielectric; actual data missing'}};
const z=await a.impedance().target(pair).method('layered').stackup(layers).materials(materials).frequencyHz(bandHz).budget({maxSeconds:300,maxMemoryMiB:2048}).run();
a.plot({title:'Coupled pair network',kind:'table',data:z.solution});
