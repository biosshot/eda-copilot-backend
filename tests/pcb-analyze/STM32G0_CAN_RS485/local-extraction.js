// Direct captured-board channel extraction. All copper in the ROI is retained.
// ROI cuts are insulated; stack/material/plating hypotheses are explicit.
const a=analysis({board});
const copper={conductivitySM:58e6,state:'Authored bulk copper at fixed 25 C; no roughness or inferred temperature law'};
const fr4={relativePermittivity:4.2,state:'Authored constant lossless dielectric; dielectric loss not available in local v1'};
const layers=stackup({boardThicknessMm:1.6,viaPlatingThicknessUm:25,platingMaterial:'copper',diameterConvention:'finished_hole',layers:[
 {kind:'copper',name:'TOP',thicknessMm:.035,material:'copper'},
 {kind:'dielectric',name:'authored-FR4',thicknessMm:1.53,material:'fr4'},
 {kind:'copper',name:'BOTTOM',thicknessMm:.035,material:'copper'}]});
const input=a.port({positive:viaId("da3574692d072a0d"),negative:viaId("aa8ff23739620a07"),positiveLayer:"TOP",negativeLayer:"TOP"});
const output=a.port({positive:viaId("da3574692d072a0d"),negative:viaId("aa8ff23739620a07"),positiveLayer:"BOTTOM",negativeLayer:"TOP"});
const scope=region({layers:["TOP","BOTTOM"],polygon:{outer:[{"x": 23.1983, "y": 3.7709}, {"x": 23.9243, "y": 3.7709}, {"x": 23.9243, "y": 4.1229}, {"x": 23.1983, "y": 4.1229}]}});
const channel=a.channel({from:input,to:output,scope:[scope]});
const run=()=>a.impedance().target(channel).method('layered').stackup(layers).materials({copper,fr4})
 .frequencyHz(10000).budget({maxSeconds:300,maxMemoryMiB:2048}).run();
const cold=await run();
a.plot({title:'Selected ROI, physical contacts, current/loss recovery and cache',kind:'table',data:cold.segments});
a.plot({title:'Cold stage timings and validation limitations',kind:'table',data:cold.solution});
const warm=await run();
a.plot({title:'Same-worker model reuse',kind:'table',data:warm.segments});
const band={startHz:1000,stopHz:100000,points:17,scale:'log'};
const z=await a.networkParameters().network(cold.network).frequency(band).representation('Z').run();
a.plot({title:'Input impedance magnitude, conditional local v1',data:z.parameter({out:'input',in:'input'}).magnitude()});
a.plot({title:'Transfer impedance magnitude; common charge retained',data:z.parameter({out:'output',in:'input'}).magnitude()});
a.check({name:'expected physical fail: real copper has positive Joule loss',actual:cold.solution.samples[0].totalLossW,target:0,tolerance:0,unit:'W'});
a.check({name:'continued: native recovery contains positive finite copper loss',actual:cold.solution.samples[0].totalLossW,min:0,unit:'W'});
a.plot({title:'Continued after expected failing check',kind:'table',data:{totalLossW:cold.solution.samples[0].totalLossW,cache:warm.solution.cache}});
const afterCheck=await run();
a.plot({title:'Check-only changes preserve the prepared model',kind:'table',data:afterCheck.segments});
