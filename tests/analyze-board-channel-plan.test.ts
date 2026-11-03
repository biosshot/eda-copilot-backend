import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {localRequest} from '../src/analyze/analysis-dsl/local-request.ts';
import {boardChannelPlan,createBoardChannelPlanner} from '../src/analyze/board-channel-plan.ts';
import {validateChannelOwnership} from '../src/analyze/segmented-channel.ts';
import {segmentedCoupon,segmentedCouponVariant,segmentedCouponDsl,segmentedCouponDslFor} from './helpers/pcb-analysis-segmented-coupon.ts';
async function extract(board:any,code:string,allowRefusal=false){let plan:any;
 const output=await executeScenario({board,code:code.split('a.plot(')[0]},{filename:'actual-segmentation.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{
  let contacts:any;const geometry=localRequest(task as any,p=>contacts=p.contacts);try{plan=boardChannelPlan(board,geometry,contacts);}catch(error){const e=error as any;if(!allowRefusal||e.code!=='CHANNEL_CORRIDOR_UNSUPPORTED')throw error;plan={refusal:e.code,...e.corridorAudit};}
  return {id:'plan',status:'complete',method:'Actual geometry segmentation only',diagnostics:[],assumptions:[],metrics:{}};
 });assert.equal(output.status,'complete',JSON.stringify(output.error));return plan;
}
test('captured coupon automatically separates constant section from physical finite-contact locals',async()=>{
 const p=await extract(segmentedCoupon(),segmentedCouponDsl);assert.deepEqual(p.blocks.map((b:any)=>b.kind),['local','section','local']);assert.deepEqual(validateChannelOwnership(p),[1000,10000]);
 const first=p.blocks[0],last=p.blocks[2];assert.equal(Math.abs(first.geometry.cutPlanes['terminal-0'].normalXY[0]),1);assert.equal(p.ports.length,2);
 for(const b of [first,last])assert.ok(Object.values(b.geometry.cutPlanes).every((c:any)=>c.spanMm[1]>c.spanMm[0]));
 assert.ok(p.audit.copperDecisions.every((d:any)=>d.sourcePath&&d.layer&&d.reason));
});
test('actual STM32 refuses distributed lateral return interfaces while preserving branch witnesses',async()=>{
 const base='tests/pcb-analyze/STM32G0_CAN_RS485/',board=JSON.parse(readFileSync(base+'STM32G0_CAN_RS485.json','utf8')),code=readFileSync(base+'local-extraction-complete-channel.js','utf8').split('a.plot(')[0],p=await extract(board,code,true);
 assert.equal(p.refusal,'CHANNEL_CORRIDOR_UNSUPPORTED');
 assert.equal(p.contacts.length,4);assert.ok(p.sourceCopper.length>800);assert.ok(p.physicalIslands.length);assert.ok(p.bores.length);assert.ok(p.corridors.every((c:any)=>Array.isArray(c.geometricIntervalsMm)&&Array.isArray(c.wideReturnDomains)&&Array.isArray(c.geometricEvents)));
 for(const [ref,num]of [['U5','7'],['J5','2'],['D5','2'],['JP2','2']]){const i=board.pads.findIndex((v:any)=>v.component===ref&&v.number===num);assert.ok(p.signalRegions.some((d:any)=>d.sourcePath===`pads[${i}]`));}
 assert.ok(p.signalRegions.some((d:any)=>d.layer==='BOTTOM'));assert.equal(p.corridors.reduce((n:number,c:any)=>n+c.runId.split('+').length,0),16);
 assert.ok(p.corridors.every((c:any)=>c.lateralContinuations.some((v:any)=>v.sourcePath.includes('zones[0]'))));
});

