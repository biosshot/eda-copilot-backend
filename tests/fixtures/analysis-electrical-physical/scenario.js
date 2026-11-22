// Supplied full conductor models, not extraction of mutual C/L between objects.
const a=analysis();
function cell(second=false){return a.model({id:second?'cell2':'cell1',kind:'network',source:'Authored full-C conductors; no intermodel EM extraction',validity:{status:'conditional',reason:'Authored synthetic coupon only',domain:{bandHz:[0,1e9]}},analyses:['si','networkParameters'],ports:['join','probe'],data:{format:'coupled-rlc',referenceTopology:'infinity',potentialNodes:['p','n'],currentDofs:['j'],incidence:[[0,0,1],[1,0,-1]],portIncidence:[[0,0,1],[1,0,-1],[0,1,1],[1,1,-1]],resistanceOhm:[[0,0,100]],inductanceH:[[0,0,1e-6]],capacitanceF:second?[[0,0,7e-9],[0,1,-3e-9],[1,0,-3e-9],[1,1,7e-9]]:[[0,0,3e-9],[0,1,-2e-9],[1,0,-2e-9],[1,1,3e-9]],conservedPotentialModes:[[1,1]],portTerminals:[{positive:{node:'p'},negative:{node:'n'}},{positive:{node:'p'},negative:{node:'n'}}]}});}
function graph(contacts){const left=a.network({model:cell(),ports:{join:'join',probe:'probe'}}),right=a.network({model:cell(true),ports:{join:'join',probe:'probe'}});return{left,right,network:a.connect({networks:[left,right],connections:[{from:left.port('join'),to:right.port('join'),...(contacts?{contacts}:{})}],ports:{left:left.port('probe'),right:right.port('probe')}})};}
function initial(g,common=false){return{kind:'physical-state',steady:false,potentialsV:[g.left,g.right].flatMap(n=>[{instanceId:n.id,node:'p',value:2},{instanceId:n.id,node:'n',value:common?2:1}]),currentsA:[g.left,g.right].map(n=>({instanceId:n.id,dof:'j',value:0}))};}
for(const [name,contacts] of [['both',undefined],['single','positive']]){
 const g=graph(contacts);
 for(const [suffix,rel,maxStepS]of [['',1e-6,1e-9],[' refined',2.5e-7,.5e-9],[' common',1e-6,1e-9]]){
  const r=await a.si().network(g.network).initial(initial(g,suffix===' common')).time({stopS:5e-6,sampleStepS:1e-8}).numerics({relativeTolerance:rel,absoluteTolerance:rel*1e-4,maxStepS}).budget({maxSeconds:60,maxMemoryMiB:256}).run();
  a.log({level:'info',message:'physical.case',data:{name:name+suffix,resultId:r.id}});
  a.plot({title:name+suffix+':left',data:r.voltage(g.network.port('left'))});
 }
 const f=await a.networkParameters().network(g.network).frequency({startHz:1e3,stopHz:1e8,points:13,scale:'log'}).representation('Z').run();
 a.log({level:'info',message:'physical.case',data:{name:name+' frequency',resultId:f.id}});
 a.plot({title:name+' frequency',data:f.parameter({out:'left',in:'left'}).magnitude()});
}
// Public negatives include missing physical charge and forbidden delay contacts.
for(const kind of ['charge','weld','steady','permission','delay','table']){
 try{
  if(kind==='delay'||kind==='table'){
   const model=kind==='delay'?{format:'delayed-scattering',referenceTopology:'common',nature:'passive',referenceOhm:50,scattering:[[0]],delayS:1e-9}:{format:'network-parameters',referenceTopology:'common',nature:'passive',representation:'Z',frequenciesHz:[1e3,1e8],parameters:[[[{real:50,imaginary:0}]],[[{real:50,imaginary:0}]]]};
   const left=a.network({model:a.model({id:kind,kind:'network',ports:['p'],data:model}),ports:{p:'p'}}),right=a.network({model:a.model({id:kind+'2',kind:'network',ports:['p','q'],data:kind==='delay'?{...model,scattering:[[0,1],[1,0]]}:{...model,parameters:[[[{real:50,imaginary:0},{real:0,imaginary:0}],[{real:0,imaginary:0},{real:50,imaginary:0}]],[[{real:50,imaginary:0},{real:0,imaginary:0}],[{real:0,imaginary:0},{real:50,imaginary:0}]]]}}),ports:{p:'p',q:'q'}});
   const n=a.connect({networks:[left,right],connections:[{from:left.port('p'),to:right.port('p'),contacts:'positive'}],ports:{out:right.port('q')}});
   await a.networkParameters().network(n).frequencyHz(1e4).representation('Z').run();
  }else{
   const g=graph('both'),state=initial(g);if(kind==='weld')state.potentialsV[2].value=3;if(kind==='steady')state.steady=true;
   let builder=a.si().network(g.network).time({stopS:1e-8,sampleStepS:1e-9});
   if(kind==='permission'){const denied=a.network({model:{...cell(),analyses:['networkParameters']},ports:{join:'join',probe:'probe'}});builder=a.si().network(denied).time({stopS:1e-8,sampleStepS:1e-9});}
   if(kind==='charge')builder=builder.initial('steady');else builder=builder.initial(state);
   await builder.run();
  }
  throw Error('Unexpected acceptance '+kind);
 }catch(e){if(String(e).includes('Unexpected acceptance'))throw e;a.log({level:'info',message:'physical.negative',data:{kind,error:String(e)}});}
}
