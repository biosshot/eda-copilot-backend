// Explicit author models; board contacts supply provenance only.
const a=typeof board==='undefined'?analysis():analysis({board});
const portContacts=typeof board==='undefined'?null:(()=>{
 const stm=board.pads.some(p=>p.component==='U3'&&p.number==='48'&&p.net==='CAN_TX');
 const pick=(component,number,net)=>{const p=board.pads.find(p=>p.component===component&&p.number===number&&p.net===net);if(!p)throw Error('Frozen authored physical port missing');return{id:p.id,component:p.component,number:p.number,net:p.net};};
 const ground=board.pads.find(p=>p.net==='GND');if(!ground)throw Error('Explicit common return contact missing');
 return{input:stm?pick('U3','48','CAN_TX'):pick('U2','58','UART1_TX'),output:stm?pick('U4','1','CAN_TX'):pick('TX1','1','UART1_TX'),return:{id:ground.id,component:ground.component,number:ground.number,net:ground.net}};
})();
const physicalProvenance=' External coupon terminals: '+JSON.stringify(portContacts);

const binding=typeof board==='undefined'?{fixture:'standalone'}:board.components.filter(c=>['R1','U2'].includes(c.designator)).map(c=>({designator:c.designator,pads:board.pads.filter(p=>p.component===c.designator).map(p=>({id:p.id,number:p.number,net:p.net}))}));
a.log({level:'warning',message:'mixed.binding',data:{binding,portContacts,assumption:'Authored delay and RLC models on recorded board contacts; no PCB trace extraction or channel claim.'}});
const line=(id,delayS)=>a.network({model:a.model({id,kind:'network',ports:['a','b'],analyses:['networkParameters','si'],source:'Authored exact causal 50 Ohm line'+physicalProvenance,data:{format:'delayed-scattering',referenceTopology:'common',nature:'passive',referenceOhm:50,scattering:[[0,1],[1,0]],delayS}}),ports:{input:'a',output:'b'}});
const lumped=(id,R,L,C,shunt=false)=>a.network({model:a.model({id,kind:'network',ports:['a','b'],analyses:['networkParameters','si'],source:'Authored discrete physical filter'+physicalProvenance,data:{format:'coupled-rlc',referenceTopology:'grounded',potentialNodes:shunt?['v']:['a','b'],currentDofs:shunt?[]:['series'],incidence:shunt?[]:[[0,0,1],[1,0,-1]],portIncidence:shunt?[[0,0,1],[0,1,1]]:[[0,0,1],[1,1,1]],resistanceOhm:shunt?[]:[[0,0,R]],inductanceH:shunt?[]:[[0,0,L]],capacitanceF:[[shunt?0:1,shunt?0:1,C]]}}),ports:{input:'a',output:'b'}});
const cascade=(networks)=>a.connect({networks,connections:networks.slice(0,-1).map((n,i)=>({from:n.port('output'),to:networks[i+1].port('input')})),ports:{input:networks[0].port('input'),output:networks.at(-1).port('output')}});
const rc=cascade([line('rc-line',2e-9),lumped('rc-filter',50,0,30e-12)]);
const rlc=cascade([cascade([line('rlc-first',2e-9),lumped('rlc-filter',5,50e-9,20e-12)]),line('rlc-last',3e-9)]);
const reflected=cascade([line('bounce-line',2e-9),lumped('bounce-C',0,0,20e-12,true)]);
// Existing rc-input requires positive C;1aF adds <0.1uV front perturbation, below the1uV reference floor.
for(const [name,network,Rs,RL,C,stop]of [['rc',rc,50,50,1e-18,16e-9],['rlc',rlc,50,50,1e-18,25e-9],['reflections',reflected,25,75,2e-12,16e-9]]){
 for(const [suffix,tolerance]of [['',1e-5],[' refined',2.5e-6]]){
  const r=await a.si().network(network).drivers([{port:network.port('input'),model:{kind:'linear-driver',lowV:0,highV:1,outputResistanceOhm:Rs},stimulus:{kind:'waveform',voltageV:{kind:'step',from:0,to:1,atS:0,riseS:0.5e-9}}}]).receivers([{port:network.port('output'),model:{kind:'rc-input',resistanceOhm:RL,capacitanceF:C}}]).time({stopS:stop,sampleStepS:0.1e-9}).numerics({relativeTolerance:tolerance,absoluteTolerance:tolerance*1e-3}).run();
  a.log({level:'info',message:'mixed.case',data:{name:name+suffix,resultId:r.id}});
  for(const port of ['input','output'])a.plot({title:name+suffix+':'+port,data:r.voltage(network.port(port))});
 }
 const f=await a.networkParameters().network(network).frequency({startHz:1e6,stopHz:1e9,points:21,scale:'linear'}).representation('S').referenceOhm(50).run();
 a.log({level:'info',message:'mixed.case',data:{name:name+' frequency',resultId:f.id}});
 for(const out of ['input','output'])for(const input of ['input','output'])a.plot({title:name+':'+out+'<-'+input,data:f.parameter({out,in:input}).magnitude()});
}
