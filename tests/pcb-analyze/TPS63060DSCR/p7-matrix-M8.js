// M8 assembled: TPS63060DSCR SW1 (L1.1->U1.1) coupled to sensitive FB node (R1.2->U1.8); 4 ports, layered differential target like coupled-pair.js.
const a=analysis({board});
const sw=a.channel({from:a.port({positive:pad('L1','1'),negative:pad('U1','11')}),to:a.port({positive:pad('U1','1'),negative:pad('U1','4')})});
const fb=a.channel({from:a.port({positive:pad('R1','2'),negative:pad('R2','2')}),to:a.port({positive:pad('U1','8'),negative:pad('U1','7')})});
const pair=a.differential({positive:sw,negative:fb});
const layers=stackup({"boardThicknessMm": 1.6, "viaPlatingThicknessUm": 25, "platingMaterial": "cu", "diameterConvention": "finished_hole", "layers": [{"kind": "copper", "name": "TOP", "thicknessMm": 0.035, "material": "cu"}, {"kind": "dielectric", "name": "assumed-gap-0", "thicknessMm": 1.53, "material": "d"}, {"kind": "copper", "name": "BOTTOM", "thicknessMm": 0.035, "material": "cu"}]});
const materials={cu:{conductivitySM:58e6,state:"Benchmark copper 25 C"},d:{relativePermittivity:4.2,state:"Benchmark lossless dielectric; actual data missing"}};
const z=await a.impedance().target(pair).method('layered').stackup(layers).materials(materials).frequency({startHz:1e3,stopHz:4e7,points:6,scale:"log"}).budget({maxSeconds:3600,maxMemoryMiB:2048}).run();
a.plot({title:'M8 SW-FB coupling network',kind:'table',data:z.solution});