test('channel refinement reuses one captured board and preserves independent plan results',async()=>{
 const board=segmentedCoupon();
 const output=await executeScenario({board,code:segmentedCouponDsl.split('a.plot(')[0]},{filename:'reuse-plan.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{
  let contacts:any;const base=localRequest(task as any,p=>contacts=p.contacts);
  const plan=createBoardChannelPlanner(board,base,contacts);
  const initial=plan();
  assert.deepEqual(plan({domainScale:2,magneticPatchLevel:2}),boardChannelPlan(board,base,contacts,{domainScale:2,magneticPatchLevel:2}));
  const coarse=plan({magneticPatchLevel:1}),fine=plan({magneticPatchLevel:2}),expanded=plan({domainScale:2,magneticPatchLevel:1});
  assert.ok(coarse.blocks.filter((b:any)=>b.kind==='local').every((b:any)=>b.geometry.maxTriangleAreaM2===5e-8));
  for(const [index,block]of coarse.blocks.entries())if(block.kind==='local'){
   assert.deepEqual(block.geometry.coordinateOriginMm,base.roiMm[0]);
   assert.deepEqual((fine.blocks[index]as any).geometry.coordinateOriginMm,block.geometry.coordinateOriginMm);
   assert.deepEqual((expanded.blocks[index]as any).geometry.coordinateOriginMm,block.geometry.coordinateOriginMm);
   assert.equal(block.geometry.electricAirMarginM,.008);
   assert.equal((expanded.blocks[index]as any).geometry.electricAirMarginM,.016);
  }
  for(const [a,b]of coarse.blocks.map((block:any,i:number)=>[block,fine.blocks[i]])){
   if(a.kind==='local')assert.equal(a.geometry.maxTriangleAreaM2,4*b.geometry.maxTriangleAreaM2);
   else assert.equal(a.line.geometry.maxTriangleAreaM2,2*b.line.geometry.maxTriangleAreaM2);
  }
  // A caller edit cannot change the board already captured for this calculation.
  board.pads=[];
  assert.deepEqual(plan(),initial);
  return {id:'plan',status:'complete',method:'geometry reuse control',diagnostics:[],assumptions:[],metrics:{}};
 });assert.equal(output.status,'complete',JSON.stringify(output.error));
});
for(const angle of [45,90])test(`rotated physical coupon retains uniform section at ${angle} degrees`,async()=>{
 const board=segmentedCouponVariant(angle===45?'rotated45':'rotated90');
 const p=await extract(board,segmentedCouponDsl);assert.ok(p.blocks.some((b:any)=>b.kind==='section'));validateChannelOwnership(p);
});
test('finite-return bend retains two uniform corridors and joint elbow geometry',async()=>{
 const board=segmentedCouponVariant('bend');
 const p=await extract(board,segmentedCouponDsl);validateChannelOwnership(p);assert.ok(p.blocks.filter((b:any)=>b.kind==='section').length>=2);assert.ok(p.blocks.some((b:any)=>b.kind==='local'));assert.equal(p.ports.length,2);
});
test('coupled corridor retains neighbouring conductor and off-diagonal section coordinates',async()=>{
 const board=segmentedCoupon();board.nets.push({name:'VICTIM'});board.copper.fixed.tracks.push({id:'victim',net:'VICTIM',layer:'TOP',widthMm:.1,points:[{x:0,y:.18},{x:10,y:.18}]});
 const p=await extract(board,segmentedCouponDsl);validateChannelOwnership(p);const sections=p.blocks.filter((b:any)=>b.kind==='section');assert.ok(sections.length);assert.ok(sections.every((b:any)=>b.line.geometry.conductors.length===3&&b.cuts.length===4&&b.dcConductors.resistanceOhmPerM[0][1]>0));
});
test('broken finite return refuses unsupported reference-free corridor instead of joining a net name',async()=>{
 const board=segmentedCoupon();board.copper.fixed.tracks[1].points=[{x:0,y:0},{x:4.5,y:0}];board.copper.fixed.tracks.push({id:'right-return',net:'RETURN',layer:'BOTTOM',widthMm:.5,points:[{x:5.5,y:0},{x:10,y:0}]});
 await assert.rejects(()=>extract(board,segmentedCouponDsl),/No finite return/);
});
test('source subdivision preserves one physical run and conductor interface across source identities',async()=>{
 const board=segmentedCoupon(),before=await extract(board,segmentedCouponDsl);board.copper.fixed.tracks[0].points=[{x:0,y:0},{x:5,y:0}];board.copper.fixed.tracks.push({id:'signal-b',net:'SIGNAL',layer:'TOP',widthMm:.1,points:[{x:5,y:0},{x:10,y:0}]});
 const after=await extract(board,segmentedCouponDsl);validateChannelOwnership(after);const a=before.blocks.filter((b:any)=>b.kind==='section'),b=after.blocks.filter((b:any)=>b.kind==='section');assert.equal(a.length,1);assert.equal(b.length,1);assert.ok(Math.abs(a[0].line.lengthMm-b[0].line.lengthMm)<1e-6);assert.ok(after.audit.graph.runs.some((r:any)=>r.sourcePaths.length===2));
});
test('parallel meander legs are extracted as one coupled physical corridor',async()=>{
 const board=segmentedCoupon();board.outline=[{x:-.3,y:-.6},{x:10.3,y:-.6},{x:10.3,y:.8},{x:-.3,y:.8}];
 for(const track of board.copper.fixed.tracks)track.points=[{x:0,y:0},{x:10,y:0},{x:10,y:.18},{x:0,y:.18}];
 for(const pad of board.pads.filter((p:any)=>p.component==='J1'))pad.at={x:0,y:.18};board.components[1].at={x:0,y:.18};
 const p=await extract(board,segmentedCouponDsl.replace("negative:pad('J1','2')","negative:pad('J0','2')"));validateChannelOwnership(p);const sections=p.blocks.filter((b:any)=>b.kind==='section');assert.ok(sections.length);const longSections=sections.filter((s:any)=>s.line.lengthMm>1);assert.ok(longSections.length);assert.ok(longSections.every((s:any)=>s.cuts.length===4&&s.line.geometry.conductors.length===3),'Both long meander legs must retain joint mutual coordinates');
});

