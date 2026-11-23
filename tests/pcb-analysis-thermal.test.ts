import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync,mkdtempSync,mkdirSync,writeFileSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
import {createAnalysisApi} from '../src/analyze/analysis-dsl/context.ts';
import {thermalLossTransfer} from '../src/analyze/analysis-dsl/thermal-loss-transfer.ts';
import {dcCoupon,couponSetup} from './helpers/pcb-analysis-dc-coupon.ts';
import {thermalResults} from './helpers/pcb-analysis-thermal-audit.ts';
import {resultFor} from '../src/analyze/analysis-dsl/executor.ts';
import {ReportStore} from '../src/analyze/analysis-dsl/data.ts';

function writeThermalEvidence(filename:string,content:string) {
 const directory='debugging/thermal-verification';
 mkdirSync(directory,{recursive:true});
 writeFileSync(join(directory,filename),content);
}

test('thermal refinement report publication obeys the same deadline before mutation',()=>{
 const updates:unknown[]=[];const store=new ReportStore('deadline',undefined,{maxEntries:10,maxOutputBytes:1e6},u=>updates.push(u));
 assert.throws(()=>store.add('calculations',{computed:1},performance.now()-1),/deadline/);
 const record={get computed(){const end=performance.now()+150;while(performance.now()<end){}return 1;}};
 assert.throws(()=>store.add('calculations',record,performance.now()+100),/deadline/);
 assert.equal(store.report.calculations.length,0,'Overdue serialization cannot publish a complete calculation');assert.equal(updates.length,0);
});

test('thermal refinement total deadline includes preparation progress and executor time',async()=>{
 for(const slowPreparation of [true,false]){
  let calls=0;
  const api=createAnalysisApi({}, {maxEntries:100,maxOutputBytes:1e6},update=>{
   if(slowPreparation&&update.kind==='entry'&&update.section==='calculations'){
    const end=performance.now()+150;while(performance.now()<end){}
   }
  },async task=>{
   calls++;assert.ok(task.deadlineAtMs!==undefined&&task.deadlineAtMs>performance.now());
   await new Promise(resolve=>setTimeout(resolve,150));
   return{id:'expired',status:'complete',method:'Injected deadline test',diagnostics:[],assumptions:[],metrics:{}};
  });
  const a=api.globals.analysis({board:dcCoupon()});
  const calculation=a.thermal().heat([]).cooling({boardSurfaces:a.model({kind:'cooling',resistanceKPerW:10})})
   .environment({name:'deadline test',ambientC:25}).domain('steady')
   .mesh({maxEdgeMm:1,refinement:{maxPasses:1,relativeTolerance:.02,validationMaxEdgeMm:.5}}).budget({maxSeconds:.1});
  await assert.rejects(calculation.run(),error=>error instanceof Error&&'code'in error&&error.code==='THERMAL_DEADLINE');
  assert.equal(calls,slowPreparation?0:1);
  assert.equal(api.snapshot().flatMap(r=>r.calculations).filter((c:any)=>c.phase==='completed').length,0);
 }
});

test('thermal lazy observations retain complete serialized arrays and preserve partial/unavailable validity',()=>{
 const axis={name:'timeS',unit:'s',values:[0,1]},validity={status:'conditional',reason:'Explicit partial computed coverage'};
 const payload:any={id:'lazy',status:'partial',method:'contract only',diagnostics:[],assumptions:[],metrics:{energyBalance:{kind:'series',values:[0,0],axis,unit:'J',validity,diagnostics:[]}},solution:{status:'partial',axis:[0,1],temperaturesC:{'pcb:TOP:0':[25,30],missing:[]},heatFlowW:{'fem:0:1':[0,1]}}};
 const result=resultFor({kind:'thermal',contextId:'ctx',config:{domain:'time'},descriptors:[],variantChanges:[]}as any,payload,()=>{},()=>{throw Error('unused');})as any;
 assert.deepEqual(result.temperature('pcb:TOP:0').values,[25,30]);assert.equal(result.temperature('pcb:TOP:0').unit,'degC');assert.equal(result.temperature('pcb:TOP:0').validity.status,'conditional');assert.equal(result.temperature('pcb:TOP:0').contextId,'ctx');
 assert.deepEqual(result.heatFlow('fem:0:1').values,[0,1]);assert.equal(result.heatFlow('fem:0:1').unit,'W');assert.equal(result.temperature('missing').validity.status,'unavailable');assert.equal(result.heatFlow('foreign').validity.status,'unavailable');
 const saved=JSON.parse(JSON.stringify(result));assert.deepEqual(saved.solution.temperaturesC,payload.solution.temperaturesC);assert.deepEqual(saved.solution.heatFlowW,payload.solution.heatFlowW);assert.equal(saved.status,'partial');assert.ok(!('metrics'in saved));
});

