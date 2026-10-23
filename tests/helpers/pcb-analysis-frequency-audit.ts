import {readFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {networkResource} from '../../src/analyze/analysis-dsl/frequency-model.ts';
import assert from 'node:assert/strict';
import type {AnalysisDslOutput} from '../../src/analyze/run-analysis-dsl.ts';
export const frequencyCases=(r:AnalysisDslOutput)=>{const ids=new Set(r.reports.flatMap(r=>r.logs).filter((l:any)=>l.message==='network.case').map((l:any)=>l.data.resultId));return r.reports.flatMap(r=>r.calculations).filter((c:any)=>c.phase==='completed'&&ids.has(c.result.id)).map((c:any)=>c.result);};
export const caseNames=['Attenuator Pad','reflecting cascade','cascade reference 75','active nonreciprocal','coupled with explicit load'];
const scalar=(real:number,imaginary=0)=>({real,imaginary});
const a=[[.1,.3],[.4,.2]],b=[[.3,.2],[.5,-.1]],d=1-a[1][1]*b[0][0];
const cascade=[[a[0][0]+a[0][1]*b[0][0]*a[1][0]/d,a[0][1]*b[0][1]/d],[b[1][0]*a[1][0]/d,b[1][1]+b[1][0]*a[1][1]*b[0][1]/d]];
// Independent closed-form 2x2 coupon renormalization. Not a general network solver.
const gamma=-.2,aa=cascade.map((r,i)=>r.map((s,j)=>(i===j?1:0)+gamma*s)),bb=cascade.map((r,i)=>r.map((s,j)=>s+(i===j?gamma:0))),det=aa[0][0]*aa[1][1]-aa[0][1]*aa[1][0];
const normalized=bb.map(r=>[(r[0]*aa[1][1]-r[1]*aa[1][0])/det,(-r[0]*aa[0][1]+r[1]*aa[0][0])/det]);
export function frequencyOracle(name:string,f:number){
 if(name==='Attenuator Pad')return a.map(r=>r.map(v=>scalar(v)));
 if(name==='reflecting cascade')return cascade.map(r=>r.map(v=>scalar(v)));
 if(name==='cascade reference 75')return normalized.map(r=>r.map(v=>scalar(v)));
 if(name==='active nonreciprocal'){const t=f/1e9;return [[scalar(.1),scalar(.01)],[scalar(t<=2?1.5+.5*(t-1):2-.5*(t-2),.5*(t-2)),scalar(.2)]];}
 if(name==='coupled with explicit load'){const g=1/3,den=1-.1*g;return [[scalar(.1+.3*g*.3/den),scalar(.2+.3*g*.2/den)],[scalar(.2+.2*g*.3/den),scalar(.1+.2*g*.2/den)]];}
 throw Error('Unexpected network case '+name);
}
const close=(a:number,b:number)=>assert.ok(Number.isFinite(a)&&Math.abs(a-b)<=1e-10+1e-8*Math.abs(b),`${a} != ${b}`);
export function auditFrequencyReport(result:AnalysisDslOutput,expected?:any){
 const supplied=JSON.parse(readFileSync(new URL('../fixtures/analysis-frequency/resources.json',import.meta.url),'utf8')).electrical;if(expected)supplied.attenuatorPad.data.content=readFileSync(new URL('../fixtures/analysis-frequency/attenuator-pad.s2p',import.meta.url),'utf8');const expectedResources=new Map(Object.values(supplied).map((m:any)=>[m.id,m]));const modelHash=(m:unknown)=>createHash('sha256').update(JSON.stringify(networkResource(m))).digest('hex');
 assert.equal(result.status,'complete',JSON.stringify(result.error));const cases=frequencyCases(result),logs=result.reports.flatMap(r=>r.logs).filter((l:any)=>l.message==='network.case') as any[];
 assert.equal(cases.length,5);assert.deepEqual(logs.map(l=>l.data.name),caseNames);if(expected)assert.deepEqual(caseNames,expected.cases);
 const allPlots=result.reports.flatMap(r=>r.plots)as any[],allChecks=result.reports.flatMap(r=>r.checks),plots=allPlots.filter(p=>!p.title.startsWith('board section:')),checks=allChecks.filter(c=>c.name!=='board section mutual impedance hypothesis');assert.equal(allPlots.length,expected?84:80);assert.equal(allChecks.length,expected?11:10);assert.equal(plots.length,80);assert.equal(checks.length,10);
 let maxError=0;
 for(let k=0;k<5;k++){
  const s=cases[k].solution,name=caseNames[k];assert.equal(logs[k].data.resultId,cases[k].id);assert.equal(s.status,'complete');assert.equal(s.representation,'S');assert.deepEqual(s.ports.map((p:any)=>p.name),['input','output']);assert.deepEqual(s.frequenciesHz,Array.from({length:9},(_,i)=>1e9+i*2e9/8));assert.deepEqual(s.referenceOhm,[k===2?75:50,k===2?75:50]);assert.equal(s.nature,k===3?'active':'passive');assert.equal(s.conditional,true);assert.ok(s.quality.maxResidual<=1e-8&&s.quality.maxCondition<=1e12);assert.ok(s.resources.peakRssBytes>0);
  assert.equal(s.provenance.length,[1,2,2,1,2][k]);assert.equal(s.sourceHashes.length,s.provenance.length);
  for(const [pidx,p]of s.provenance.entries()){
   const ids=[['synthetic-reflecting-attenuator-pad'],['synthetic-reflecting-attenuator-pad','synthetic-second-attenuator'],['synthetic-reflecting-attenuator-pad','synthetic-second-attenuator'],['synthetic-nonreciprocal-amplifier'],['synthetic-coupled-three-port','explicit-100-ohm-load']][k];assert.equal(p.resource.resourceId,ids[pidx]);assert.equal(s.sourceHashes[pidx].resourceId,ids[pidx]);assert.equal(s.sourceHashes[pidx].sha256,modelHash(expectedResources.get(ids[pidx])));assert.equal(p.resource.kind,'network');assert.equal(p.resource.source,(expectedResources.get(ids[pidx]) as any).source);assert.deepEqual(p.mapping,pidx===1&&k===4?['termination']:k===4&&pidx===0?['input','output','load']:['input','output']);
   const ports=pidx===1&&k===4?1:k===4&&pidx===0?3:2;assert.deepEqual(p.sourcePorts,ports===1?['p']:ports===3?['p1','p2','p3']:['p1','p2']);assert.equal(p.nature,k===3?'active':'passive');assert.equal(p.sourceRepresentation,pidx===1&&k===4?'Z':'S');assert.deepEqual(p.sourceReferenceOhm,pidx===1&&k===4?null:Array(ports).fill(50));
   const frequencies=p.resource.resourceId==='synthetic-second-attenuator'?[1e9,1.5e9,3e9]:[1e9,2e9,3e9];assert.deepEqual(p.normalized.frequenciesHz,frequencies);assert.equal(p.normalized.parameters.length,3);
   for(let t=0;t<3;t++){
    const expectedMatrix=p.resource.resourceId==='synthetic-reflecting-attenuator-pad'?a.map(r=>r.map(v=>scalar(v))):p.resource.resourceId==='synthetic-second-attenuator'?b.map(r=>r.map(v=>scalar(v))):p.resource.resourceId==='synthetic-nonreciprocal-amplifier'?frequencyOracle('active nonreciprocal',frequencies[t]):p.resource.resourceId==='explicit-100-ohm-load'?[[scalar(100)]]:[[.1,.2,.3],[.2,.1,.2],[.3,.2,.1]].map(r=>r.map(v=>scalar(v)));
    assert.deepEqual(p.normalized.parameters[t],expectedMatrix);
   }
  }
  const expectedIntervals=s.provenance.flatMap((p:any)=>s.frequenciesHz.filter((f:number)=>!p.normalized.frequenciesHz.includes(f)).map((f:number)=>({id:p.instanceId,f})));assert.equal(s.interpolation.length,expectedIntervals.length);for(const e of expectedIntervals)assert.equal(s.interpolation.filter((v:any)=>v.instanceId===e.id&&v.frequencyHz===e.f).length,1);
  for(const entry of s.interpolation){const p=s.provenance.find((p:any)=>p.instanceId===entry.instanceId);assert.ok(p);assert.equal(entry.resourceId,p.resource.resourceId);assert.equal(entry.representation,p.sourceRepresentation);assert.ok(entry.frequencyHz>entry.intervalHz[0]&&entry.frequencyHz<entry.intervalHz[1]);assert.ok(p.normalized.frequenciesHz.includes(entry.intervalHz[0])&&p.normalized.frequenciesHz.includes(entry.intervalHz[1]));}
  assert.equal(s.parameters.length,9);
  for(let t=0;t<9;t++){const expected=frequencyOracle(name,s.frequenciesHz[t]);assert.equal(s.parameters[t].length,2);for(let i=0;i<2;i++){assert.equal(s.parameters[t][i].length,2);for(let j=0;j<2;j++){const z=s.parameters[t][i][j],ref=expected[i][j];close(z.real,ref.real);close(z.imaginary,ref.imaginary);maxError=Math.max(maxError,Math.hypot(z.real-ref.real,z.imaginary-ref.imaginary));}}}
  for(let i=0;i<2;i++)for(let j=0;j<2;j++)for(const part of ['magnitude','phase','real','imaginary']){
   const title=name+':'+['input','output'][i]+'<-'+['input','output'][j]+':'+part;const found=plots.filter(p=>p.title===title);assert.equal(found.length,1,title);const plot=found[0].data;assert.equal(plot.kind,'series');assert.equal(plot.unit,part==='phase'?'deg':'1');assert.equal(plot.validity.status,'conditional');assert.deepEqual(plot.axis,{name:'frequencyHz',unit:'Hz',values:s.frequenciesHz});assert.equal(plot.values.length,9);for(let t=0;t<9;t++){const z=s.parameters[t][i][j];close(plot.values[t],part==='real'?z.real:part==='imaginary'?z.imaginary:part==='magnitude'?Math.hypot(z.real,z.imaginary):Math.atan2(z.imaginary,z.real)*180/Math.PI);}
  }
  assert.equal(checks[2*k].name,name+': expected fail');assert.equal(checks[2*k].status,'fail');assert.equal(checks[2*k+1].name,name+': continued');assert.equal(checks[2*k+1].status,'pass');
 }
 if(expected){const binding=(result.reports.flatMap(r=>r.logs).find((l:any)=>l.message==='network.binding')as any)?.data;assert.deepEqual(binding,expected.binding);
 const label=(result.reports.flatMap(r=>r.logs).find((l:any)=>l.message==='board.network.case')as any)?.data;assert.ok(label);const section=result.reports.flatMap(r=>r.calculations).find((c:any)=>c.phase==='completed'&&c.result.id===label.resultId)as any;assert.ok(section);const s=section.result.solution;assert.equal(s.representation,'Z');assert.equal(s.referenceOhm,null);assert.equal(s.nature,'passive');assert.equal(label.resourceId,expected.boardSection.resourceId);assert.equal(s.provenance[0].resource.resourceId,expected.boardSection.resourceId);const sectionModel=JSON.parse(readFileSync(new URL('../pcb-analyze/'+expected.fixture+'/network.resources.json',import.meta.url),'utf8')).electrical.boardSection;assert.equal(s.sourceHashes[0].sha256,modelHash(sectionModel));assert.deepEqual(s.ports.map((p:any)=>p.name),['input','output']);const f=expected.boardSection.frequenciesHz;assert.deepEqual(s.provenance[0].normalized.frequenciesHz,f);assert.deepEqual(s.frequenciesHz,Array.from({length:9},(_,i)=>f[0]+i*(f.at(-1)-f[0])/8));for(const matrix of s.parameters)for(let i=0;i<2;i++)for(let j=0;j<2;j++){close(matrix[i][j].real,expected.boardSection.matrixOhm[i][j]);close(matrix[i][j].imaginary,0);}
 for(const part of ['magnitude','phaseDeg','real','imaginary']){const found=allPlots.filter(p=>p.title==='board section:'+part);assert.equal(found.length,1);assert.deepEqual(found[0].data.axis.values,s.frequenciesHz);assert.equal(found[0].data.unit,part==='phaseDeg'?'deg':'Ohm');for(const v of found[0].data.values)close(v,part==='phaseDeg'||part==='imaginary'?0:expected.boardSection.matrixOhm[1][0]);}assert.equal(allChecks.at(-1)?.status,'pass');
 }
 return {cases:expected?6:5,rfCases:5,boardSectionCases:expected?1:0,plots:expected?84:80,expectedFails:5,followingPasses:expected?6:5,maxComplexAbsoluteError:maxError,tolerance:'1e-8 relative + 1e-10 dimensionless absolute',status:'pass'};
}
