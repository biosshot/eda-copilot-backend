import test from 'node:test';
import assert from 'node:assert/strict';
import {autoFoilPlan} from '../src/analyze/auto-foil-plan.ts';

test('oriented foil closure rotates complete islands and barrel envelope',()=>{
 const outputs=[];
 for(const degrees of [0,45,90]){
  const a=degrees*Math.PI/180,c=Math.cos(a),s=Math.sin(a);
  const rotate=([x,y]:number[]):[number,number]=>[x*c-y*s,x*s+y*c];
  const rect=(x:number,y:number,w:number,h:number)=>[[x,y],[x+w,y],[x+w,y+h],[x,y+h]].map(rotate);
  const region=(id:string,box:[number,number,number,number])=>({id,polygon:{outer:rect(...box)},source:{path:id},layer:'TOP'});
  const regions=[region('seed',[0,0,2,.2]),region('near',[2.25,0,.2,.2]),region('remote',[4,0,.2,.2]),region('far',[8,0,.2,.2])];
  const captured={status:'complete',excluded:[],fills:[],boolean_error_bound_mm:1e-9,regions,
   islands:[{region_ids:['seed']},{region_ids:['near','remote']},{region_ids:['far']}],
   holes:[{id:'via',plated:true,span_confirmed:true,polygons:[{outer:rect(2.1,.25,.05,.05)}],source:{path:'vias[0]'},layers:['TOP','BOTTOM']}]};
  const geometry={stack:[{name:'TOP',copper:true},{name:'core',copper:false},{name:'BOTTOM',copper:true}],maxTriangleAreaM2:1e-6,finishedHolePlatingM:.000025,barrelConductivitySM:58e6};
  const terminal={id:'a',sourcePath:'seed',layer:'TOP'},other={id:'b',sourcePath:'near',layer:'TOP'};
  const p=autoFoilPlan({boardJson:'{}',geometry,captured:captured as any,terminals:[terminal,other],ports:[{name:'P',positive:'a',negative:'b'}],components:[],seedRegions:new Set(['seed']),marginMm:.35,deadlineMs:performance.now()+1000,direction:[c,s]})!;
  assert.deepEqual(p.sourceCopper.map(v=>v.regionId),['seed','near','remote']);
  assert.deepEqual(p.sourceBarrels.map(v=>v.barrelId),['via']);
  assert.equal(p.domain.policy,'oriented-run-v1');assert.deepEqual(p.geometry.domainPolygonMm,p.domain.polygonMm);
  outputs.push(p.domain.polygonMm!.map(([x,y])=>[x*c+y*s,-x*s+y*c]));
 }
 for(const polygon of outputs.slice(1))for(let i=0;i<4;i++)for(let j=0;j<2;j++)assert.ok(Math.abs(polygon[i][j]-outputs[0][i][j])<1e-12);
});

test('foil closure leaves authored polygon scopes untouched',()=>{
 const geometry={stack:[],maxTriangleAreaM2:1,domainPolygonMm:[[0,0],[1,0],[0,1]]};
 const result=autoFoilPlan({boardJson:'{}',geometry,captured:{} as any,terminals:[],ports:[],components:[],seedRegions:new Set(),marginMm:1,deadlineMs:performance.now()+1000,direction:[1,0]});
 assert.equal(result,undefined);assert.deepEqual(geometry.domainPolygonMm,[[0,0],[1,0],[0,1]]);
});

test('hybrid planner selects the physical run frame at each rotation',async()=>{
 const {readFileSync}=await import('node:fs');
 const {boardChannelPlan}=await import('../src/analyze/board-channel-plan.ts');
 const cases=JSON.parse(readFileSync('tests/fixtures/analysis-local-extraction/orientation-aligned-pads.json','utf8'));
 const old=process.env.P7_COMPACT_PLANAR;process.env.P7_COMPACT_PLANAR='1';
 try {
  const domains=[];
  for(const [i,item] of cases.entries()){
   const board=JSON.parse(item[0]),geometry=item[1];
   const contacts=[{sourcePath:'pads[0]',layer:'TOP'},{sourcePath:'pads[1]',layer:'TOP'},{sourcePath:'pads[2]',layer:'BOTTOM'},{sourcePath:'pads[3]',layer:'BOTTOM'}];
   const plan=boardChannelPlan(board,geometry,contacts,{closedJointDeadlineMs:performance.now()+5000});
   assert.equal(plan.domain.policy,'oriented-run-v1');
   const a=i*Math.PI/4,c=Math.cos(a),s=Math.sin(a);
   domains.push(plan.domain.polygonMm!.map(([x,y])=>[x*c+y*s,-x*s+y*c]));
  }
  for(const poly of domains.slice(1))for(let k=0;k<4;k++)for(let j=0;j<2;j++)assert.ok(Math.abs(poly[k][j]-domains[0][k][j])<1e-6);
 } finally {if(old===undefined)delete process.env.P7_COMPACT_PLANAR;else process.env.P7_COMPACT_PLANAR=old;}
});

test('branched physical runs report an axis-aligned fallback',async()=>{
 const {readFileSync}=await import('node:fs');
 const {boardChannelPlan}=await import('../src/analyze/board-channel-plan.ts');
 const [item]=JSON.parse(readFileSync('tests/fixtures/analysis-local-extraction/orientation-aligned-pads.json','utf8'));
 const board=JSON.parse(item[0]),geometry=item[1];
 board.copper.fixed.tracks[0].points.push({x:.6,y:.5});
 const contacts=[{sourcePath:'pads[0]',layer:'TOP'},{sourcePath:'pads[1]',layer:'TOP'},{sourcePath:'pads[2]',layer:'BOTTOM'},{sourcePath:'pads[3]',layer:'BOTTOM'}];
 const old=process.env.P7_COMPACT_PLANAR;process.env.P7_COMPACT_PLANAR='1';
 try {
  const plan=boardChannelPlan(board,geometry,contacts,{closedJointDeadlineMs:performance.now()+5000});
  assert.equal(plan.domain.policy,'axis-aligned-fallback-v1');
  assert.equal(plan.domain.reason,'multiple non-collinear physical runs');
  assert.equal(plan.geometry.domainPolygonMm,undefined);
 } finally {if(old===undefined)delete process.env.P7_COMPACT_PLANAR;else process.env.P7_COMPACT_PLANAR=old;}
});