test('thermal DSL additions preserve authored background metadata and reject malformed cooling',()=>{
 const {globals:g}=createAnalysisApi({}, {maxEntries:100,maxOutputBytes:1e6},()=>{});const a=g.analysis();
 const s=g.stackup({layers:[{kind:'copper',name:'TOP',thicknessMm:.035,material:'copper',backgroundMaterial:'resin'},{kind:'dielectric',name:'core',thicknessMm:1,material:'resin'},{kind:'copper',name:'BOTTOM',thicknessMm:.035,material:'copper',backgroundMaterial:'resin'}]});
 assert.equal(s.input.layers[0].backgroundMaterial,'resin');assert.ok(!('backgroundMaterial'in s.routing.layers[0]));
 assert.throws(()=>a.model({kind:'cooling',table:[{deltaTemperatureK:0,heatFlowW:0},{deltaTemperatureK:10,heatFlowW:-1}]}));
 assert.throws(()=>a.model({kind:'cooling',table:[{deltaTemperatureK:1,heatFlowW:1},{deltaTemperatureK:10,heatFlowW:2}]}));
 assert.ok(a.model({kind:'cooling',resistanceKPerW:10,scale:{kind:'step',from:0,to:1,atS:10,riseS:0}}));
 assert.ok(a.thermal().mesh({geometryDeviationMm:.005,maxNodes:50000}));for(const geometryDeviationMm of [0,1e-7,.011])assert.throws(()=>a.thermal().mesh({geometryDeviationMm}));
 assert.ok(a.thermal().mesh({maxNodes:60000}));assert.ok(a.thermal().mesh({maxNodes:100000}));assert.throws(()=>a.thermal().mesh({maxNodes:100001}));
 assert.ok(a.thermal().mesh({maxEdgeMm:1,maxNodes:140000,refinement:{maxPasses:5,relativeTolerance:.02,validationMaxEdgeMm:.5}}));
 assert.throws(()=>a.thermal().mesh({recipe:{parentMeshHash:'internal',edges:[[0,1]]}}as any));
 assert.throws(()=>a.thermal().mesh({refinement:{maxPasses:1,relativeTolerance:.02,validationMaxEdgeMm:.5,parentMeshHash:'internal'}}as any));
 for(const refinement of [{maxPasses:0,relativeTolerance:.02,validationMaxEdgeMm:.5},{maxPasses:7,relativeTolerance:.02,validationMaxEdgeMm:.5},{maxPasses:5,relativeTolerance:.03,validationMaxEdgeMm:.5},{maxPasses:5,relativeTolerance:.02,validationMaxEdgeMm:1}])assert.throws(()=>a.thermal().mesh({maxEdgeMm:1,refinement}));
});
test('loss transfer resolves authoritative same-context geometry and rejects source/array/validity substitutions',()=>{
 const source:any={id:'dc1',boardIdentity:'board1',status:'complete',conditional:true,domains:[{domain_id:'d',layer:'TOP',loss_w:[1,0]}],excluded_domains:[],copper_loss_w:1,barrel_loss_w:0,mesh:{domains:[{id:'d',layer:'TOP',nodes:[[0,0],[1,0],[1,1],[0,1]],triangles:[[0,1,2],[0,2,3]]}]}};
 const task:any={contextId:'context',boardIdentity:'board1',config:{losses:{contextId:'context',kind:'map',unit:'W',validity:{status:'conditional'},values:[1,0],data:{solutionId:'dc1',field:'loss_w',association:'triangle',slices:[{domainId:'d',start:0,count:2}]}}}};
 const resolve=()=>source;const transfer=thermalLossTransfer(task,resolve)!;assert.equal(transfer.conditional,true);assert.equal(transfer.heat.length,1);assert.equal((transfer.provenance as any).zeroPowerEntriesSkipped,1);assert.equal((transfer.provenance as any).totalPowerW,1);
 for(const mutate of [(t:any)=>t.config.losses.values.pop(),(t:any)=>t.config.losses.values.push(0),(t:any)=>t.config.losses.values[1]=null,(t:any)=>t.config.losses.values[0]=0,(t:any)=>t.config.losses.unit='J',(t:any)=>t.boardIdentity='foreign',(t:any)=>t.config.losses.contextId='foreign',(t:any)=>t.config.losses.validity.status='valid',(t:any)=>t.config.losses.data.solutionId='foreign']){
  const t=structuredClone(task);mutate(t);assert.throws(()=>thermalLossTransfer(t,(context,id)=>id==='dc1'?source:undefined),/source|Loss|loss|conditional|same-context/i);
 }
 const nonzeroBarrel=structuredClone(source);nonzeroBarrel.barrel_loss_w=.1;assert.throws(()=>thermalLossTransfer(task,()=>nonzeroBarrel),/barrel/);
 const omitted=structuredClone(source);omitted.excluded_domains=['missing'];assert.throws(()=>thermalLossTransfer(task,()=>omitted),/uncomputed/);
 const reused=structuredClone(source);delete reused.mesh;reused.meshReference='first';assert.equal(thermalLossTransfer(task,(_,id)=>id==='dc1'?reused:id==='first'?source:undefined)!.heat.length,1);
 const withBarrel=structuredClone(source);withBarrel.barrel_loss_w=.1;withBarrel.barrels=[{hole_id:'hole-via',from_layer:'TOP',to_layer:'BOTTOM',loss_w:.1}];withBarrel.mesh.barrels=[{hole_id:'hole-via',ready:true,source:{kind:'via',source_id:'hotvia',path:'$.copper.fixed.vias[0]'},segments:[{from_layer:'TOP',to_layer:'BOTTOM'}]}];const barrel=thermalLossTransfer(task,()=>withBarrel)!;assert.equal(barrel.barrelHeat.length,1);assert.equal((barrel.provenance as any).totalPowerW,1.1);assert.equal((barrel.barrelHeat[0] as any).barrel,'via:hotvia');
});
test('production thermal T1 example, finite arrays, unavailable geometry and expected failed check',async()=>{
 const code=readFileSync('tests/fixtures/analysis-thermal.js','utf8'),started=performance.now();const result=await runAnalysisDsl({code},{maxOutputBytes:8*1024*1024,timeoutMs:120000});assert.equal(result.status,'complete',JSON.stringify(result.error));assert.deepEqual(result.reports[0].checks.map(c=>c.status),['pass','pass','fail','pass']);
 const [steady,time,continuation]=thermalResults(result);assert.ok(Math.abs(steady.solution.temperaturesC.junction[0]-55)<1e-8);assert.equal(time.solution.axis.length,201);assert.equal(continuation.solution.axis[0],100);assert.equal(time.solution.quality.normalizedEnergyBalance<1e-6,true);assert.equal((result.reports[0].plots.at(-1)as any).data.validity.status,'unavailable');
 const {nativeFilename}=createRequire(import.meta.url)('../native/node-addon/platform.cjs');const hash=(b:string|Buffer)=>createHash('sha256').update(b).digest('hex');writeThermalEvidence('t1-production.json',JSON.stringify({wallMs:performance.now()-started,scriptSha256:hash(code),nativeSha256:hash(readFileSync(join('native/node-addon',nativeFilename()))),result}));
});
test('thermal native numerical failure and budget rejection recover in next scenario',async()=>{
 const base=`const a=analysis();const n=a.thermalNetwork({nodes:[{id:'j',heatCapacityJPerK:2},{id:'a',boundary:'environment'}],links:[{from:'j',to:'a',resistanceKPerW:10}],sources:[{node:'j',powerW:3}]});`;
 const bad=await runAnalysisDsl({code:base+`await a.thermal().network(n).environment({name:'room',ambientC:25}).domain('time').initialC('ambient').time({stopS:10,sampleStepS:1}).numerics({maxIterations:1}).run();`});assert.equal(bad.error?.code,'THERMAL_ITERATIONS');
 const good=await runAnalysisDsl({code:base+`await a.thermal().network(n).environment({name:'room',ambientC:25}).domain('steady').run();`});assert.equal(good.status,'complete',JSON.stringify(good.error));
});


