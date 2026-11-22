import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdtempSync,mkdirSync}from'node:fs';
import {tmpdir}from'node:os';
import {join,resolve}from'node:path';
import {spawnSync}from'node:child_process';
import {runAnalysisDsl,type AnalysisDslOutput} from '../src/analyze/run-analysis-dsl.ts';
import {value} from '../src/analyze/analysis-dsl/data.ts';
import type{AnalysisElectricalReport}from'../native/analyze/index.js';
import{auditRlcReport}from'./helpers/pcb-analysis-rlc-audit.ts';
import{createElectricalExecutor}from'../src/analyze/analysis-dsl/electrical-executor.ts';

test('frequency admission precedes Array.from, including huge integers and multiple ports',async()=>{
 const execute=createElectricalExecutor();
 const original=Array.from;let materializations=0;
 Array.from=((...args:unknown[])=>{materializations++;throw Error('Frequency axis materialized before admission');})as typeof Array.from;
 try{
  for(const [points,ports]of [[1e9,1],[Number.MAX_SAFE_INTEGER+1,1],[50001,2],[100001,1]]){
   await assert.rejects(execute({contextId:'early',kind:'pi',config:{circuit:{id:'c'},domain:'frequency',frequency:{startHz:1,stopHz:10,points,scale:'linear'}},variantChanges:[],descriptors:[{id:'c',input:{nodes:['gnd','p'],ports:Array(ports).fill({id:'p',positive:'p',negative:'gnd'}),elements:[{id:'r',kind:'resistor',nodes:['p','gnd'],resistanceOhm:1}]}}]}as any),e=>!!e&&typeof e==='object'&&'code'in e&&e.code==='NETWORK_BUDGET');
  }
 }finally{Array.from=original;}
 assert.equal(materializations,0);
 for(const frequency of ["frequency({startHz:1,stopHz:3,points:3,scale:'linear'})","frequency({startHz:1,stopHz:100,points:3,scale:'log'})","frequencyHz(7)"]){
  const good=await runAnalysisDsl({code:`${setup}await a.pi().circuit(c).domain('frequency').${frequency}.run();`});assert.equal(good.status,'complete',JSON.stringify(good.error));assert.deepEqual(networkResults(good)[0].solution.axis,frequency.includes('linear')?[1,2,3]:frequency.includes('log')?[1,10,100]:[7]);
 }
 const rejected=await runAnalysisDsl({code:`${setup}await a.pi().circuit(c).domain('frequency').frequency({startHz:1,stopHz:10,points:1000000000,scale:'linear'}).run();`});assert.equal(rejected.error?.code,'NETWORK_BUDGET');
 const recovery=await runAnalysisDsl({code:`${setup}await a.dc().circuit(c).run();`});assert.equal(recovery.status,'complete');
});
test('production periodic right events, repeat end and continuation boundary have independent 0/1 expectations',async()=>{
 for(const waveform of [
  {kind:'pulse',low:0,high:1,delayS:0,riseS:0,highS:.05,fallS:0,periodS:.1,count:10},
  {kind:'pwl',points:[[0,1],[.05,0],[.1,0]],interpolation:'hold',repeat:{periodS:.1,count:10}},
 ]){
  const code=`const a=analysis();const c=a.circuit({nodes:['gnd','p'],reference:'gnd',ports:[{id:'p',positive:'p',negative:'gnd'}],elements:[{id:'r',kind:'resistor',nodes:['p','gnd'],resistanceOhm:1},{id:'i',kind:'current-source',nodes:['gnd','p'],currentA:0,waveform:${JSON.stringify(waveform)}}]});const first=await a.pi().circuit(c).domain('time').initial('steady').time({stopS:.5,sampleStepS:.01}).run();const next=await a.pi().circuit(c).domain('time').initial(first.finalState).time({stopS:1.01,sampleStepS:.01}).run();for(const [d,t,target]of [[first,.49,0],[first,.5,1],[next,.5,1],[next,.51,1],[next,.55,0],[next,1,0],[next,1.01,0]])a.check({name:'independent boundary '+t,actual:d.voltage(c.port('p')).at({timeS:t}),target,tolerance:1e-10,unit:'V'});`;
  const r=await runAnalysisDsl({code});assert.equal(r.status,'complete',JSON.stringify(r.error));assert.deepEqual(r.reports[0].checks.map(c=>c.status),Array(7).fill('pass'));const [first,next]=networkResults(r).map(c=>c.solution);assert.equal((first.finalState as any).timeS,.5);assert.equal(next.axis![0],.5);assert.equal((next.finalState as any).timeS,1.01);
 }
});
test('production late continuation emits about a thousand samples rather than planning from zero',async()=>{
 const r=await runAnalysisDsl({code:`const a=analysis();const c=a.circuit({nodes:['gnd','p'],reference:'gnd',ports:[{id:'p',positive:'p',negative:'gnd'}],elements:[{id:'r',kind:'resistor',nodes:['p','gnd'],resistanceOhm:1},{id:'i',kind:'current-source',nodes:['gnd','p'],currentA:1}]});const first=await a.pi().circuit(c).domain('time').initial('steady').time({stopS:1,sampleStepS:1}).run();const next=await a.pi().circuit(c).domain('time').initial(first.finalState).time({stopS:1.001,sampleStepS:1e-6}).run();a.check({name:'constant',actual:next.voltage(c.port('p')).min(),target:1,tolerance:1e-12,unit:'V'});`});assert.equal(r.status,'complete',JSON.stringify(r.error));assert.equal(r.reports[0].checks[0].status,'pass');const s=networkResults(r)[1].solution;assert.equal(s.axis!.length,1001);assert.equal(s.axis![0],1);assert.equal(s.axis!.at(-1),1.001);assert.ok(s.axis!.every((t,i,a)=>!i||t>a[i-1]));assert.equal((s.finalState as any).timeS,1.001);
});
export const networkResults=(r:AnalysisDslOutput)=>r.reports.flatMap(r=>r.calculations).filter((c):c is Record<string,any>=>!!c&&typeof c==='object'&&!Array.isArray(c)&&'phase'in c&&c.phase==='completed').map(c=>c.result as {id:string;solution:AnalysisElectricalReport});
const setup=`const a=analysis();const c=a.circuit({nodes:['gnd','src','out'],ports:[{id:'out',positive:'out',negative:'gnd'}],elements:[{id:'v',kind:'voltage-source',nodes:['src','gnd'],voltageV:2,model:'ideal'},{id:'r',kind:'resistor',nodes:['src','out'],resistanceOhm:10},{id:'load',kind:'resistor',nodes:['out','gnd'],resistanceOhm:10}]});`;
test('production sparse DC signs, late passive ports, unknown current and immutable previous results',async()=>{
 const r=await runAnalysisDsl({code:`${setup}const d=await a.dc().circuit(c).run();const p=c.port('out');a.check({name:'V',actual:d.voltage(p),target:1,tolerance:1e-12,unit:'V'});a.check({name:'oriented source',actual:d.current('v'),target:-.1,tolerance:1e-12,unit:'A'});a.check({name:'dissipation',actual:d.losses,target:.2,tolerance:1e-12,unit:'W'});a.check({name:'port current unavailable',actual:d.current(p),min:0,unit:'A'});a.check({name:'strict',actual:d.losses,max:0,unit:'W'});a.check({name:'continued after fail',actual:d.current('r'),min:0,unit:'A'});a.plot({title:'no geometry',data:d.lossMap});await a.dc().circuit(c).run();a.check({name:'frozen',actual:d.voltage(p),target:1,tolerance:1e-12,unit:'V'});`});
 assert.equal(r.status,'complete',JSON.stringify(r.error));assert.deepEqual(r.reports[0].checks.map(c=>c.status),['pass','pass','pass','unknown','fail','pass','pass']);assert.equal((r.reports[0].plots[0]as any).data.validity.status,'unavailable');assert.equal(networkResults(r)[1].solution.counters!.reusedFactors,1);
});
test('local p/n model mapping exactly matches R C L elements; incomplete/unsupported mappings reject',async()=>{
 for(const kind of ['resistor','capacitor','inductor']){
  const model=kind==='resistor'?{kind,resistanceOhm:10}:kind==='capacitor'?{kind,capacitanceF:1e-6,esrOhm:.1,eslH:1e-9}:{kind,inductanceH:1e-3,seriesResistanceOhm:.1};
  const code=(mapped:boolean)=>`const a=analysis();const m=a.model(${JSON.stringify(model)});const c=a.circuit({nodes:['gnd','p'],ports:[{id:'p',positive:'p',negative:'gnd'}],elements:[${JSON.stringify(mapped?{id:'x',kind:'model',model,pins:{p:'p',n:'gnd'}}:{...model,id:'x',nodes:['p','gnd']})}]});const d=await a.pi().circuit(c).domain('frequency').frequencyHz(1000).run();a.plot({title:'Z',data:d.impedance(c.port('p'))});`;
  const [direct,mapped]=await Promise.all([runAnalysisDsl({code:code(false)}),runAnalysisDsl({code:code(true)})]);assert.equal(mapped.status,'complete',JSON.stringify(mapped.error));assert.deepEqual(networkResults(direct)[0].solution.impedances,networkResults(mapped)[0].solution.impedances);
  const bad=await runAnalysisDsl({code:code(true).replace('"n":"gnd"','"negative":"gnd"')});assert.equal(bad.error?.code,'UNSUPPORTED_NETWORK_MODEL');
 }
});
test('complex DataValue parts, phase, undefined zero phase, series reducers and exact saved samples',()=>{
 const z=value({kind:'series',unit:'Ohm',validity:{status:'valid'},diagnostics:[],values:[{real:3,imaginary:4},{real:-1,imaginary:0},{real:0,imaginary:0}],axis:{name:'frequencyHz',unit:'Hz',values:[1,2,3]}});
 assert.deepEqual(z.real().values,[3,-1,0]);assert.deepEqual(z.imaginary().values,[4,0,0]);assert.equal(z.magnitude().max().value,5);assert.equal(z.min().validity.status,'unavailable');assert.equal(z.phaseDeg().unit,'deg');assert.equal(z.phaseDeg().values![2],null);assert.equal(z.phaseDeg().values![1],180);assert.equal(z.at({frequencyHz:2}).real().value,-1);assert.equal(z.at({frequencyHz:1.5}).validity.status,'unavailable');
});
test('host PCB/unused aggregate ports do not connect to circuit; DC constant differs from absolute time initial waveform',async()=>{
 const board=JSON.parse(readFileSync('tests/fixtures/analysis-dc-coupon.json','utf8'));
 const code=setup.replace('const a=analysis();',`const a=analysis({board});const unused=a.port({positive:[pad('J1','1'),pad('U1','1')],negative:pad('J1','2')});`).replace("voltageV:2,model:'ideal'","voltageV:2,model:'ideal',waveform:{kind:'step',from:4,to:6,atS:.001,riseS:.0001}");
 const r=await runAnalysisDsl({board,code:`${code}const dc=await a.dc().circuit(c).run();const tr=await a.pi().circuit(c).domain('time').initial('steady').time({stopS:.002,sampleStepS:.00001}).run();const p=c.port('out');a.check({name:'DC constant',actual:dc.voltage(p),target:1,tolerance:1e-12,unit:'V'});a.check({name:'absolute waveform initial',actual:tr.voltage(p).at({timeS:0}),target:2,tolerance:1e-12,unit:'V'});a.check({name:'unbound board port',actual:dc.voltage(unused),min:0,unit:'V'});`});assert.equal(r.status,'complete',JSON.stringify(r.error));assert.deepEqual(r.reports[0].checks.map(c=>c.status),['pass','pass','unknown']);
});
test('production ideal voltage step through R into C is regular; direct ideal capacitor jump is diagnosed',async()=>{
 const code=(impulse:boolean)=>`const a=analysis();const c=a.circuit({nodes:['gnd','src','out'],ports:[{id:'out',positive:'out',negative:'gnd'}],elements:[{id:'v',kind:'voltage-source',nodes:['src','gnd'],voltageV:0,model:'ideal',waveform:{kind:'step',from:0,to:1,atS:.0001,riseS:0}},{id:'r',kind:'resistor',nodes:['src','out'],resistanceOhm:1000},{id:'c',kind:'capacitor',nodes:[${impulse?"'src'":"'out'"},'gnd'],capacitanceF:1e-6,ideal:true}]});const d=await a.pi().circuit(c).domain('time').initial('steady').time({stopS:.005,sampleStepS:.00001}).run();a.check({name:'analytic RC',actual:d.voltage(c.port('out')).at({timeS:.005}),target:1-Math.exp(-4.9),tolerance:.005,unit:'V'});`;
 const good=await runAnalysisDsl({code:code(false)});assert.equal(good.status,'complete',JSON.stringify(good.error));assert.equal(good.reports[0].checks[0].status,'pass');const bad=await runAnalysisDsl({code:code(true)});assert.equal(bad.error?.code,'NETWORK_IMPULSE_UNSUPPORTED');
});
test('task example DC/Z/transient/continuation runs through production; phases and values available',async()=>{
 const code=readFileSync('tests/fixtures/analysis-electrical.js','utf8');const r=await runAnalysisDsl({code},{timeoutMs:120000,maxOutputBytes:16*1024*1024});assert.equal(r.status,'complete',JSON.stringify(r.error));assert.equal(r.reports[0].summary.unknown,0);assert.equal(networkResults(r).length,4);const [dc,ac,tr,continued]=networkResults(r).map(r=>r.solution);assert.ok(Math.abs((dc.nodeVoltages!.load as number)-3.294)<1e-10);assert.equal(ac.axis!.length,201);assert.equal(continued.axis![0],tr.finalState&&(tr.finalState as any).timeS);assert.ok(tr.quality!.relativeEnergyError<.005);assert.ok(continued.quality!.relativeEnergyError<.005);
});
test('integer-valued 20 V USB state survives the Node JSON round trip and continues',async()=>{
 const fixture='tests/pcb-analyze/USB_PD_Trigger_5A/';const r=await runAnalysisDsl({board:JSON.parse(readFileSync(fixture+'USB_PD_Trigger_5A.json','utf8')),code:readFileSync(fixture+'rlc.js','utf8')},{maxOutputBytes:32*1024*1024});assert.equal(r.status,'complete',JSON.stringify(r.error));assert.equal(auditRlcReport(r,JSON.parse(readFileSync(fixture+'rlc.expectations.json','utf8'))).cases,8);
});
test('coarse output preserves fast RC energy; unresolved scale/front refuses instead of false complete',async()=>{
 const code=(r:number,rise=0)=>`const a=analysis();const c=a.circuit({nodes:['gnd','src','out'],ports:[{id:'out',positive:'out',negative:'gnd'}],elements:[{id:'v',kind:'voltage-source',nodes:['src','gnd'],voltageV:0,model:'ideal',waveform:{kind:'step',from:0,to:1,atS:.01,riseS:${rise}}},{id:'r',kind:'resistor',nodes:['src','out'],resistanceOhm:${r}},{id:'c',kind:'capacitor',nodes:['out','gnd'],capacitanceF:1e-9,ideal:true}]});await a.pi().circuit(c).domain('time').initial('steady').time({stopS:.1,sampleStepS:.001}).run();`;
 const good=await runAnalysisDsl({code:code(1)});assert.equal(good.status,'complete',JSON.stringify(good.error));const s=networkResults(good)[0].solution;assert.ok(s.quality!.relativeEnergyError<.005);assert.ok(s.quality!.minInternalStepS<1e-10);assert.ok(Math.abs(s.quality!.dissipationJ-.5e-9)<.005e-9);
 const limited=await runAnalysisDsl({code:code(.001)});assert.equal(limited.error?.code,'NETWORK_STEP_UNDERFLOW');const front=await runAnalysisDsl({code:code(1,1e-30)});assert.equal(front.error?.code,'NETWORK_WAVEFORM_RESOLUTION');
});
test('multiple ports use separate RHS; matrix invalidates R/C/L/frequency and RHS stays reusable',async()=>{
 const r=await runAnalysisDsl({code:`const a=analysis();const build=(r,l,c,v=1)=>a.circuit({nodes:['gnd','p','q'],ports:[{id:'p',positive:'p',negative:'gnd'},{id:'q',positive:'q',negative:'gnd'}],elements:[{id:'r',kind:'resistor',nodes:['p','gnd'],resistanceOhm:r},{id:'l',kind:'inductor',nodes:['p','q'],inductanceH:l,seriesResistanceOhm:.1},{id:'c',kind:'capacitor',nodes:['q','gnd'],capacitanceF:c,ideal:true},{id:'i',kind:'current-source',nodes:['gnd','p'],currentA:v}]});for(const [r,l,c,f,v]of [[1,.001,.000001,1000,1],[1,.001,.000001,1000,2],[2,.001,.000001,1000,2],[2,.002,.000001,1000,2],[2,.002,.000002,1000,2],[2,.002,.000002,2000,2]])await a.pi().circuit(build(r,l,c,v)).domain('frequency').frequencyHz(f).run();`});
 assert.equal(r.status,'complete',JSON.stringify(r.error));const s=networkResults(r).map(r=>r.solution);assert.deepEqual(s.map(r=>r.counters!.numericalFactorizations),[1,0,1,1,1,1]);assert.deepEqual(s[0].impedances,s[1].impedances);assert.equal(s[0].counters!.solves,2);assert.equal(s[1].counters!.compilations,0);
});
test('negative networks, state, budgets, unavailable AC voltage, units and next successful run',async()=>{
 for(const code of [
  `${setup}await a.dc().circuit(c).numerics({relativeTolerance:1e-4}).run();`,
  `${setup}await a.pi().circuit(c).domain('frequency').frequencyHz(100).budget({maxMemoryMiB:.0001}).run();`,
  `${setup}await a.pi().circuit(c).domain('time').initial('steady').time({stopS:1,sampleStepS:1e-12}).run();`,
 ]){const r=await runAnalysisDsl({code});assert.equal(r.status,'failed');assert.match(r.error!.code,/NETWORK_/);}
 const r=await runAnalysisDsl({code:`${setup}const d=await a.pi().circuit(c).domain('frequency').frequencyHz(100).run();a.check({name:'uncomputed V',actual:d.voltage(c.port('out')),min:0,unit:'V'});a.check({name:'M1',actual:d.impedance(c.port('out')).magnitude().at({frequencyHz:100}),min:0,unit:'V'});a.plot({title:'continued',data:d.impedance(c.port('out')).magnitude()});`});assert.equal(r.status,'complete');assert.deepEqual(r.reports[0].checks.map(c=>c.status),['unknown','unknown']);
 const success=await runAnalysisDsl({code:`${setup}await a.dc().circuit(c).run();`});assert.equal(success.status,'complete');
});
test('abort/timeout kill synchronous native transient and runner recovers without orphan process',()=>{
 const directory=mkdtempSync(join(tmpdir(),'rlc-cancel-')),path=join(directory,'test.mjs'),log=join(directory,'pids.jsonl');
 const code=`import assert from 'node:assert/strict';import{readFileSync,existsSync}from'node:fs';import{runAnalysisDsl}from ${JSON.stringify(new URL('../dist/index.js',import.meta.url).href)};
 const records=()=>existsSync(process.env.ANALYSIS_ELECTRICAL_PID_LOG)?readFileSync(process.env.ANALYSIS_ELECTRICAL_PID_LOG,'utf8').trim().split(/\\r?\\n/).filter(Boolean).map(JSON.parse):[];
 const alive=pid=>{try{process.kill(pid,0);return true}catch{return false}};
 const script=${JSON.stringify(readFileSync('tests/fixtures/analysis-electrical.js','utf8').replace('500e-6','500').replace('0.5e-6','0.1').replace('{ kind: "step", from: 0.1, to: 1, atS: 100e-6, riseS: 1e-6 }','{ kind: "sine", offset: 0.1, amplitude: 1, periodS: 1e-6, phaseDeg: 0 }'))};
 for(const mode of ['abort','timeout']){const count=records().length,c=new AbortController();const pending=runAnalysisDsl({code:script},{signal:c.signal,timeoutMs:mode==='timeout'?1800:15000,maxOutputBytes:16*1024*1024});let started;const deadline=Date.now()+12000;while(!(started=records().slice(count).find(r=>r.phase==='solve-start'&&r.mode==='time'))){assert.ok(Date.now()<deadline);await new Promise(r=>setTimeout(r,10));}if(mode==='abort'){await new Promise(r=>setTimeout(r,50));c.abort();}const r=await pending;assert.equal(r.error?.code,mode==='abort'?'ABORTED':'TIMEOUT',JSON.stringify(r.error));assert.equal(alive(started.pid),false);assert.ok(!records().slice(count).some(r=>r.pid===started.pid&&r.phase==='solve-end'&&r.mode==='time'));const good=await runAnalysisDsl({code:${JSON.stringify(setup+'await a.dc().circuit(c).run();')}});assert.equal(good.status,'complete');console.log(JSON.stringify({mode,pid:started.pid,alive:alive(started.pid),recovery:good.status}));}for(const r of records())assert.equal(alive(r.pid),false); `;
 writeFileSync(path,code);const r=spawnSync(process.execPath,[path],{encoding:'utf8',timeout:45000,windowsHide:true,env:{...process.env,NODE_OPTIONS:`--require="${resolve('tests/fixtures/analysis-electrical-pid.cjs').replaceAll('\\','/')}"`,ANALYSIS_ELECTRICAL_PID_LOG:log}});assert.equal(r.status,0,r.stdout+r.stderr);assert.doesNotMatch(r.stderr,/CubeCL|CUDA|Vulkan/i);mkdirSync('debugging/analysis-dsl',{recursive:true});writeFileSync('debugging/analysis-dsl/electrical-cancel.log',r.stdout+r.stderr);
});
