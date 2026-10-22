import test from 'node:test';
import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdtempSync,writeFileSync,readFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {createRequire} from 'node:module';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
import {runAnalysisProblem,terminateAnalysisPool} from '../src/analyze/run-analysis.ts';
import {dcCoupon,couponSetup} from './helpers/pcb-analysis-dc-coupon.ts';
import {value} from '../src/analyze/analysis-dsl/data.ts';
import {dcMaterial,conductionProfile} from '../src/analyze/analysis-dsl/profile-adapter.ts';
import {stackup} from '../src/analyze/analysis-dsl/descriptions.ts';
import type {AnalysisDcRequest,AnalysisDcReport} from '../native/analyze/index.js';
import type {AnalysisDslOutput} from '../src/analyze/run-analysis-dsl.ts';
const addon=createRequire(import.meta.url)('../native/node-addon/index.cjs') as typeof import('../native/node-addon/index.cjs');
const completed=(r:AnalysisDslOutput)=>r.reports.flatMap(r=>r.calculations).filter((c):c is Record<string,never>=>!!c&&typeof c==='object'&&!Array.isArray(c)&&'phase'in c&&c.phase==='completed').map(c=>c.result) as unknown as {id:string;status:string;solution:AnalysisDcReport&{meshReference?:string}}[];
test('production board DC has real pad voltage, return drop, maps and retained failed checks',async()=>{
  const r=await runAnalysisDsl({board:dcCoupon(),code:`${couponSetup}
    const result=await base.loads([{port:load,currentA:1}]).run();
    a.check({name:'voltage',actual:result.voltage(load),target:1-16e-3/(58e6*2e-3*.035e-3),tolerance:1e-8,unit:'V'});
    a.check({name:'current',actual:result.current(load),target:1,tolerance:1e-12,unit:'A'});
    a.check({name:'intentional fail',actual:result.losses,max:0,unit:'W'});
    a.check({name:'after fail',actual:result.currentDensityMap.max(),min:0,unit:'A/mm2'});
    a.check({name:'undefined net current',actual:result.current(net('VCC')),min:0,unit:'A'});
    a.plot({title:'Actual voltage',data:result.voltageMap});`});
  assert.equal(r.status,'complete',JSON.stringify(r.error));
  assert.deepEqual(r.reports[0].checks.map(c=>c.status),['pass','pass','fail','pass','unknown']);
  const s=completed(r)[0].solution;assert.equal(s.status,'complete');assert.ok(s.copper_loss_w!>0);assert.ok(s.quality!.relative_residual<=1e-8);assert.equal(r.reports[0].plots.length,1);
});
test('production RHS sweep changes actual numbers, reuses factor and invalidates changed thickness',async()=>{
  const r=await runAnalysisDsl({board:dcCoupon(),code:`${couponSetup}
    await a.sweep({parameter:{name:'load',unit:'A',values:[1,2]},evaluate:async currentA=>{const d=await base.loads([{port:load,currentA}]).run();a.log({level:'info',message:'voltage',data:{v:d.voltage(load).value}});return d.id;}});
    await base.stackup(stackup({layers:[{kind:'copper',name:'TOP',thicknessMm:.07,material:'cu'}]})).loads([{port:load,currentA:2}]).run();`});
  assert.equal(r.status,'complete',JSON.stringify(r.error));const [a,b,c]=completed(r).map(r=>r.solution);
  assert.equal(a.reused_factorization,false);assert.equal(b.reused_factorization,true);assert.equal(c.reused_factorization,false);assert.equal(b.meshReference,completed(r)[0].id);
  assert.ok(Math.abs(b.copper_loss_w!/a.copper_loss_w!-4)<1e-7);assert.ok(Math.abs(c.copper_loss_w!/a.copper_loss_w!-2)<1e-7);
});

