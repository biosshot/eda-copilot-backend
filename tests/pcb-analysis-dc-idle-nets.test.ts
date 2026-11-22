import test from 'node:test';
import assert from 'node:assert/strict';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
import {thermalLossTransfer} from '../src/analyze/analysis-dsl/thermal-loss-transfer.ts';
import {dcCoupon,couponSetup} from './helpers/pcb-analysis-dc-coupon.ts';

function board(){
  const b=dcCoupon();
  b.nets.push({name:'IDLE'});
  b.pads.push({id:'idle-pad',number:'1',net:'IDLE',at:{x:5,y:7.2},rotationDeg:0,layers:['TOP'],shape:{kind:'rect',widthMm:4,heightMm:.6}} as any);
  return b;
}
async function run(idle:string){
  const chain=idle?`.idleNets(${idle})`:'';
  const result=await runAnalysisDsl({board:board(),code:`${couponSetup.replace(/;$/,'')}${chain}; const dc=await base.loads([{port:load,currentA:1}]).run();a.check({name:'ran',actual:dc.lossMap.min(),min:0,unit:'W'});`},{timeoutMs:120000,maxOutputBytes:32*1024*1024});
  return result as any;
}
const dcSolution=(r:any)=>r.reports[0].calculations.filter((c:any)=>c.phase==='completed')[0].result;

test('idleNets puts idle copper into the loss map as zero loss with conditional validity',async()=>{
  const r=await run("[net('IDLE')]");assert.equal(r.status,'complete',JSON.stringify(r.error));
  const res=dcSolution(r),sol=res.solution;
  assert.equal(sol.idle_domains.length,1);assert.deepEqual(sol.idle_domains[0].nets,['IDLE']);
  assert.ok(sol.idle_domains[0].loss_w.length>0&&sol.idle_domains[0].loss_w.every((v:number)=>v===0));
  const mesh=sol.mesh.domains.find((d:any)=>d.id===sol.idle_domains[0].domain_id);assert.equal(mesh.triangles.length,sol.idle_domains[0].loss_w.length);
  const m=res.lossMap;assert.equal(m.validity.status,'conditional');assert.match(m.validity.reason,/zero DC current asserted by the scenario for nets IDLE/);
  assert.equal(m.values.length,sol.domains.reduce((n:number,d:any)=>n+d.loss_w.length,0)+sol.idle_domains[0].loss_w.length);
  assert.deepEqual(m.data.excludedDomains,[]);assert.equal(m.data.slices.at(-1).domainId,sol.idle_domains[0].domain_id);
  assert.equal(res.voltageMap.data.excludedDomains.length,1,'voltage map still does not cover the idle domain');
});

test('without idleNets the excluded domain stays excluded and the loss map has no idle entries',async()=>{
  const r=await run('');assert.equal(r.status,'complete',JSON.stringify(r.error));
  const res=dcSolution(r),sol=res.solution;
  assert.equal(sol.idle_domains,undefined);assert.equal(sol.excluded_domains.length,1);assert.equal(res.lossMap.data.excludedDomains.length,1);assert.equal(res.lossMap.data.idleDomains,undefined);
});

test('idleNets refuses a net reached by declared sources/loads and a net absent from the board',async()=>{
  const carry=await run("[net('VCC')]");assert.equal(carry.status,'failed');assert.equal(carry.error.code,'DC_IDLE_NET_CARRIES_CURRENT');
  const unknown=await run("[net('NOPE')]");assert.equal(unknown.status,'failed');assert.match(unknown.error.code,/DC_IDLE_NET_UNKNOWN|INVALID_CONFIGURATION/);assert.match(unknown.error.message,/NOPE/);
  const nonNet=await run("[pad('J1','1')]");assert.equal(nonNet.status,'failed');
});

test('thermal loss transfer accepts scenario-asserted idle domains as explicit zero heat and stays conditional',()=>{
  const source:any={id:'dc1',boardIdentity:'board1',status:'complete',conditional:false,domains:[{domain_id:'d',layer:'TOP',loss_w:[1,0]}],
    idle_domains:[{domain_id:'i',layer:'TOP',nets:['IDLE'],loss_w:[0]}],excluded_domains:['i'],copper_loss_w:1,barrel_loss_w:0,
    mesh:{domains:[{id:'d',layer:'TOP',nodes:[[0,0],[1,0],[1,1],[0,1]],triangles:[[0,1,2],[0,2,3]]},{id:'i',layer:'TOP',nodes:[[3,0],[4,0],[4,1]],triangles:[[0,1,2]]}]}};
  const task:any={contextId:'context',boardIdentity:'board1',config:{losses:{contextId:'context',kind:'map',unit:'W',validity:{status:'conditional'},values:[1,0,0],data:{solutionId:'dc1',field:'loss_w',association:'triangle',slices:[{domainId:'d',start:0,count:2},{domainId:'i',start:2,count:1}]}}}};
  const t=thermalLossTransfer(task,()=>source)!;
  assert.equal(t.heat.length,1);assert.equal((t.provenance as any).zeroPowerEntriesSkipped,2);assert.equal(t.conditional,true);assert.match(t.conditionalReason!,/nets IDLE/);
  const valid=structuredClone(task);valid.config.losses.validity.status='valid';assert.throws(()=>thermalLossTransfer(valid,()=>source),/relabelled valid/);
  const nonzero=structuredClone(source);nonzero.idle_domains[0].loss_w=[1e-3];assert.throws(()=>thermalLossTransfer(task,()=>nonzero),/idle domain/i);
  const other=structuredClone(source);other.excluded_domains=['i','x'];assert.throws(()=>thermalLossTransfer(task,()=>other),/uncomputed/);
});
