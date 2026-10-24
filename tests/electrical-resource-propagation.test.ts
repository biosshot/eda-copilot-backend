import test from 'node:test';
import assert from 'node:assert/strict';
import {createFrequencyExecutor} from '../src/analyze/analysis-dsl/frequency-executor.ts';
import {networkResource} from '../src/analyze/analysis-dsl/frequency-model.ts';
const z=(real:number,imaginary=0)=>({real,imaginary});
function resource(kind:string):any{
 const common={id:kind,kind:'network',ports:['a','b'],analyses:['networkParameters','si'],validity:{status:'conditional',reason:'authored material limitation',domain:{bandHz:[0,5e6],authorLimit:'retained'}}};
 if(kind==='delay')return{...common,data:{format:'delayed-scattering',referenceTopology:'common',nature:'passive',referenceOhm:{a:25,b:100},scattering:[[.1,.5],[.5,.2]],delayS:1e-9}};
 if(kind==='coupled')return{...common,analyses:['networkParameters'],data:{format:'coupled-rlc',referenceTopology:'grounded',potentialNodes:['v1','v2'],currentDofs:['r1','r2'],incidence:[[0,0,1],[1,1,1]],portIncidence:[[0,0,1],[1,1,1]],resistanceOhm:[[0,0,10],[1,1,20]],inductanceH:[],capacitanceF:[]}};
 const frequenciesHz=[0,1e6,1e7];return{...common,data:{format:'network-parameters',representation:'Y',referenceTopology:'common',nature:'passive',frequenciesHz,parameters:frequenciesHz.map(f=>{const q=f/73,re=.01*q*q/(1+q*q),im=.01*q/(1+q*q);return[[z(.002+re,im),z(0)],[z(0),z(.004+re,im)]];}),temporal:{method:'passive-relaxation',normalizationOhm:50,highFrequency:{conductanceS:[.012,.014],capacitanceF:[0,0]}}}};
}
function task(model:any,mode='single',frequency=1e6):any{
 const handle=(id:string)=>({id,kind:'network',contextId:'a'}),port=(owner:string,name:string)=>({id:owner+name,kind:'port',contextId:'a'});
 const descriptors:any[]=[];
 for(const id of mode==='cascade'?['n1','n2']:['n1']){
  descriptors.push({id,kind:'network',input:{model,ports:{left:'a',right:'b'}}});
  for(const name of ['left','right'])descriptors.push({id:id+name,kind:'port',input:{owner:id,name}});
 }
 let root='n1';if(mode!=='single'){
  root='joined';descriptors.push({id:root,kind:'connect',input:{networks:mode==='cascade'?[handle('n1'),handle('n2')]:[handle('n1')],connections:mode==='cascade'?[{from:port('n1','right'),to:port('n2','left')}]:[],ports:mode==='cascade'?{left:port('n1','left'),right:port('n2','right')}:{right:port('n1','right'),left:port('n1','left')}}});
 }
 return{contextId:'a',kind:'networkParameters',variantChanges:[],descriptors,config:{network:handle(root),frequencyHz:frequency,representation:'S',referenceOhm:{left:40,right:75}}};
}
const execute=createFrequencyExecutor();
test('resource propagation matrix: complete/reordered/cascade, coupled/delay/sampled, scope/reference/port order',async()=>{
 for(const kind of ['coupled','delay','sampled'])for(const mode of ['single','reordered','cascade']){
  const model=resource(kind);
  if(kind==='coupled'&&mode==='reordered'){await assert.rejects(()=>execute(task(model,mode)),(e:any)=>e.code==='COUPLED_PORT');continue;}
  const result=await execute(task(model,mode)),derived=networkResource(result.network);
  // Accepted full coupled assembly supports composed graphs; direct ordering stays strict.
  const analytic=kind==='coupled'||mode==='single'&&kind!=='sampled';
  assert.deepEqual(derived.ports,mode==='reordered'?['right','left']:['left','right']);
  assert.equal(derived.validity!.status,'conditional');
  assert.deepEqual(derived.analyses,analytic?model.analyses:['networkParameters']);
  assert.deepEqual(derived.validity!.domain!.bandHz,analytic?[0,5e6]:[1e6,1e6]);
  assert.equal(derived.data.format,analytic?model.data.format:'network-parameters');
  if(kind==='delay'&&analytic)assert.deepEqual((derived.data as any).referenceOhm,{left:25,right:100});
  if(!analytic)assert.deepEqual((derived.data as any).referenceOhm,mode==='reordered'?{right:75,left:40}:{left:40,right:75});
  const provenance=(derived.validity!.domain as any).provenance;
  assert.equal(provenance.length,mode==='cascade'?2:1);
  for(const p of provenance){assert.deepEqual(p.sourceValidity,model.validity);assert.deepEqual(p.allowedAnalyses,model.analyses);}
  // A factory/reuse round trip catches renamed reference dictionaries and order.
  const reused=task(derived);reused.descriptors[0].input.ports=Object.fromEntries(derived.ports.map((p,i)=>[['left','right'][i],p]));
  reused.config.referenceOhm=mode==='reordered'?{left:75,right:40}:{left:40,right:75};
  const again=await execute(reused);
  const actual=(again.solution as any).parameters,expected=(result.solution as any).parameters;
  assert.equal(actual.length,expected.length);
  for(let f=0;f<actual.length;f++)for(let i=0;i<2;i++)for(let j=0;j<2;j++)for(const part of ['real','imaginary'])assert.ok(Math.abs(actual[f][i][j][part]-expected[f][i][j][part])<1e-12);
  await assert.rejects(()=>execute(task(model,mode,6e6)),(e:any)=>e.code==='NETWORK_BAND');
 }
 for(const kind of ['coupled','delay','sampled']){
  const model=resource(kind);model.analyses=['si'];await assert.rejects(()=>execute(task(model)),(e:any)=>/RESOURCE|PORT|INPUT/.test(e.code));
  model.analyses=['networkParameters'];model.validity.status='unavailable';await assert.rejects(()=>execute(task(model)),(e:any)=>/VALIDITY|INPUT/.test(e.code));
 }
});
