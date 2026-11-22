// P8 stage 6 board pi() on BandAmpV5. Benchmark hypotheses: stack, materials, supply-header output resistance and decap ESR/ESL are not measured.
// Rail VCC_5V (header H1.1 against H1.2 GND as the supply source, hypothesis 100 mOhm). Load: IC pin pair U1.1/U1.2 (SOT-363).
// Rail-return parts declared as hypotheses: C3 (470 pF C0603, ESR 50 mOhm, ESL 0.5 nH), C4 (100 nF C0603, ESR 10 mOhm, ESL 0.6 nH),
// C5 (1 uF C0603, ESR 20 mOhm, ESL 0.6 nH). Ports: 5.
const a=analysis({board});
const src=a.port({positive:pad('H1','1'),negative:pad('H1','2')});
const load=a.port({positive:pad('U1','1'),negative:pad('U1','2')});
const layers=stackup({"boardThicknessMm": 1.6, "viaPlatingThicknessUm": 25, "platingMaterial": "cu", "diameterConvention": "finished_hole", "layers": [{"kind": "copper", "name": "TOP", "thicknessMm": 0.035, "material": "cu"}, {"kind": "dielectric", "name": "assumed-gap-0", "thicknessMm": 1.53, "material": "d"}, {"kind": "copper", "name": "BOTTOM", "thicknessMm": 0.035, "material": "cu"}]});
const materials={cu:{conductivitySM:58e6,state:'Benchmark copper 25 C'},d:{relativePermittivity:4.2,state:'Benchmark lossless dielectric; actual data missing'}};
const supply=a.model({kind:'linear-driver',lowV:0,highV:5,outputResistanceOhm:0.1});
const z=await a.pi().domain('frequency').frequency({startHz:1e3,stopHz:4e7,points:30,scale:"log"})
 .sources([{port:src,voltageV:5,model:supply}])
 .loads([{port:load,currentA:.05}])
 .components([
  capacitor({designator:'C3',pins:{positive:'1',negative:'2'},capacitanceF:470e-12,esrOhm:.05,eslH:.5e-9}),
  capacitor({designator:'C4',pins:{positive:'1',negative:'2'},capacitanceF:100e-9,esrOhm:.01,eslH:.6e-9}),
  capacitor({designator:'C5',pins:{positive:'1',negative:'2'},capacitanceF:1e-6,esrOhm:.02,eslH:.6e-9})])
 .stackup(layers).materials(materials).budget({maxSeconds:3600,maxMemoryMiB:6144}).run();
a.plot({title:'BandAmpV5 VCC_5V board pi impedance',kind:'table',data:z.solution});