test('physical via/finite-return corridor retains layer transition in a joint local event',async()=>{
 const p=await extract(segmentedCouponVariant('via'),segmentedCouponDslFor('via'));validateChannelOwnership(p);assert.equal(p.audit.barrelOwnership.length,1);assert.equal(p.blocks.find((b:any)=>b.id===p.audit.barrelOwnership[0].ownerBlockId).kind,'local');assert.ok(p.blocks.some((b:any)=>b.kind==='section'));assert.ok(p.blocks.some((b:any)=>b.kind==='local'&&b.ownership.some((o:any)=>o.sourcePath.includes('vias[0]'))));assert.ok(p.audit.copperDecisions.some((r:any)=>r.role==='signal'&&r.layer==='BOTTOM'&&r.included));
});
test('physical fragment ownership refuses invented areas, duplicated copper and missing source pieces',async()=>{
 const p=await extract(segmentedCoupon(),segmentedCouponDsl);const area=structuredClone(p);area.blocks[0].ownership[0].areaMm2+=.01;assert.throws(()=>validateChannelOwnership(area),/original polygon/);
 const duplicate=structuredClone(p);duplicate.blocks[1].ownership.push(structuredClone(duplicate.blocks[0].ownership[0]));assert.throws(()=>validateChannelOwnership(duplicate),/Overlapping/);
 const gap=structuredClone(p);gap.blocks[0].ownership.shift();assert.throws(()=>validateChannelOwnership(gap),/Uncovered|coverage/);
 assert.ok(p.coverage.every((f:any)=>f.kind==='fragment'&&f.areaMm2>0&&f.startMm===undefined&&f.stopMm===undefined));
});
