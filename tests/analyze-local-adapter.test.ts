import test from'node:test';import assert from'node:assert/strict';import{readFileSync}from'node:fs';import{executeScenario}from'../src/analyze/analysis-dsl/scenario.ts';import{localRequest}from'../src/analyze/analysis-dsl/local-request.ts';import type{AnalysisTask}from'../src/analyze/analysis-dsl/executor.ts';
test('existing direct channel DSL preserves exact original STM32 request and physical return TOP',async()=>{
 const folder='tests/pcb-analyze/STM32G0_CAN_RS485/';const board=JSON.parse(readFileSync(folder+'STM32G0_CAN_RS485.json','utf8'));const script=readFileSync(folder+'local-extraction.js','utf8');const code=script.slice(0,script.indexOf('const cold='))+'await run();';let captured:unknown;
 const result=await executeScenario({board,code},{filename:'local.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{captured=localRequest(task as AnalysisTask<'impedance'>);return{id:'test',status:'complete',method:'transport test only',diagnostics:[],assumptions:[],metrics:{}};});
 assert.equal(result.status,'complete',JSON.stringify(result.error));const expected=JSON.parse(readFileSync('tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json','utf8'));assert.deepEqual(captured,expected);
});

test('direct channel without scope plans a layer-changing finite-via coupon without insulated cuts',async()=>{
 const folder='tests/pcb-analyze/STM32G0_CAN_RS485/';const board=JSON.parse(readFileSync(folder+'STM32G0_CAN_RS485.json','utf8'));const script=readFileSync(folder+'local-extraction.js','utf8');const code=script.slice(0,script.indexOf('const cold=')).replace('scope:[scope]','')+'await run();';let captured:any,plan:any;
 board.pads=[];board.copper.fixed.tracks=[];board.copper.fixed.zones=[];board.copper.fixed.vias=board.copper.fixed.vias.filter((v:any)=>['da3574692d072a0d','aa8ff23739620a07'].includes(v.id));
 const result=await executeScenario({board,code},{filename:'automatic-local.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{captured=localRequest(task as AnalysisTask<'impedance'>,p=>{plan=p;});return{id:'test',status:'complete',method:'transport test only',diagnostics:[],assumptions:[],metrics:{}};});
 assert.equal(result.status,'complete',JSON.stringify(result.error));assert.equal(captured.allowInsulatedCopperCuts,false);assert.ok(plan.selectedRegionIds.length>0);assert.equal(captured.electrodes[0].layer,'TOP');assert.equal(captured.electrodes[1].layer,'BOTTOM');assert.match(plan.spatialValidation,/unverified/);
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
