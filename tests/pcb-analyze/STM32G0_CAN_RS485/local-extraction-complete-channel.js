// Full remote RS485_B candidate; no numerical admission or 5% accuracy claimed.
// Numerical domain expansion and physical guards must pass before a model is returned.
const a=analysis({board});
const copper={conductivitySM:58e6,state:'Authored bulk copper at fixed 25 C; no roughness or inferred temperature law'};
const fr4={relativePermittivity:4.2,state:'Authored constant lossless dielectric; dielectric loss not available in local v1'};
const layers=stackup({boardThicknessMm:1.6,viaPlatingThicknessUm:25,platingMaterial:'copper',diameterConvention:'finished_hole',layers:[
 {kind:'copper',name:'TOP',thicknessMm:.035,material:'copper'},
 {kind:'dielectric',name:'authored-FR4',thicknessMm:1.53,material:'fr4'},
 {kind:'copper',name:'BOTTOM',thicknessMm:.035,material:'copper'}]});
const input=a.port({positive:pad("U5","7"),negative:pad("U5","5"),positiveLayer:"TOP",negativeLayer:"TOP"});
const output=a.port({positive:pad("J5","2"),negative:pad("J5","3"),positiveLayer:"TOP",negativeLayer:"TOP"});
const channel=a.channel({from:input,to:output,});
const run=()=>a.impedance().target(channel).method('layered').stackup(layers).materials({copper,fr4})
 .frequencyHz(10000).budget({maxSeconds:300,maxMemoryMiB:2048}).run();const result=await run();
a.plot({title:'Automatic finite domains, physical warning decisions and currents',kind:'table',data:result.solution});