test('production resistance changes invalidate the matrix while a later voltage RHS reuses it',async()=>{
  const r=await runAnalysisDsl({board:dcCoupon(),code:`${couponSetup}
    await base.loads([{port:load,resistanceOhm:.1}]).run();
    await base.loads([{port:load,resistanceOhm:.2}]).run();
    await base.sources([{port:source,voltageV:2,model:'ideal'}]).loads([{port:load,resistanceOhm:.2}]).run();`});
  assert.equal(r.status,'complete',JSON.stringify(r.error));const [a,b,c]=completed(r).map(r=>r.solution);
  assert.deepEqual([a.reused_factorization,b.reused_factorization,c.reused_factorization],[false,false,true]);
  assert.ok(Math.abs(c.copper_loss_w!/b.copper_loss_w!-4)<1e-10);
});
test('DC uses existing equal/manual load resolver with explicit zero and independent return weights',async()=>{
  const b=dcCoupon();for(const [id,number,net,y]of[['up3','3','VCC',0],['un4','4','GND',5]]as const)b.pads.push({...b.pads.find(p=>p.id==='up')!,id,number,net,at:{x:9.5,y}});
  const code=`${couponSetup}await base.components([ic({designator:'U1',supplies:{rail:['1','3']},ground:['2','4']})]).loads([{designator:'U1',rail:net('VCC'),currentA:1,pinCurrentsA:{'1':0},returnPinCurrentsA:{'2':.25}}]).run();`;
  const r=await runAnalysisDsl({board:b,code});assert.equal(r.status,'complete',JSON.stringify(r.error));const s=completed(r)[0].solution as unknown as {assignments:{distributions?:{side:string;pins:{padId:string;currentA:number}[]}[]}[]};
  const assignment=s.assignments.find(a=>a.distributions)!;assert.deepEqual(assignment.distributions![0].pins.map(p=>p.currentA),[0,1]);assert.deepEqual(assignment.distributions![1].pins.map(p=>p.currentA),[.25,.75]);
  const invalid=await runAnalysisDsl({board:b,code:code.replace("'1':0","'1':2")});assert.equal(invalid.error?.code,'INVALID_CONFIGURATION');
});
test('DC defaults require explicit classified consumers and do not invent capacitor loads',async()=>{
  const b=dcCoupon();b.pads.push(...b.pads.filter(p=>p.component==='U1').map(p=>({...p,id:'second-'+p.id,component:'U2',at:{...p.at,x:5}})));
  const r=await runAnalysisDsl({board:b,code:`${couponSetup}await base.components([ic({designator:'U1'}),ic({designator:'U2'})]).loads({rail:net('VCC'),defaultCurrentA:.1,overrides:{U2:{currentA:.2}}}).run();`});
  assert.equal(r.status,'complete',JSON.stringify(r.error));assert.equal(completed(r)[0].solution.status,'complete');
  const bad=await runAnalysisDsl({board:dcCoupon(),code:`${couponSetup}await base.loads({rail:net('VCC'),defaultCurrentA:.1,overrides:{}}).run();`});assert.equal(bad.error?.code,'INVALID_CONFIGURATION');
});
test('unsupported DC power/dynamics/resources/circuit are explicit; other analyzers remain absent',async()=>{
  for(const config of [".loads([{port:load,powerW:1}])",".loads([{port:load,currentA:1,waveform:{kind:'step',from:1,to:2,atS:1,riseS:0}}])",".sources([{port:source,voltageV:1,model:{id:'custom',kind:'source'}}]).loads([])"]){
    const r=await runAnalysisDsl({board:dcCoupon(),code:`${couponSetup}await base${config}.run();`});assert.equal(r.error?.code,'UNSUPPORTED_DC_MODEL',JSON.stringify(r.error));
  }
  const r=await runAnalysisDsl({board:dcCoupon(),code:'await analysis({board}).geometry().rules([]).run();'});assert.equal(r.error?.code,'ANALYZER_NOT_IMPLEMENTED');
  const circuit=await runAnalysisDsl({code:`const a=analysis();const c=a.circuit({nodes:['p','gnd'],ports:[{id:'port',positive:'p',negative:'gnd'}],elements:[{id:'v',kind:'voltage-source',nodes:['p','gnd'],voltageV:1,model:'ideal'}]});await a.dc().circuit(c).run();`});assert.equal(circuit.error?.code,'UNSUPPORTED_DC_MODEL');
});
test('unknown/partial numeric data retains unit/validity instead of becoming zero or a full map maximum',()=>{
  const partial=value({kind:'map',unit:'V',values:[1,null,2],validity:{status:'valid'},diagnostics:[],data:{coverage:'partial'}});
  assert.equal(partial.max().value,2);assert.equal(partial.max().validity.status,'conditional');
  assert.equal(value({kind:'map',unit:'V',values:[1,2],validity:{status:'valid'},diagnostics:[],data:{independentReferences:[0,1]}}).max().validity.status,'unavailable');
  assert.equal(value({kind:'scalar',unit:'A',value:-2,validity:{status:'conditional',reason:'snapshot'},diagnostics:[]}).abs().value,2);
});
test('explicit barrel material/convention and material temperature law adapt without defaults',()=>{
  const s=stackup({layers:[{kind:'copper',name:'TOP',thicknessMm:.035,material:'cu'}],viaPlatingThicknessUm:25,platingMaterial:'cu',diameterConvention:'finished_hole'});
  const p=conductionProfile(s,{cu:{conductivitySM:58e6,state:'ref 20 C',referenceC:20,temperatureCoefficientPerK:.004}},70)!;
  assert.equal(p.barrel?.diameter_convention,'finished_hole');assert.equal(p.barrel?.plating_thickness_mm,.025);assert.equal(p.barrel?.material?.conductivity_s_m,58e6/1.2);
  assert.throws(()=>dcMaterial({conductivitySM:58e6,state:'partial law',referenceC:20},25));
  assert.throws(()=>dcMaterial({conductivitySM:58e6}));
});
test('standalone material validity survives the native/JS result boundary',async()=>{
  const r=await runAnalysisDsl({code:`const a=analysis();const c=a.conductor({lengthMm:10,widthMm:1,thicknessMm:.035,material:{conductivitySM:58e6,state:'constant test state',validity:{status:'conditional',reason:'provided material approximation'}}});const dc=await a.dc().conductor(c).currentA(1).run();a.check({name:'conditional',actual:dc.resistance,min:0,unit:'Ohm'});a.plot({title:'conditional field',data:dc.lossMap});`});
  assert.equal(r.status,'complete');assert.equal(r.reports[0].checks[0].status,'pass');assert.equal((r.reports[0].checks[0].actual as {validity:{status:string}}).validity.status,'conditional');assert.equal(completed(r)[0].solution.conditional,true);
});
test('internal DC problem uses the same addon report through the existing process runtime',async()=>{
  const request:AnalysisDcRequest={context_id:'direct-coupon',board_json:null,conductor:{length_mm:10,width_mm:1,thickness_mm:.035,conductivity_s_m:58e6,state:'constant'},terminals:[],elements:[{id:'source',kind:'current',positive:'p',negative:'n',value:1,role:'source'}]};
  try{const r=await runAnalysisProblem({kind:'dc',request});const session=new addon.AnalysisDcSession();const direct=session.solve(request);assert.deepEqual(r.standalone,direct.standalone);assert.deepEqual(r.quality,direct.quality);assert.doesNotThrow(()=>session.clear());await assert.rejects(runAnalysisProblem({kind:'dc',request:{...request,elements:[{...request.elements[0],value:NaN}]} }),{code:'INVALID_REQUEST'});}finally{await terminateAnalysisPool();}
});
test('abort and deadline interrupt real native DC, await worker exit, and recover',()=>{
  const directory=mkdtempSync(join(tmpdir(),'dc-native-cancel-')),log=join(directory,'pids.jsonl');
  const board=JSON.parse(readFileSync('tests/pcb-analyze/NanoScope/NanoScope.json','utf8'));
  const code=readFileSync('tests/pcb-analyze/NanoScope/dc.js','utf8');
  const source=`import assert from 'node:assert/strict';import{existsSync,readFileSync}from'node:fs';
import{runAnalysisDsl}from'./dist/index.js';
const log=process.env.ANALYSIS_DC_PID_LOG,records=()=>existsSync(log)?readFileSync(log,'utf8').trim().split(/\\r?\\n/).filter(Boolean).map(JSON.parse):[];
const alive=pid=>{try{process.kill(pid,0);return true}catch{return false}};
const board=JSON.parse(readFileSync('tests/pcb-analyze/NanoScope/NanoScope.json','utf8'));
for(const mode of ['abort','timeout']){
 const count=records().length,controller=new AbortController();
 const run=runAnalysisDsl({board,code:${JSON.stringify(code)}},{signal:controller.signal,timeoutMs:mode==='timeout'?2500:20000,maxOutputBytes:64*1024*1024});
 const deadline=Date.now()+15000;let started;
 while(!(started=records().slice(count).find(r=>r.phase==='solve-start'))){assert.ok(Date.now()<deadline,'Native DC never started');await new Promise(r=>setTimeout(r,20))}
 assert.equal(started.board,true);await new Promise(r=>setTimeout(r,80));if(mode==='abort')controller.abort();
 const result=await run;assert.equal(result.error?.code,mode==='abort'?'ABORTED':'TIMEOUT');
 assert.equal(alive(started.pid),false,'native worker still alive');assert.ok(!records().slice(count).some(r=>r.pid===started.pid&&r.phase==='solve-end'),'DC finished before cancellation');
 const recovery=await runAnalysisDsl({code:'const a=analysis();const c=a.conductor({lengthMm:10,widthMm:1,thicknessMm:.035,material:{conductivitySM:58e6,state:"constant"}});const dc=await a.dc().conductor(c).currentA(1).run();a.check({name:"recovered",actual:dc.resistance,min:0,unit:"Ohm"});'});
 assert.equal(recovery.status,'complete');assert.equal(recovery.reports[0].checks[0].status,'pass');
 console.log(JSON.stringify({mode,killedPid:started.pid,error:result.error.code,recovery:recovery.status,partialReports:result.reports.length}));
}
for(const record of records())assert.equal(alive(record.pid),false);
`;
  writeFileSync(join(directory,'source.mjs'),source);
  const child=spawnSync(process.execPath,['--input-type=module','-e',source],{cwd:resolve('.'),windowsHide:true,encoding:'utf8',timeout:45000,maxBuffer:2*1024*1024,env:{...process.env,NODE_OPTIONS:`--require="${resolve('tests/fixtures/analysis-dc-pid.cjs').replaceAll('\\','/')}"`,ANALYSIS_DC_PID_LOG:log}});
  writeFileSync('debugging/analysis-dsl/dc-cancel.log',(child.stdout??'')+(child.stderr??''));
  assert.ifError(child.error);assert.equal(child.status,0,child.stdout+child.stderr);assert.doesNotMatch(child.stderr,/CubeCL|CUDA|Vulkan/i);
});
