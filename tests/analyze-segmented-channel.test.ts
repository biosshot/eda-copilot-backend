import {mkdirSync,writeFileSync}from'node:fs';
import test from 'node:test';import assert from 'node:assert/strict';
import {solveSegmentedChannel,sectionMagneticBlock,validateChannelOwnership,type SegmentedChannel} from '../src/analyze/segmented-channel.ts';
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
test('source ownership refuses overlaps, gaps and nonlocal coupling split before solving',()=>{for(const delta of [-1,1]){const p=plan();const own=p.blocks[1].ownership[0];assert.ok(own.kind!=='fragment');own.startMm+=delta;assert.throws(()=>validateChannelOwnership(p),/overlap|gap|Uncovered/);}const p=plan();for(const b of p.blocks)b.couplingGroup='shared-field';assert.throws(()=>validateChannelOwnership(p),/jointly/);});

test('section magnetic frame converts mm origin/length but preserves SI outerM geometry',()=>{
 const metal=(role:string,y:number)=>({role,contour:{outerM:[[0,y],[.001,y],[.001,y+.0001],[0,y+.0001]],holesM:[]}});
 const block=sectionMagneticBlock('frame',{kind:'polygon-section',lengthMm:2,geometry:{conductors:[metal('signal',0),metal('reference',.0002)]}},{frequencyDependent:false,capacitanceFPerM:[[1e-12]],modes:[{delaySPerM:1e-6}]},[1000],{originMm:[100,200],direction:[1,0]});
 assert.equal(block.dofs,1);assert.equal(block.directSelf,true);const vertices=block.cells.flatMap((c:any)=>c.verticesM);
 assert.ok(vertices.every((v:number[])=>v[0]>=.099-1e-15&&v[0]<=.101+1e-15&&v[1]>=.2-1e-15&&v[1]<=.201+1e-15),JSON.stringify(vertices[0]));
 assert.ok(vertices.some((v:number[])=>Math.abs(v[1]-.201)<1e-15),'outerM width must not be multiplied by 1e-3');
 for(const cell of block.cells)for(const current of cell.currentsPerAmpM2)assert.ok(Math.abs(Math.abs(current[0])-1e7)<1e-5,'one-amp section DOF requires J=I/conductorArea, without another length divisor');
});

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