test('thermal native deadline and report budget preserve subsequent recovery',{timeout:20000},async()=>{
 const base=`const a=analysis();const n=a.thermalNetwork({nodes:[{id:'j',heatCapacityJPerK:2},{id:'a',boundary:'environment'}],links:[{from:'j',to:'a',resistanceKPerW:10}],sources:[{node:'j',powerW:{kind:'sine',offset:3,amplitude:1,periodS:1,phaseDeg:0}}]});`;
 const long=base+`await a.thermal().network(n).environment({name:'room',ambientC:25}).domain('time').initialC('ambient').time({stopS:100000,sampleStepS:10000}).run();`;
 const deadline=await runAnalysisDsl({code:long.replace('.run();',`.budget({maxSeconds:0.001}).run();`)});assert.equal(deadline.error?.code,'THERMAL_DEADLINE',JSON.stringify(deadline.error));
 const bounded=await runAnalysisDsl({code:base+`await a.thermal().network(n).environment({name:'room',ambientC:25}).domain('time').initialC('ambient').time({stopS:100000,sampleStepS:.01}).run();`});assert.equal(bounded.error?.code,'THERMAL_BUDGET');
 const recovered=await runAnalysisDsl({code:base+`await a.thermal().network(n).environment({name:'room',ambientC:25}).domain('steady').run();`});assert.equal(recovered.status,'complete',JSON.stringify(recovered.error));
});


