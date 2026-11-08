import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {createAnalysisExecutor} from '../src/analyze/analysis-dsl/executor.ts';
import {terminateAnalysisPool} from '../src/analyze/run-analysis.ts';
import {segmentedCouponDsl} from '../tests/helpers/pcb-analysis-segmented-coupon.ts';

const folder=process.env.P7_OUTPUT_DIR;
assert.ok(folder&&process.env.PCB_ANALYSIS_CACHE_DIR,'supervised output and cache directories are required');
const seconds=Math.min(25,Number(process.env.P7_VERIFY_SECONDS??25));
const memoryMiB=Math.min(512,Number(process.env.P7_VERIFY_MEMORY_MIB??512));
assert.ok(Number.isInteger(seconds)&&seconds>0&&Number.isInteger(memoryMiB)&&memoryMiB>0);
mkdirSync(folder,{recursive:true});
process.env.P7_VERIFY_SECONDS=String(seconds);process.env.P7_VERIFY_MEMORY_MIB=String(memoryMiB);process.env.P7_COMPACT_PLANAR='1';
const base=JSON.parse(readFileSync('tests/fixtures/analysis-segmented-coupon.json','utf8'));
base.outline=[{x:-1.5,y:-1},{x:1.5,y:-1},{x:1.5,y:1},{x:-1.5,y:1}];
base.pads=base.pads.slice(0,4);
for(const [i,pad] of base.pads.entries()){pad.at={x:i%2===0?-.6:.6,y:0};pad.rotationDeg=0;pad.shape={kind:'rect',widthMm:.8,heightMm:.8};}
base.copper.fixed.tracks=['TOP','BOTTOM'].map((layer,i)=>({id:'wide-'+i,net:i?'RETURN':'SIGNAL',layer,widthMm:.8,points:[{x:-.6,y:0},{x:.6,y:0}]}));
base.copper.fixed.vias=[];base.copper.fixed.zones=[];
const rotate=(board:any,degrees:number)=>{
 const a=degrees*Math.PI/180,c=Math.cos(a),s=Math.sin(a),point=(p:any)=>({x:p.x*c-p.y*s,y:p.x*s+p.y*c});
 board.outline=board.outline.map(point);
 for(const component of board.components){component.at=point(component.at);component.rotationDeg+=degrees;}
 for(const pad of board.pads)pad.at=point(pad.at);
 for(const pad of board.pads)pad.rotationDeg=(pad.rotationDeg??0)+degrees;
 for(const track of board.copper.fixed.tracks)track.points=track.points.map(point);
 return board;
};
const code=segmentedCouponDsl.replace('boardThicknessMm:.12','boardThicknessMm:.17').replaceAll('thicknessMm:.01','thicknessMm:.035').replace('startHz:1000,stopHz:10000,points:3','startHz:1000000,stopHz:1000000,points:1').replace('maxSeconds:300,maxMemoryMiB:2048',`maxSeconds:${seconds},maxMemoryMiB:${memoryMiB}`);
const execute=createAnalysisExecutor();
async function run(degrees:number){
 console.error(`P7_ORIENTATION stage=start degrees=${degrees}`);
 let solution:any;
 const board=rotate(structuredClone(base),degrees);
 const r=await executeScenario({board,code},{filename:`hybrid-orientation-${degrees}.js`,maxEntries:1000,maxOutputBytes:8*1024*1024},()=>{},async task=>{const out=await execute(task);if(task.kind==='impedance')solution=out.solution;return out;});
 assert.equal(r.status,'complete',JSON.stringify(r.error));assert.equal(solution.status,'complete');
 assert.equal(solution.coverage,'unverified');assert.equal(solution.provenance.operatorPolicy,'hybrid-planar-v3');
 assert.equal(solution.provenance.partitionPolicy,'automatic-hybrid-v1');
 assert.equal(solution.portObservations.frequencies.length,1);
 const f=solution.frequenciesHz[0],s=solution.parameters[0];
 console.error(`P7_ORIENTATION stage=end degrees=${degrees} prisms=${solution.provenance.prisms} localTetrahedra=${solution.provenance.volumeTetrahedra}`);
 const z=s.map((row:any[])=>row.map((x:any)=>({real:x.real,imaginary:x.imaginary})));
 return {degrees,ports:solution.physicalPorts,roiMm:solution.foilPlan.roiMm,frequencyHz:f,s:z,loss:solution.samples[0].lossGrams,quality:solution.quality,yStatus:'omitted: no validated physical-reference S-to-Y conversion bound to this compact control'};
}
const results=[];
try{
for(const degrees of [0,45,90])results.push(await run(degrees));
const ref=results[0];
for(const value of results.slice(1)){
 assert.deepEqual(value.ports,ref.ports,'physical port identities/order changed under rigid rotation');
 const tolerance=value.degrees===45?.03:.02;
 const close=(a:any,b:any,floor:number)=>Math.hypot(a.real-b.real,a.imaginary-b.imaginary)<=Math.max(floor,tolerance*Math.hypot(b.real,b.imaginary));
 for(const [i,x] of value.s.entries())for(const [j,z] of x.entries())assert.ok(close(z,ref.s[i][j],1e-8),`S orientation sensitivity exceeds ${tolerance*100}% plus floor`);
 for(const [i,row] of value.loss.copperWPerA2.entries())for(const [j,z] of row.entries())assert.ok(close(z,ref.loss.copperWPerA2[i][j],1e-8),`complex loss-Gram sensitivity exceeds ${tolerance*100}% plus floor`);
}
writeFileSync(`${folder}/comparison.json`,JSON.stringify({status:'pass',scope:'rotated copper/contact coupon with automatic domain and mesh orientation sensitivity; finite dielectric domains may differ; physical accuracy unverified',limits:{maxSeconds:seconds,maxMemoryMiB:memoryMiB},results},null,2));
}catch(error){
 writeFileSync(`${folder}/comparison.json`,JSON.stringify({status:'validation-failure',reason:String(error),scope:'automatic domain and mesh orientation development control; physical accuracy unverified',results},null,2));
 throw error;
}finally{await terminateAnalysisPool();}
