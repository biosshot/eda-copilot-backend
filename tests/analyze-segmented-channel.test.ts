import {mkdirSync,writeFileSync}from'node:fs';
import test from 'node:test';import assert from 'node:assert/strict';
import {solveSegmentedChannel,validateChannelOwnership,type SegmentedChannel} from '../src/analyze/segmented-channel.ts';
import {createProblemExecutor} from '../src/analyze/execute-problem.ts';
const record=(name:string,data:unknown)=>{const dir=process.env.P7_EVIDENCE_DIR;if(dir){mkdirSync(dir,{recursive:true});writeFileSync(dir+'/'+name+'.json',JSON.stringify(data,null,2));}};
const line=(lengthMm:number)=>({kind:'microstrip',widthMm:1,copperThicknessMm:.035,dielectricHeightMm:1,lengthMm,material:{relativePermittivity:4,lossTangent:0,state:'Explicit ideal numerical coupon'}});
const cut=(positive:string)=>({positive,negative:positive+'-return'});
function plan():SegmentedChannel{return {blocks:[{id:'left',kind:'section',line:line(10),bandHz:[1000,100000],cuts:[cut('in'),cut('join')],ownership:[{sourcePath:'strip',startMm:0,stopMm:10}]},{id:'right',kind:'section',line:line(10),bandHz:[1000,100000],cuts:[cut('join'),cut('out')],ownership:[{sourcePath:'strip',startMm:10,stopMm:20}]}],ports:[{name:'in',cut:cut('in')},{name:'out',cut:cut('out')}],coverage:[{sourcePath:'strip',startMm:0,stopMm:20}]};}
test('actual FEM sections compose through physical P6 terminals and match unpartitioned propagation',()=>{
 const frequencies=[1000,10000,100000];const r=solveSegmentedChannel(plan(),frequencies);const reference=createProblemExecutor()({kind:'cross-section',request:{line:line(20),frequencies}});
 assert.equal(r.result.status,'complete',JSON.stringify(r.result.diagnostics));assert.equal(reference.status,'complete');
 let error=0;for(let f=0;f<3;f++)for(let i=0;i<2;i++)for(let j=0;j<2;j++){const a=r.result.parameters![f][i][j],b=reference.network!.parameters[f][i][j];error=Math.max(error,Math.hypot(a.real-b.real,a.imaginary-b.imaginary));}assert.ok(error<1e-7,String(error));assert.equal(r.evidence.length,2);record('uniform-composition',{maxAbsoluteSError:error,composed:r.result.parameters,reference:reference.network!.parameters,frequencies,evidence:r.evidence,wallSeconds:r.wallSeconds});
});
test('source ownership refuses overlaps, gaps and nonlocal coupling split before solving',()=>{for(const delta of [-1,1]){const p=plan();p.blocks[1].ownership[0].startMm+=delta;assert.throws(()=>validateChannelOwnership(p),/overlap|gap|Uncovered/);}const p=plan();for(const b of p.blocks)b.couplingGroup='shared-field';assert.throws(()=>validateChannelOwnership(p),/jointly/);});

test('branched numerical sections retain the third port and match independent star-line modes',()=>{
 const p=plan();p.blocks=[];p.coverage=[];p.ports=[];
 for(let i=0;i<3;i++){const name='arm'+i;p.blocks.push({id:name,kind:'section',line:line(10),bandHz:[1000,100000],cuts:[cut(name),cut('join')],ownership:[{sourcePath:name,startMm:0,stopMm:10}]});p.coverage.push({sourcePath:name,startMm:0,stopMm:10});p.ports.push({name,cut:cut(name)});}
 const f=100000,r=solveSegmentedChannel(p,[f]);const q:any=(r.evidence[0]as any).parameters;const z=q.modes[0].impedanceOhm,theta=2*Math.PI*f*.01*q.modes[0].delaySPerM;
 const reflection=(b:number)=>({real:(b*b-2500)/(b*b+2500),imaginary:100*b/(b*b+2500)});const open=reflection(-z/Math.tan(theta)),short=reflection(z*Math.tan(theta));
 for(let i=0;i<3;i++)for(let j=0;j<3;j++){const a=r.result.parameters![0][i][j],sign=i===j?2:-1;assert.ok(Math.hypot(a.real-(open.real+sign*short.real)/3,a.imaginary-(open.imaginary+sign*short.imaginary)/3)<1e-7);}
});

