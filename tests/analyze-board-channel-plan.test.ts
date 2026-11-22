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
 assert.match(p.returnClosure.reason,/distributed interface coordinates/);
 assert.equal(p.contacts.length,4);assert.ok(p.sourceCopper.length>800);assert.ok(p.physicalIslands.length);assert.ok(p.bores.length);assert.ok(p.corridors.every((c:any)=>Array.isArray(c.geometricIntervalsMm)&&Array.isArray(c.wideReturnDomains)&&Array.isArray(c.geometricEvents)));
 for(const [ref,num]of [['U5','7'],['J5','2'],['D5','2'],['JP2','2']]){const i=board.pads.findIndex((v:any)=>v.component===ref&&v.number===num);assert.ok(p.signalRegions.some((d:any)=>d.sourcePath===`pads[${i}]`));}
 assert.ok(p.signalRegions.some((d:any)=>d.layer==='BOTTOM'));assert.equal(p.corridors.reduce((n:number,c:any)=>n+c.runId.split('+').length,0),16);
 assert.ok(p.corridors.every((c:any)=>c.lateralContinuations.some((v:any)=>v.sourcePath.includes('zones[0]'))));
});

test('STM32 closure trace accounts for growth without changing the candidate',async()=>{
 const root='tests/pcb-analyze/STM32G0_CAN_RS485/';
 const board=JSON.parse(readFileSync(root+'STM32G0_CAN_RS485.json','utf8'));
 const code=readFileSync(root+'local-extraction-complete-channel.js','utf8').split('a.plot(')[0];
 const output=await executeScenario({board,code},{filename:'closure-trace.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{
  let extraction:any;const base=localRequest(task as any,p=>extraction=p);
  const planner=createBoardChannelPlanner(task.board,base,extraction.terminalRequest??extraction.contacts);
  const capture=(onClosureStep?:any)=>{
   try{planner({autoFoilDeadlineMs:performance.now()+10000,onClosureStep});assert.fail('Expected finite-return refusal');}
   catch(e:any){assert.equal(e.code,'CHANNEL_CORRIDOR_UNSUPPORTED');assert.equal(e.geometryReason,'finite-return-closure');assert.ok(e.foilCandidate);return e.foilCandidate;}
  };
  const plain=capture(),steps:any[]=[];const traced=capture((s:any)=>steps.push(s));
  assert.deepEqual(traced,plain);
  assert.equal(steps[0].reason,'seed');assert.equal(steps.at(-1).reason,'complete');
  const regions=new Set<string>(),barrels=new Set<string>();
  for(const step of steps){
   for(const id of step.addedRegionIds){assert.ok(!regions.has(id));regions.add(id);}
   for(const id of step.addedBarrelIds){assert.ok(!barrels.has(id));barrels.add(id);}
   assert.equal(regions.size,step.selectedRegions);assert.equal(barrels.size,step.selectedBarrels);
  }
  assert.deepEqual([...regions].sort(),traced.sourceCopper.map((r:any)=>r.regionId).sort());
  assert.deepEqual([...barrels].sort(),traced.sourceBarrels.map((r:any)=>r.barrelId).sort());
  assert.ok(steps.some(s=>s.reason==='intersecting-islands'&&s.addedRegionIds.length>0));
  assert.equal(traced.geometry.allowInsulatedCopperCuts,false);
  return {id:'trace',status:'complete',diagnostics:[],assumptions:[],metrics:{}};
 });assert.equal(output.status,'complete',JSON.stringify(output.error));
});

