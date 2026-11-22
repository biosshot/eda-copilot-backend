// P8 stage 6 board pi() on STM32G0_CAN_RS485. Benchmark hypotheses: stack, materials, LDO output resistance and every decap ESR/ESL are not measured.
// Rail +3V3 (U2.5 LDO output against GND U2.2), same binding as tests/pcb-analyze/STM32G0_CAN_RS485/rlc.js.
// Source: U2.5/U2.2 with a 200 mOhm output-resistance hypothesis. Load: MCU U3.4/U3.7 (VDD/VSS pad pair).
// Rail-return decaps declared as hypotheses: 10 uF C0805 (ESR 5 mOhm, ESL 1 nH): C8, C11; 1 uF C0603 (ESR 20 mOhm, ESL 0.6 nH): C13, C22;
// 100 nF C0603 (ESR 10 mOhm, ESL 0.6 nH): C9, C10, C12, C14, C15, C19, C21. Ports: 13.
const a=analysis({board});
const src=a.port({positive:pad('U2','5'),negative:pad('U2','2')});
const load=a.port({positive:pad('U3','4'),negative:pad('U3','7')});
const cap=(d,c,esr,esl)=>capacitor({designator:d,pins:{positive:'1',negative:'2'},capacitanceF:c,esrOhm:esr,eslH:esl});
const layers=stackup({"boardThicknessMm": 1.6, "viaPlatingThicknessUm": 25, "platingMaterial": "cu", "diameterConvention": "finished_hole", "layers": [{"kind": "copper", "name": "TOP", "thicknessMm": 0.035, "material": "cu"}, {"kind": "dielectric", "name": "assumed-gap-0", "thicknessMm": 1.53, "material": "d"}, {"kind": "copper", "name": "BOTTOM", "thicknessMm": 0.035, "material": "cu"}]});
const materials={cu:{conductivitySM:58e6,state:'Benchmark copper 25 C'},d:{relativePermittivity:4.2,state:'Benchmark lossless dielectric; actual data missing'}};
const ldo=a.model({kind:'linear-driver',lowV:0,highV:3.3,outputResistanceOhm:0.2});
const z=await a.pi().domain('frequency').frequency({startHz:1e3,stopHz:4e7,points:30,scale:'log'})
 .sources([{port:src,voltageV:3.3,model:ldo}])
 .loads([{port:load,currentA:.1}])
 .components([
  cap('C8',10e-6,.005,1e-9), cap('C11',10e-6,.005,1e-9),
  cap('C13',1e-6,.02,.6e-9), cap('C22',1e-6,.02,.6e-9),
  cap('C9',100e-9,.01,.6e-9), cap('C10',100e-9,.01,.6e-9), cap('C12',100e-9,.01,.6e-9), cap('C14',100e-9,.01,.6e-9),
  cap('C15',100e-9,.01,.6e-9), cap('C19',100e-9,.01,.6e-9), cap('C21',100e-9,.01,.6e-9)])
 .stackup(layers).materials(materials).budget({maxSeconds:1800,maxMemoryMiB:6144}).run();
a.plot({title:'STM32G0 +3V3 board pi impedance',kind:'table',data:z.solution});