test('coupled numerical sections compose full matrices with finite physical return cuts',()=>{
 const q=(l:number)=>({...line(l),kind:'coupled-microstrip',gapMm:1});const frequencies=[1e6,1e7,1e8],p:SegmentedChannel={blocks:[],coverage:[{sourcePath:'pair',startMm:0,stopMm:20}],ports:[]};
 const c=(s:string,i:number)=>({positive:s+'-'+i,negative:s+'-return'});
 for(let k=0;k<2;k++)p.blocks.push({id:'pair-'+k,kind:'section',line:q(10),bandHz:[1e6,1e8],cuts:[c('end'+k,0),c('end'+k,1),c('end'+(k+1),0),c('end'+(k+1),1)],ownership:[{sourcePath:'pair',startMm:k*10,stopMm:(k+1)*10}]});
 for(const s of ['end0','end2'])for(let i=0;i<2;i++)p.ports.push({name:s+'-'+i,cut:c(s,i)});
 const r=solveSegmentedChannel(p,frequencies),reference=createProblemExecutor()({kind:'cross-section',request:{line:q(20),frequencies}});assert.equal(reference.status,'complete');
 let error=0;for(let f=0;f<3;f++)for(let i=0;i<4;i++)for(let j=0;j<4;j++){const a=r.result.parameters![f][i][j],b=reference.network!.parameters[f][i][j];error=Math.max(error,Math.hypot(a.real-b.real,a.imaginary-b.imaginary));}assert.ok(error<2e-6,String(error));record('coupled-composition',{maxAbsoluteSError:error,composed:r.result.parameters,reference:reference.network!.parameters,frequencies,evidence:r.evidence,wallSeconds:r.wallSeconds});
});

test('existing no-scope DSL automatically executes section and local FEM with copper and return ownership', {timeout:620000},async()=>{
 const {runAnalysisDsl}=await import('../src/analyze/run-analysis-dsl.ts');const {segmentedCoupon,segmentedCouponDsl}=await import('./helpers/pcb-analysis-segmented-coupon.ts');
 const previous=process.env.P7_VERIFY_SECONDS;process.env.P7_VERIFY_SECONDS='600';const memory=Number(process.env.P7_VERIFY_MEMORY_MIB??2048);
 try{const code=segmentedCouponDsl.replace('maxSeconds:300,maxMemoryMiB:2048',`maxSeconds:600,maxMemoryMiB:${memory}`);const output=await runAnalysisDsl({board:segmentedCoupon(),code},{filename:'automatic-segmented-regression.js',timeoutMs:610000,maxEntries:1000,maxOutputBytes:64*1024*1024});assert.equal(output.status,'complete',JSON.stringify(output.error));
  const result=(output as any).reports.flatMap((r:any)=>r.calculations).find((c:any)=>c.phase==='completed').result,solution=result.solution;
  assert.deepEqual(solution.blocks.map((b:any)=>b.kind),['local','section','local']);assert.equal(solution.result.assembledModel.referenceTopology,'infinity');
  assert.notEqual(solution.result.assembledModel.portTerminals[0].negative.node,solution.result.assembledModel.portTerminals[1].negative.node,'Finite return path must not become an ideal shared ground');
  assert.equal(solution.evidence.filter((e:any)=>e.kind==='local'&&e.stageCounts.p6Solves>0).length,2);assert.ok(solution.verification.every((v:any)=>v.spatial.maxRelativeChange<=.02));assert.ok(solution.verification.some((v:any)=>v.domain?.maxRelativeChange<=.02));
  const wave=solution.result.parameters[0][1][0],den=wave.real**2+wave.imaginary**2,resistance=100*(wave.real/den-1),reference=.0098/58e6*(1/(.0001*.00001)+1/(.0005*.00001));assert.ok(Math.abs(resistance-reference)/reference<=.05,JSON.stringify({resistance,reference}));
  assert.equal(solution.validity.domain.temporal,'not admitted');record('automatic-mixed-coupon',{solution,resistanceOhm:resistance,dcReferenceOhm:reference,errorRelative:Math.abs(resistance-reference)/reference});
 }finally{if(previous===undefined)delete process.env.P7_VERIFY_SECONDS;else process.env.P7_VERIFY_SECONDS=previous;}
});