test('channel refinement reuses one captured board and preserves independent plan results',async()=>{
 const board=segmentedCoupon();
 const output=await executeScenario({board,code:segmentedCouponDsl.split('a.plot(')[0]},{filename:'reuse-plan.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{
  let contacts:any;const base=localRequest(task as any,p=>contacts=p.contacts);
  const plan=createBoardChannelPlanner(board,base,contacts);
  const initial=plan();
  const guarded=plan({closedJointDeadlineMs:performance.now()+10000,guardMarginMm:0.5});
  const wider=plan({closedJointDeadlineMs:performance.now()+10000,guardMarginMm:0.5,domainScale:2});
  assert.ok(guarded.returnSearch);
  const search1=guarded.returnSearch!.expand(1,performance.now()+10000);
  const search2=guarded.returnSearch!.expand(2,performance.now()+10000);
  assert.deepEqual(search1.geometry.coordinateOriginMm,search2.geometry.coordinateOriginMm);
  assert.equal(JSON.stringify(guarded).includes('returnSearch'),false);
  const boundary=search1.geometry.returnBoundaryMm as number[][];
  assert.deepEqual(boundary[0].slice(0,2),search1.observationRoiMm![0]);
  assert.deepEqual(boundary[1].slice(0,2),search1.observationRoiMm![1]);
  assert.ok(boundary[0][2]<0 && boundary[1][2]>0);
  assert.ok(Number(search1.geometry.electricAirMarginM)>=2*guarded.returnSearch!.initialMarginMm/1000);
  assert.ok(guarded.observationRoiMm);
  for(let axis=0;axis<2;axis++){
   assert.ok(guarded.roiMm[0][axis]<=guarded.observationRoiMm![0][axis]-0.5+1e-9);
   assert.ok(guarded.roiMm[1][axis]>=guarded.observationRoiMm![1][axis]+0.5-1e-9);
   assert.ok(wider.observationRoiMm![0][axis]<=guarded.observationRoiMm![0][axis]);
   assert.ok(wider.observationRoiMm![1][axis]>=guarded.observationRoiMm![1][axis]);
  }
  assert.ok(guarded.sourceCopper.every(r=>wider.sourceCopper.some(w=>w.regionId===r.regionId)));
  assert.ok(guarded.sourceBarrels.every(r=>wider.sourceBarrels.some(w=>w.barrelId===r.barrelId)));
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
test('split finite return keeps its gap in one joint event and distinct copper contacts',async()=>{
 const board=segmentedCoupon();board.copper.fixed.tracks[1].points=[{x:0,y:0},{x:4.5,y:0}];board.copper.fixed.tracks.push({id:'right-return',net:'RETURN',layer:'BOTTOM',widthMm:.5,points:[{x:5.5,y:0},{x:10,y:0}]});
 const plan=await extract(board,segmentedCouponDsl);
 validateChannelOwnership(plan);
 assert.deepEqual(plan.blocks.map((block:any)=>block.kind),['local','section','local','section','local']);
 const joint=plan.blocks[2];
 assert.equal(joint.cuts.length,2);
 const returns=plan.audit.faces.flatMap((face:any)=>face.sourceMapping)
  .filter((mapping:any)=>mapping.layer==='BOTTOM').map((mapping:any)=>mapping.island);
 assert.equal(new Set(returns).size,2);
 assert.equal(plan.audit.returnIslands.length,2);
 assert.equal(joint.geometry.electrodes.filter((electrode:any)=>electrode.layer==='BOTTOM').length,2);
 assert.ok(!plan.audit.corridors.some((corridor:any)=>corridor.admissibleIntervalsMm.some(([lo,hi]:[number,number])=>lo<5&&hi>5)));
});
test('finite wide return closes on its physical edge independent of CAD subdivision or a detached same-net island',async()=>{
 const board=segmentedCoupon();
 board.outline=[{x:-1,y:-1.6},{x:11,y:-1.6},{x:11,y:1.6},{x:-1,y:1.6}];
 const tracks=board.copper.fixed.tracks;
 tracks[1].widthMm=1;
 tracks.push({id:'unconnected-return',net:'RETURN',layer:'BOTTOM',widthMm:.15,points:[{x:0,y:1.2},{x:10,y:1.2}]});
 const before=await extract(board,segmentedCouponDsl);
 tracks[1].points=[{x:0,y:0},{x:5,y:0}];
 tracks.push({...tracks[1],id:'split-return',points:[{x:5,y:0},{x:10,y:0}]});
 const after=await extract(board,segmentedCouponDsl);
 for(const plan of [before,after]){
  validateChannelOwnership(plan);
  assert.deepEqual(plan.blocks.map((block:any)=>block.kind),['local','section','local']);
  assert.ok(plan.audit.returnClosure.transverseMm[0]<-.5);
  assert.ok(plan.audit.returnClosure.transverseMm[1]>.5);
  assert.deepEqual(plan.audit.corridors[0].lateralContinuations,[]);
  assert.equal(plan.audit.returnIslands.length,1);
  assert.ok(plan.audit.copperDecisions.some((d:any)=>d.sourcePath==='copper.fixed.tracks[2]'&&d.role==='neighbour'&&!d.included));
  const metal=plan.blocks[1].line.geometry.conductors.find((m:any)=>m.role==='reference');
  const xy=metal.contour.outerM??metal.contour.outer_m??metal.contour.outer;
  const x=xy.map((p:number[])=>p[0]);
  assert.ok(Math.abs(Math.max(...x)-Math.min(...x)-.001)<1e-10);
 }
 assert.deepEqual(before.audit.returnClosure,after.audit.returnClosure);
 assert.equal(before.blocks[1].line.lengthMm,after.blocks[1].line.lengthMm);
});
test('finite remote copper bypass beyond the original ROI stays inside a joint event',async()=>{
 const board=segmentedCoupon();
 board.outline=[{x:-1,y:-1.8},{x:11,y:-1.8},{x:11,y:1.8},{x:-1,y:1.8}];
 board.copper.fixed.tracks[1].points=[{x:0,y:0},{x:3.5,y:0},{x:5,y:1.2},{x:6.5,y:0},{x:10,y:0}];
 const plan=await extract(board,segmentedCouponDsl);
 validateChannelOwnership(plan);
 assert.equal(plan.audit.returnIslands.length,1);
 assert.ok(plan.audit.returnClosure.transverseMm[1]>1.2);
 assert.deepEqual(plan.blocks.map((block:any)=>block.kind),['local','section','local','section','local']);
 const joint=plan.blocks[2];
 assert.ok(joint.geometry.domainPolygonMm.some((p:number[])=>p[1]>1.2));
 assert.ok(joint.ownership.some((source:any)=>source.sourcePath==='copper.fixed.tracks[1]'&&source.layer==='BOTTOM'));
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
