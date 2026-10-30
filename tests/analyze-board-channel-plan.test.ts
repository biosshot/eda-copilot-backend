import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {localRequest} from '../src/analyze/analysis-dsl/local-request.ts';
import {boardChannelPlan} from '../src/analyze/board-channel-plan.ts';
import {validateChannelOwnership} from '../src/analyze/segmented-channel.ts';
import {segmentedCoupon,segmentedCouponDsl} from './helpers/pcb-analysis-segmented-coupon.ts';
async function extract(board:any,code:string){let plan:any;
 const output=await executeScenario({board,code:code.split('a.plot(')[0]},{filename:'actual-segmentation.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{
  let contacts:any;const geometry=localRequest(task as any,p=>contacts=p.contacts);plan=boardChannelPlan(board,geometry,contacts);
  return {id:'plan',status:'complete',method:'Actual geometry segmentation only',diagnostics:[],assumptions:[],metrics:{}};
 });assert.equal(output.status,'complete',JSON.stringify(output.error));return plan;
}
test('captured coupon automatically separates constant section from physical finite-contact locals',async()=>{
 const p=await extract(segmentedCoupon(),segmentedCouponDsl);assert.deepEqual(p.blocks.map((b:any)=>b.kind),['local','section','local']);assert.deepEqual(validateChannelOwnership(p),[1000,10000]);
 const first=p.blocks[0],last=p.blocks[2];assert.equal(first.geometry.cutPlanes['terminal-0'].axis,0);assert.equal(p.ports.length,2);
 for(const b of [first,last])assert.ok(Object.values(b.geometry.cutPlanes).every((c:any)=>c.spanMm[1]>c.spanMm[0]));
 assert.ok(p.audit.copperDecisions.every((d:any)=>d.sourcePath&&d.layer&&d.reason));
});
test('actual remote slab plan retains branches, layer changes and both physical return contacts',async()=>{
 const base='tests/pcb-analyze/STM32G0_CAN_RS485/',board=JSON.parse(readFileSync(base+'STM32G0_CAN_RS485.json','utf8')),code=readFileSync(base+'local-extraction-complete-channel.js','utf8').split('a.plot(')[0],p=await extract(board,code);
 validateChannelOwnership(p);const decisions=p.audit.copperDecisions;
 for(const [ref,num]of [['U5','7'],['J5','2'],['D5','2'],['JP2','2']]){const i=board.pads.findIndex((v:any)=>v.component===ref&&v.number===num);assert.ok(decisions.some((d:any)=>d.sourcePath===`pads[${i}]`&&d.role==='signal'&&d.included));}
 assert.ok(decisions.some((d:any)=>d.role==='signal'&&d.layer==='BOTTOM'&&d.included));assert.equal(p.ports.length,2);
 // Close geometric changes deliberately merge into one joint solve; a short
 // electrical path must never erase the remote branch or surrounding coupling.
 assert.ok(p.blocks.every((b:any)=>b.kind==='local'));
});
