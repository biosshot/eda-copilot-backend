// Authored electrical hypotheses. A board context is provenance, not extraction.
const a=typeof board==='undefined'?analysis():analysis({board});
const binding=typeof board==='undefined'?{fixture:'standalone'}:board.components.filter(c=>['R1','U2','J1','J2'].includes(c.designator)).map(c=>({designator:c.designator,pads:board.pads.filter(p=>p.component===c.designator).map(p=>({id:p.id,number:p.number,net:p.net}))}));
a.log({level:'warning',message:'core.binding',data:{binding,assumption:'Explicit authored RLC/temporal models. Board contacts are recorded only as provenance; no PCB copper/trace extraction or channel claim.'}});
const circuit=a.circuit({nodes:['in','out','gnd'],reference:'gnd',ports:[{id:'input',positive:'in',negative:'gnd'},{id:'output',positive:'out',negative:'gnd'}],elements:[{id:'R',kind:'model',model:resistor({designator:typeof board==='undefined'?'R-authored':'R1',resistanceOhm:1000}),pins:{p:'in',n:'out'}},{id:'C',kind:'capacitor',nodes:['out','gnd'],capacitanceF:1e-9,ideal:true}]});
const frequencies=Array.from({length:41},(_,i)=>1000+(100000-1000)*(i/40));
const ac=await a.pi().circuit(circuit).domain('frequency').sources([{port:circuit.port('input'),model:{kind:'linear-driver',lowV:0,highV:1,outputResistanceOhm:50},voltageSpectrum:{frequencyHz:frequencies,values:frequencies.map(()=>({real:1,imaginary:0.5})),unit:'V'}}]).loads([{port:circuit.port('output'),model:{kind:'rc-input',capacitanceF:1e-9,resistanceOhm:10000}}]).frequency({startHz:1000,stopHz:100000,points:41,scale:'linear'}).run();
a.log({level:'info',message:'core.case',data:{name:'ac',resultId:ac.id}});
for(const [name,data]of [['input',ac.voltage(circuit.port('input')).magnitude()],['output',ac.voltage(circuit.port('output')).magnitude()],['transfer',ac.transfer({from:circuit.port('input'),to:circuit.port('output')}).magnitude()],['phase',ac.transfer({from:circuit.port('input'),to:circuit.port('output')}).phaseDeg()]])a.plot({title:'ac:'+name,data});
const axis=[0,...Array.from({length:161},(_,i)=>10**(-1+i/20))];
const sampled=a.model({id:'authored-sampled-rc',kind:'network',ports:['p'],analyses:['networkParameters','si'],source:'Independent one-pole Y formula sampled before native fitting; closure authored explicitly',data:{format:'network-parameters',representation:'Y',referenceTopology:'common',nature:'passive',frequenciesHz:axis,parameters:axis.map(f=>{const q=f/73;return[[{real:0.002+0.01*q*q/(1+q*q),imaginary:0.01*q/(1+q*q)}]];}),temporal:{method:'passive-relaxation',normalizationOhm:50,highFrequency:{conductanceS:[0.012],capacitanceF:[0]}}}});
const imported=a.network({model:sampled,ports:{output:'p'}});
for(const [name,tolerance,step]of [['sampled',1e-5,1e-5],['sampled refined',2.5e-6,1e-5]]){
 const r=await a.si().network(imported).drivers([{port:imported.port('output'),model:{kind:'linear-driver',lowV:0,highV:1,outputResistanceOhm:50},stimulus:{kind:'waveform',voltageV:{kind:'step',from:0,to:1,atS:0,riseS:1e-4}}}]).receivers([{port:imported.port('output'),model:{kind:'rc-input',capacitanceF:1e-12}}]).time({stopS:0.02,sampleStepS:step}).numerics({relativeTolerance:tolerance,absoluteTolerance:1e-9}).run();
 a.log({level:'info',message:'core.case',data:{name,resultId:r.id}});a.plot({title:name+':output',data:r.voltage(imported.port('output'))});
}
const wave=(id,delayS,gain)=>a.model({id,kind:'network',ports:['in','out'],analyses:['networkParameters','si'],source:'Exact authored real-K causal wave model; not fitted from a finite frequency table',data:{format:'delayed-scattering',referenceTopology:'common',nature:'passive',referenceOhm:50,scattering:[[0,gain],[gain,0]],delayS}});
const line=a.network({model:wave('two-ns-line',2e-9,1),ports:{input:'in',output:'out'}});
for(const [name,resistance,capacitance]of [['matched',50,1e-15],['reflected',100,1e-15]]){
 const r=await a.si().network(line).drivers([{port:line.port('input'),model:{kind:'linear-driver',lowV:0,highV:1,outputResistanceOhm:25},stimulus:{kind:'waveform',voltageV:{kind:'step',from:0,to:1,atS:0,riseS:1e-9}}}]).receivers([{port:line.port('output'),model:{kind:'rc-input',resistanceOhm:resistance,capacitanceF:capacitance}}]).time({stopS:30e-9,sampleStepS:0.1e-9}).numerics({relativeTolerance:1e-6,absoluteTolerance:1e-9}).run();
 a.log({level:'info',message:'core.case',data:{name,resultId:r.id}});for(const port of ['input','output'])a.plot({title:name+':'+port,data:r.voltage(line.port(port))});
}
const pad=a.network({model:wave('half-voltage-pad',0,0.5),ports:{input:'in',output:'out'}});
const cascade=a.connect({networks:[pad,line],connections:[{from:pad.port('output'),to:line.port('input')}],ports:{input:pad.port('input'),output:line.port('output')}});
const cascaded=await a.networkParameters().network(cascade).frequency({startHz:1e7,stopHz:1e8,points:21,scale:'linear'}).representation('S').referenceOhm(50).run();
a.log({level:'info',message:'core.case',data:{name:'cascade frequency',resultId:cascaded.id}});
for(const out of ['input','output'])for(const input of ['input','output'])a.plot({title:'cascade:'+out+'<-'+input,data:cascaded.parameter({out,in:input}).magnitude()});
a.check({name:'intentional failed hypothesis',actual:ac.voltage(circuit.port('output')).magnitude().max(),max:0,unit:'V'});
a.check({name:'execution continues after failed check',actual:ac.voltage(circuit.port('output')).magnitude().max(),min:0.1,unit:'V'});