test('production DC loss transfer resolves saved meshReference and conserves triangle and finite barrel watts',async()=>{
 const board=dcCoupon();board.layers.push({name:'BOTTOM',index:1,side:'bottom'});
 for(const p of [...board.pads]){if(p.id==='rail'||p.id==='return')board.pads.push({...p,id:p.id+'-bottom',layers:['BOTTOM']});if(p.component==='U1')p.layers=['BOTTOM'];}
 board.copper.fixed.vias.push(...[0,5].map((y,i)=>({id:'thermal-transfer-via-'+i,net:i?'GND':'VCC',at:{x:5,y},diameterMm:.6,drillMm:.3,fromLayer:'TOP',toLayer:'BOTTOM',type:'through' as const})));
 const setup=couponSetup.replace("cu:{conductivitySM:58e6,state:'constant isotropic coupon 20 C'}","cu:{conductivitySM:58e6,state:'constant isotropic coupon 20 C'},resin:{state:'Authored electrically insulating core'}").replace('maxAreaMm2:.125','maxAreaMm2:1').replace("layers:[{kind:'copper',name:'TOP',thicknessMm:.035,material:'cu'}]","viaPlatingThicknessUm:25,platingMaterial:'cu',diameterConvention:'finished_hole',layers:[{kind:'copper',name:'TOP',thicknessMm:.035,material:'cu'},{kind:'dielectric',name:'core',thicknessMm:1,material:'resin'},{kind:'copper',name:'BOTTOM',thicknessMm:.035,material:'cu'}]");
 const result=await runAnalysisDsl({board,code:`${setup}
 await base.loads([{port:load,currentA:1}]).run();const dc=await base.loads([{port:load,currentA:2}]).run();
 const materials={cu:{thermalConductivityWPerMK:390,source:'Authored coupon copper',state:'constant'},resin:{thermalConductivityWPerMK:.3,source:'Authored coupon resin',state:'constant'}};
 const t=await a.thermal().losses(dc.lossMap).materials(materials).stackup(stackup({viaPlatingThicknessUm:25,platingMaterial:'cu',diameterConvention:'finished_hole',layers:[{kind:'copper',name:'TOP',thicknessMm:.035,material:'cu',backgroundMaterial:'resin'},{kind:'dielectric',name:'core',thicknessMm:1,material:'resin'},{kind:'copper',name:'BOTTOM',thicknessMm:.035,material:'cu',backgroundMaterial:'resin'}]})).mesh({maxEdgeMm:2,maxNodes:10000}).cooling({boardSurfaces:a.model({kind:'cooling',resistanceKPerW:10})}).environment({name:'room',ambientC:25}).domain('steady').run();
 a.check({name:'conserved transferred watts',actual:t.temperatureMap.min(),min:25,unit:'degC'});`},{timeoutMs:120000,maxOutputBytes:32*1024*1024});
 assert.equal(result.status,'complete',JSON.stringify(result.error));const calculations=result.reports[0].calculations.filter((c:any)=>c.phase==='completed')as any[];assert.ok(calculations[1].result.solution.meshReference);const dc=calculations[1].result.solution,t=thermalResults(result)[0].solution;
 assert.ok(dc.barrel_loss_w>0,'Current crosses finite barrel segments');assert.ok(t.lossTransfer.barrelSegments>0);assert.ok(Math.abs(t.inputPowerW[0]-dc.copper_loss_w-dc.barrel_loss_w)<1e-12);assert.equal(t.lossTransfer.solutionId,dc.id);assert.equal(t.lossTransfer.triangles,dc.domains.reduce((n:number,d:any)=>n+d.loss_w.length,0));assert.ok(t.quality.normalizedEnergyBalance<=1e-6);assert.equal(result.reports[0].checks[0].status,'pass');
 writeThermalEvidence('loss-transfer-production.json',JSON.stringify({board,result}));
});


