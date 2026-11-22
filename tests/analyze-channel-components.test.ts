import test from 'node:test';
import assert from 'node:assert/strict';
import { channelPassiveElement } from '../src/analyze/channel-components.ts';
import { channelTopology, validateChannelOwnership, type ChannelBlock, type ChannelCut, type SegmentedChannel } from '../src/analyze/segmented-channel.ts';
import { createProblemExecutor } from '../src/analyze/execute-problem.ts';
import { capturedBoardGeometry } from '../src/analyze/geometry-snapshot.ts';
import { physicalChannelComponents } from '../src/analyze/analysis-dsl/channel-component-request.ts';
import { segmentedCoupon, segmentedCouponDsl } from './helpers/pcb-analysis-segmented-coupon.ts';
import { boardChannelPlan } from '../src/analyze/board-channel-plan.ts';
import type { BoardChannelRequest } from '../src/analyze/channel-components.ts';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import type { AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import { localRequest } from '../src/analyze/analysis-dsl/local-request.ts';

const cut=(positive:string,negative:string):ChannelCut=>({positive,negative});
const local=(id:string,cuts:ChannelCut[]):ChannelBlock=>({kind:'local',id,cuts,ownership:[],bandHz:[1000,10000],boardJson:'{}',geometry:{}});

test('component pins attach to separately exposed physical terminals across copper blocks',()=>{
 const a=cut('left-port','left-return'), b=cut('right-port','right-return');
 const attachment={id:'component:C1',cut:cut('C1:pad1','C1:pad2'),
  model:channelPassiveElement('C1',{kind:'capacitor',capacitanceF:1e-6,ideal:true},[2000,8000])};
 const plan:SegmentedChannel={
  blocks:[local('left',[a,cut('C1:pad1','left-return')]),local('right',[b,cut('C1:pad2','right-return')])],
  ports:[{name:'left',cut:a},{name:'right',cut:b}],coverage:[],attachments:[attachment],
 };
 assert.deepEqual(validateChannelOwnership(plan),[2000,8000]);
 const topology=channelTopology(plan);
 assert.deepEqual(topology.ports,[{name:'left',index:0},{name:'right',index:2}]);
 assert.deepEqual(topology.connections,[[0,1],[1,4],[2,3],[3,4]]);
 assert.deepEqual(topology.connectionContacts,['negative','positive','negative','positive-negative']);
 assert.throws(()=>validateChannelOwnership({...plan,attachments:[{...attachment,cut:cut('C1:pad1','same-net-label') }]}),/exact exposed copper terminals/);
 assert.throws(()=>validateChannelOwnership({...plan,attachments:[{...attachment,id:'left'}]}),/Duplicate/);
});

test('native P6 keeps authored R/C/L package impedance and energy coordinates',()=>{
 const f=4000, omega=2*Math.PI*f;
 const models=[
  {id:'R',element:{kind:'resistor',resistanceOhm:15},expected:[15,0]},
  {id:'C',element:{kind:'capacitor',capacitanceF:1e-6,ideal:true},expected:[0,-1/(omega*1e-6)]},
  {id:'C-ESR-ESL',element:{kind:'capacitor',capacitanceF:1e-6,esrOhm:.2,eslH:2e-6},expected:[.2,omega*2e-6-1/(omega*1e-6)]},
  {id:'L',element:{kind:'inductor',inductanceH:1e-3,seriesResistanceOhm:.1},expected:[.1,omega*1e-3]},
 ] as const;
 const execute=createProblemExecutor(true);
 for(const {id,element,expected} of models){
  const model=channelPassiveElement(id,element);
  const result=execute({kind:'frequency-network',request:{
   instances:[{id,model,ports:model.ports}],connections:[],ports:[{name:'port',index:0}],
   frequencies:[f],representation:'Z',budget:{maxSeconds:3,maxMemoryMiB:64},
  }});
  assert.equal(result.status,'complete',JSON.stringify(result.diagnostics));
  const actual=result.parameters![0][0][0];
  assert.ok(Math.hypot(actual.real-expected[0],actual.imaginary-expected[1])<1e-9,JSON.stringify({id,actual,expected}));
  const matched=execute({kind:'frequency-network',request:{
   instances:[{id,model,ports:model.ports}],connections:[],ports:[{name:'port',index:0}],
   frequencies:[f],representation:'S',referenceOhm:[50],recoverInstanceLoss:true,
   budget:{maxSeconds:3,maxMemoryMiB:64},
  }});
  assert.equal(matched.status,'complete',JSON.stringify(matched.diagnostics));
  assert.deepEqual(matched.loadedInstanceLoss?.instanceIds,[id]);
  const expectedLoss=expected[0]*2500/((50+expected[0])**2+expected[1]**2);
  const sample=matched.loadedInstanceLoss!.samples[0];
  assert.equal(sample.frequencyHz,f);
  assert.ok(Math.abs(sample.jouleWPerA2[0][0][0]-expectedLoss)<1e-12+1e-9*expectedLoss);
  assert.ok(Math.abs(sample.oppositeDriveW[0]-expectedLoss)<1e-12+1e-9*expectedLoss);

 }
});

test('a capacitor transfers AC across two copper descriptors without shorting their pins',()=>{
 const f=3000, omega=2*Math.PI*f, bridgeC=2e-6, shuntC=5e-9, seriesR=2;
 const left=cut('outer-left','finite-return'), right=cut('outer-right','finite-return');
 const attachment={id:'component:C1',cut:cut('pad-left','pad-right'),
  model:channelPassiveElement('C1',{kind:'capacitor',capacitanceF:bridgeC,ideal:true})};
 const plan:SegmentedChannel={
  blocks:[local('left',[left,cut('pad-left','finite-return')]),local('right',[right,cut('pad-right','finite-return')])],
  ports:[{name:'left',cut:left},{name:'right',cut:right}],coverage:[],attachments:[attachment],
 };
 const copper=(id:string)=>{
  const ports=['outer','pad'];
  return {id,ports,model:{id,kind:'network',ports,data:{
   format:'coupled-rlc',referenceTopology:'infinity',potentialNodes:['outer','pad','return'],currentDofs:['trace'],
   incidence:[[0,0,1],[1,0,-1]],portIncidence:[[0,0,1],[2,0,-1],[1,1,1],[2,1,-1]],
   resistanceOhm:[[0,0,seriesR]],inductanceH:[],
   capacitanceF:[[0,0,shuntC],[2,2,shuntC],[0,2,-shuntC],[2,0,-shuntC]],
   conservedPotentialModes:[[0,0,1]],structuralGaugeModes:[[1,1,1]],
   portTerminals:[{positive:{node:'outer'},negative:{node:'return'}},{positive:{node:'pad'},negative:{node:'return'}}],
  }}};
 };
 const result=createProblemExecutor(true)({kind:'frequency-network',request:{
  instances:[copper('left'),copper('right'),{id:attachment.id,model:attachment.model,ports:attachment.model.ports}],
  ...channelTopology(plan),frequencies:[f],representation:'Y',budget:{maxSeconds:3,maxMemoryMiB:64},
 }});
 assert.equal(result.status,'complete',JSON.stringify(result.diagnostics));
 const matrix=result.parameters![0];
 const r=2*seriesR, x=1/(omega*bridgeC), denominator=r*r+x*x;
 const y={real:r/denominator,imaginary:x/denominator};
 assert.ok(Math.hypot(matrix[0][1].real+y.real,matrix[0][1].imaginary+y.imaginary)<1e-9);
 assert.ok(Math.hypot(matrix[1][0].real+y.real,matrix[1][0].imaginary+y.imaginary)<1e-9);
 assert.ok(Math.hypot(matrix[0][0].real-y.real,matrix[0][0].imaginary-y.imaginary-omega*shuntC)<1e-9);
});

test('existing component descriptions bind explicit equivalents to confirmed original pads',()=>{
 const board=segmentedCoupon();
 const geometry=capturedBoardGeometry(JSON.stringify(board));
 const entry={designator:'J0',kind:'capacitor',pins:{first:'1',second:'2'},capacitanceF:1e-6,ideal:true} as const;
 const [bridge]=physicalChannelComponents([entry],{board},geometry);
 assert.deepEqual(bridge.pins.map(({sourcePath,layer})=>[sourcePath,layer]),[['pads[0]','TOP'],['pads[2]','BOTTOM']]);
 assert.throws(()=>physicalChannelComponents([{...entry,capacitanceF:undefined}],{board},geometry),/Missing explicit component equivalent/);
 assert.throws(()=>physicalChannelComponents([{...entry,pins:{first:'1',second:'not-a-pad'}}],{board},geometry),/Missing or ambiguous physical component pad/);
 const region=geometry.regions.find(r=>r.source.path==='pads[0]')!;
 const ambiguous={...geometry,regions:[...geometry.regions,{...region,id:region.id+'-other',layer:'BOTTOM'}]};
 assert.throws(()=>physicalChannelComponents([entry],{board},ambiguous),/one confirmed copper region on one layer/);
});

async function planComponentDsl(board:any, code:string) {
 let plan:ReturnType<typeof boardChannelPlan>|undefined;
 const output=await executeScenario(
  {board,code},
  {filename:'component-geometry.js',maxEntries:1000,maxOutputBytes:4*1024*1024},
  ()=>{},
  async task=>{
   let request:BoardChannelRequest|undefined;
   const geometry=localRequest(task as AnalysisTask<'impedance'>, prepared=>{request=prepared.terminalRequest;});
   assert.ok(request?.components?.length);
   plan=boardChannelPlan(board,geometry,request);
   return {id:'geometry',status:'complete',method:'Component topology and geometry only',diagnostics:[],assumptions:[],metrics:{}};
  },
 );
 return {output,plan};
}

const couponCode=segmentedCouponDsl.split('a.plot(')[0];

function splitCoupon(boundaries:readonly [number,number][]) {
 const board=segmentedCoupon();
 const source=board.copper.fixed.tracks[0];
 const starts=[0,...boundaries.map(gap=>gap[1])], ends=[...boundaries.map(gap=>gap[0]),10];
 board.copper.fixed.tracks.splice(0,1,...starts.map((x,i)=>({...source,id:'signal-'+i,points:[{x,y:0},{x:ends[i],y:0}]})));
 for(const [index,[left,right]] of boundaries.entries()){
  const designator='C'+(index+1);
  board.components.push({...board.components[0],designator,at:{x:(left+right)/2,y:0}});
  for(const [pin,x] of [left,right].entries()){
   const pad=structuredClone(board.pads[0]);
   pad.id=designator+'-'+pin;
   pad.component=designator;
   pad.number=String(pin+1);
   pad.at={x,y:0};
   board.pads.push(pad);
  }
 }
 return board;
}

const componentCode=(models:string,code=couponCode)=>code.replace('.frequency(','.components(['+models+']).frequency(');
const cap=(name:string)=>"capacitor({designator:'"+name+"',pins:{positive:'1',negative:'2'},capacitanceF:1e-6,ideal:true})";

test('layered DSL exposes both pads around a signal gap in their complete local owner',async()=>{
 const board=splitCoupon([[4.75,5.25]]);
 const {output,plan}=await planComponentDsl(board,componentCode(cap('C1')));
 assert.equal(output.status,'complete',JSON.stringify(output.error));
 assert.ok(plan?.attachments);
 assert.deepEqual(validateChannelOwnership(plan),[1000,10000]);
 assert.equal(plan.audit.signalIslands.length,2);
 const [component]=plan.audit.components;
 assert.equal(new Set(component.pins.map((pin:{owner:string})=>pin.owner)).size,1);
 assert.equal(new Set(component.pins.map((pin:{island:string})=>pin.island)).size,2);
 assert.deepEqual(component.pins.map((pin:{sourcePath:string})=>pin.sourcePath),['pads[4]','pads[5]']);
 const owner=plan.blocks.find(block=>block.id===component.pins[0].owner);
 assert.equal(owner?.kind,'local');
 assert.ok(owner?.cuts.some(c=>c.positive===plan.attachments![0].cut.positive));
 assert.ok(owner?.cuts.some(c=>c.positive===plan.attachments![0].cut.negative));
 assert.equal(channelTopology(plan).ports.length,2);
 const missing=await planComponentDsl(board,couponCode);
 assert.match(JSON.stringify(missing.output.error),/physically disconnected on leg 1/);
});

test('layered DSL follows an authored component chain through an intermediate copper island',async()=>{
 const board=splitCoupon([[2.75,3.25],[6.75,7.25]]);
 const {output,plan}=await planComponentDsl(board,componentCode([cap('C1'),cap('C2')].join(',')));
 assert.equal(output.status,'complete',JSON.stringify(output.error));
 assert.ok(plan);
 assert.deepEqual(validateChannelOwnership(plan),[1000,10000]);
 assert.equal(plan.audit.signalIslands.length,3);
 assert.equal(plan.attachments?.length,2);
 assert.equal(plan.audit.components[0].pins[1].island,plan.audit.components[1].pins[0].island);
 const missing=await planComponentDsl(board,componentCode(cap('C1')));
 assert.match(JSON.stringify(missing.output.error),/physically disconnected on leg 1/);
});

test('a component whose pins are external electrodes reuses each across distinct local owners',async()=>{
 const board=splitCoupon([[4.75,5.25]]);
 board.pads[0].component='C1';
 board.pads[0].number='3';
 board.pads[1].component='C1';
 board.pads[1].number='4';
 const code=couponCode.replace("pad('J0','1')","pad('C1','3')").replace("pad('J1','1')","pad('C1','4')");
 const resistor="resistor({designator:'C1',pins:{positive:'3',negative:'4'},resistanceOhm:47})";
 const {output,plan}=await planComponentDsl(board,componentCode(resistor,code));
 assert.equal(output.status,'complete',JSON.stringify(output.error));
 assert.ok(plan?.attachments);
 assert.deepEqual(validateChannelOwnership(plan),[1000,10000]);
 const [component]=plan.audit.components;
 assert.notEqual(component.pins[0].owner,component.pins[1].owner);
 assert.deepEqual(plan.attachments[0].cut,{
  positive:plan.ports[0].cut.positive,
  negative:plan.ports[1].cut.positive,
 });
 const topology=channelTopology(plan);
 assert.ok(topology.connectionContacts.includes('positive-negative'));
});

test('independent package contacts can be exposed in different local copper blocks',async()=>{
 const board=splitCoupon([[4.75,5.25]]);
 board.pads[4].at.x=.4;
 board.pads[5].at.x=9.6;
 const {output,plan}=await planComponentDsl(board,componentCode(cap('C1')));
 assert.equal(output.status,'complete',JSON.stringify(output.error));
 assert.ok(plan?.attachments);
 const [component]=plan.audit.components;
 assert.notEqual(component.pins[0].owner,component.pins[1].owner);
 assert.notEqual(plan.attachments[0].cut.positive,plan.ports[0].cut.positive);
 assert.notEqual(plan.attachments[0].cut.negative,plan.ports[1].cut.positive);
 const {positive,negative}=plan.attachments[0].cut;
 for(const [index,key] of [positive,negative].entries()){
  const owner:ChannelBlock|undefined=plan.blocks.find(block=>block.id===component.pins[index].owner);
  assert.equal(owner?.kind,'local');
  assert.ok(owner?.cuts.some(cut=>cut.positive===key));
 }
 assert.deepEqual(validateChannelOwnership(plan),[1000,10000]);
 assert.ok(channelTopology(plan).connectionContacts.includes('positive-negative'));
});
