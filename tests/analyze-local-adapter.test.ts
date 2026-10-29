import test from'node:test';import assert from'node:assert/strict';import{readFileSync}from'node:fs';import{executeScenario}from'../src/analyze/analysis-dsl/scenario.ts';import{localRequest}from'../src/analyze/analysis-dsl/local-request.ts';import type{AnalysisTask}from'../src/analyze/analysis-dsl/executor.ts';
test('existing direct channel DSL preserves exact original STM32 request and physical return TOP',async()=>{
 const folder='tests/pcb-analyze/STM32G0_CAN_RS485/';const board=JSON.parse(readFileSync(folder+'STM32G0_CAN_RS485.json','utf8'));const script=readFileSync(folder+'local-extraction.js','utf8');const code=script.slice(0,script.indexOf('const cold='))+'await run();';let captured:unknown;
 const result=await executeScenario({board,code},{filename:'local.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{captured=localRequest(task as AnalysisTask<'impedance'>);return{id:'test',status:'complete',method:'transport test only',diagnostics:[],assumptions:[],metrics:{}};});
 assert.equal(result.status,'complete',JSON.stringify(result.error));const expected=JSON.parse(readFileSync('tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json','utf8'));assert.deepEqual(captured,expected);
});

test('direct channel without scope plans a layer-changing finite-via coupon with mandatory numerical domain verification',async()=>{
 const folder='tests/pcb-analyze/STM32G0_CAN_RS485/';const board=JSON.parse(readFileSync(folder+'STM32G0_CAN_RS485.json','utf8'));const script=readFileSync(folder+'local-extraction.js','utf8');const code=script.slice(0,script.indexOf('const cold=')).replace('scope:[scope]','')+'await run();';let captured:any,plan:any;
 board.pads=[];board.copper.fixed.tracks=[];board.copper.fixed.zones=[];board.copper.fixed.vias=board.copper.fixed.vias.filter((v:any)=>['da3574692d072a0d','aa8ff23739620a07'].includes(v.id));
 const result=await executeScenario({board,code},{filename:'automatic-local.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{captured=localRequest(task as AnalysisTask<'impedance'>,p=>{plan=p;});return{id:'test',status:'complete',method:'transport test only',diagnostics:[],assumptions:[],metrics:{}};});
 assert.equal(result.status,'complete',JSON.stringify(result.error));assert.equal(plan.automaticConvergenceRequired,true);assert.equal(plan.candidateDomainsMm.length,3);assert.ok(plan.selectedRegionIds.length>0);assert.equal(captured.electrodes[0].layer,'TOP');assert.equal(captured.electrodes[1].layer,'BOTTOM');assert.match(plan.spatialValidation,/unverified/);
});

test('automatic plan retains signal branches and floating neighbours, without net-name joins',async()=>{
 const {channelPlan}=await import('../src/analyze/analysis-dsl/channel-plan.ts');
 const region=(id:string,x:number)=>({id,layer:'TOP',source:{path:id},polygon:{outer:[[x,0],[x+1,0],[x+1,1],[x,1]],holes:[]}});
 const g:any={status:'complete',excluded:[],fills:[],diagnostics:[],boolean_error_bound_mm:0,regions:[region('in',0),region('out',2),region('branch',4),region('return',0),region('floating',1.5),region('unrelated',20)],islands:[{id:'signal',region_ids:['in','out','branch']},{id:'ref',region_ids:['return']},{id:'float',region_ids:['floating']},{id:'other',region_ids:['unrelated']}]};
 const contacts=[{sourcePath:'in',layer:'TOP'},{sourcePath:'out',layer:'TOP'},{sourcePath:'return',layer:'TOP'}];
 const p=channelPlan(g,contacts);assert.deepEqual(p.selectedRegionIds,['branch','floating','in','out','return']);assert.ok(p.roiMm[1][0]<20);
 g.islands[0].region_ids=['in','branch'];g.islands.push({id:'detached',region_ids:['out']});
 assert.throws(()=>channelPlan(g,contacts),/physically disconnected/);
});

test('conservative native topology diagnostics become explicit assumptions without creating contacts',async()=>{
 const {channelPlan}=await import('../src/analyze/analysis-dsl/channel-plan.ts');
 const r=(id:string,x:number)=>({id,layer:'TOP',source:{path:id},polygon:{outer:[[x,0],[x+1,0],[x+1,1],[x,1]],holes:[]}});
 const g:any={status:'partial',excluded:[],fills:[],boolean_error_bound_mm:0,regions:[r('in',0),r('out',2)],islands:[{id:'confirmed',region_ids:['in','out']}],diagnostics:[{code:'contact_uncertain',path:'in'},{code:'drill_topology_uncertain',path:'out'},{code:'contact_uncertain',path:'remote'}]};
 const p=channelPlan(g,[{sourcePath:'in',layer:'TOP'},{sourcePath:'out',layer:'TOP'}]);assert.deepEqual(p.diagnosticDecisions.map(d=>d.decision),['assumed','assumed','ignored']);assert.equal(g.islands.length,1);assert.match(p.diagnosticDecisions[0].reason,/not used to join/);
});


test('interior via terminal uses a stable physical copper face in either channel direction',async()=>{
 const folder='tests/pcb-analyze/STM32G0_CAN_RS485/';const board=JSON.parse(readFileSync(folder+'STM32G0_CAN_RS485.json','utf8'));
 board.layers=[{name:'TOP',index:0,side:'top'},{name:'INNER1',index:1,side:'inner'},{name:'BOTTOM',index:2,side:'bottom'}];
 board.pads=[];board.copper.fixed.tracks=[];board.copper.fixed.zones=[];board.copper.fixed.vias=board.copper.fixed.vias.filter((v:any)=>['da3574692d072a0d','aa8ff23739620a07'].includes(v.id));
 let code=readFileSync(folder+'local-extraction-automatic.js','utf8').split('const result=')[0]+'await run();';
 code=code.replace("{kind:'dielectric',name:'authored-FR4',thicknessMm:1.53,material:'fr4'},", "{kind:'dielectric',name:'FR4-a',thicknessMm:.7475,material:'fr4'},{kind:'copper',name:'INNER1',thicknessMm:.035,material:'copper'},{kind:'dielectric',name:'FR4-b',thicknessMm:.7475,material:'fr4'},").replace('positiveLayer:"BOTTOM"','positiveLayer:"INNER1"');
 let captured:any;const run=async(source:string)=>executeScenario({board,code:source},{filename:'interior.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{captured=localRequest(task as AnalysisTask<'impedance'>);return{id:'test',status:'complete',method:'transport only',diagnostics:[],assumptions:[],metrics:{}};});
 const forward=await run(code);assert.equal(forward.status,'complete',JSON.stringify(forward.error));assert.equal(captured.electrodes.find((e:any)=>e.layer==='INNER1').upper,false);
 const backward=await run(code.replace('from:input,to:output','from:output,to:input'));assert.equal(backward.status,'complete',JSON.stringify(backward.error));assert.equal(captured.electrodes.find((e:any)=>e.layer==='INNER1').upper,false);
});