test('abort kills an observed production native thermal solve and next scenario recovers',{timeout:30000},()=>{
 const directory=mkdtempSync(join(tmpdir(),'thermal-native-cancel-')),log=join(directory,'pids.jsonl');
 const code=`const a=analysis();const n=a.thermalNetwork({nodes:[{id:'j',heatCapacityJPerK:2},{id:'a',boundary:'environment'}],links:[{from:'j',to:'a',resistanceKPerW:10}],sources:[{node:'j',powerW:{kind:'sine',offset:3,amplitude:1,periodS:1,phaseDeg:0}}]});await a.thermal().network(n).environment({name:'room',ambientC:25}).domain('time').initialC('ambient').numerics({maxIterations:1000000}).time({stopS:100000,sampleStepS:10000}).run();`;
 const source=`import assert from 'node:assert/strict';import{existsSync,readFileSync}from'node:fs';import{runAnalysisDsl}from'./dist/index.js';
 const records=()=>existsSync(process.env.ANALYSIS_THERMAL_PID_LOG)?readFileSync(process.env.ANALYSIS_THERMAL_PID_LOG,'utf8').trim().split(/\\r?\\n/).filter(Boolean).map(JSON.parse):[];
 const alive=pid=>{try{process.kill(pid,0);return true}catch{return false}};
 const controller=new AbortController();const pending=runAnalysisDsl({code:${JSON.stringify(code)}},{signal:controller.signal,timeoutMs:20000});let started;const deadline=Date.now()+15000;
 while(!(started=records().find(r=>r.phase==='solve-start'))){assert.ok(Date.now()<deadline,'Native thermal solve did not start');await new Promise(r=>setTimeout(r,20));}
 await new Promise(r=>setTimeout(r,80));controller.abort();const result=await pending;assert.equal(result.error?.code,'ABORTED');assert.equal(alive(started.pid),false);assert.ok(!records().some(r=>r.phase==='solve-end'),'Solve finished before abort');
 const recovered=await runAnalysisDsl({code:${JSON.stringify(readFileSync('tests/fixtures/analysis-thermal.js','utf8'))}},{maxOutputBytes:8*1024*1024});assert.equal(recovered.status,'complete',JSON.stringify(recovered.error));console.log(JSON.stringify({killedPid:started.pid,error:result.error.code,recovery:recovered.status}));`;
 const child=spawnSync(process.execPath,['--input-type=module','-e',source],{cwd:resolve('.'),windowsHide:true,encoding:'utf8',timeout:25000,maxBuffer:2*1024*1024,env:{...process.env,NODE_OPTIONS:`--require="${resolve('tests/fixtures/analysis-thermal-pid.cjs').replaceAll('\\','/')}"`,ANALYSIS_THERMAL_PID_LOG:log}});
 writeThermalEvidence('cancellation.log',(child.stdout??'')+(child.stderr??''));assert.ifError(child.error);assert.equal(child.status,0,child.stdout+child.stderr);assert.doesNotMatch(child.stderr,/CubeCL|CUDA|Vulkan/i);
});
