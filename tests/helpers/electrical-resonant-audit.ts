import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {resonantResource,resonantPhysicalAc,resonantPhysicalTime} from './electrical-resonant-model.ts';
const finite=(v:any)=>{assert.equal(typeof v,'number');assert.ok(Number.isFinite(v));return v as number;};
const near=(got:any,want:number,tolerance:number)=>assert.ok(Math.abs(finite(got)-want)<=tolerance,`${got} != ${want}, tolerance ${tolerance}`);
export function auditResonantRuntime(result:any){
 assert.equal(result.status,'complete',JSON.stringify(result.error));
 const logs=result.reports.flatMap((r:any)=>r.logs),plots=result.reports.flatMap((r:any)=>r.plots);
 const cases=logs.filter((l:any)=>l.message==='resonant.case');assert.deepEqual(cases.map((l:any)=>l.data.name),['coarse','fine','ac']);
 const completed=result.reports.flatMap((r:any)=>r.calculations).filter((c:any)=>c.phase==='completed');assert.equal(completed.length,3);
 const get=(name:string)=>{const id=cases.find((l:any)=>l.data.name===name).data.resultId;return completed.find((c:any)=>c.result.id===id).result;};
 const negatives=logs.filter((l:any)=>l.message==='resonant.negative');assert.deepEqual(negatives.map((l:any)=>l.data.kind),['U','closure','shape','NaN','active','conditioning','grid','source','Z','class']);
 for(const n of negatives)assert.match(n.data.error,/RESONANT_|TEMPORAL_|NETWORK_|COUPLED_|invalid|finite|closure|resonant|JSON|schema/i);
 for(const [kind,code] of [['U','RESONANT_MODAL'],['active','TEMPORAL_PASSIVITY'],['conditioning','RESONANT_CONDITION'],['grid','RESONANT_RESOLUTION'],['source','TEMPORAL_SOURCE_BAND']])assert.match(negatives.find((n:any)=>n.data.kind===kind).data.error,new RegExp(code));
 const source=resonantResource(),sha=createHash('sha256').update(JSON.stringify(source)).digest('hex');
 const report:Record<string,any>={};assert.equal(plots.length,9);
 for(const name of ['coarse','fine']){
  const calculation=get(name),s=calculation.solution,fit=s.fit.sourceFits[0];assert.equal(s.status,'complete');assert.equal(s.conditional,true);assert.deepEqual(s.bandHz,[0,1e8]);
  assert.equal(fit.method,'passive-resonant-y');assert.equal(fit.validity.status,'conditional');assert.match(fit.validity.reason,/intersample/);
  assert.equal(fit.orderPerMode,2);assert.equal(fit.coefficients.length,2);
  for(let i=0;i<2;i++){
   const c=fit.coefficients[i];for(const [key,want] of [['resistanceOhm',i===0?50:70],['inductanceH',i===0?1e-5:2.5e-5],['capacitanceF',i===0?1e-9:2.5e-10]] as const)near(c[key],want,Math.abs(want)*1e-9);
   assert.ok(c.conditionUpperBound<=1e10);assert.ok(c.trainingSamplesPerLinewidth>=8);
  }
  assert.ok(fit.trainingMaximum<=.005&&fit.holdoutMaximum<=.005);
  assert.equal(s.sourceResources.length,1);assert.equal(s.sourceResources[0].sha256,sha);assert.equal(s.sourceResources[0].resource.sha256,sha);
  assert.deepEqual(s.sourceResources[0].allowedAnalyses,source.analyses);assert.deepEqual(s.sourceResources[0].sourceValidity,source.validity);
  assert.equal(fit.frequenciesHz.length,1026);
  let matrixError=0;
  for(let k=0;k<fit.frequenciesHz.length;k++){
   const ref=resonantPhysicalAc(finite(fit.frequenciesHz[k]));
   for(let i=0;i<2;i++)for(let j=0;j<2;j++){
    const z=fit.fittedAdmittanceS[k][i][j],sourceZ=fit.sourceAdmittanceS[k][i][j];
    const diff=50*Math.hypot(finite(z.real)-ref[i][j][0],finite(z.imaginary)-ref[i][j][1]);
    assert.ok(diff<=.005*50*Math.hypot(...ref[i][j])+1e-8);matrixError=Math.max(matrixError,diff);
    near(sourceZ.real,ref[i][j][0],1e-14);near(sourceZ.imaginary,ref[i][j][1],1e-14);
   }
  }
  assert.equal(s.axis.length,2001);let currentError=0,voltageError=0,energyError=0;
  for(let k=0;k<s.axis.length;k++){
   const t=finite(s.axis[k]);near(t,k*1e-8,1e-18);if(k)assert.ok(t>s.axis[k-1]);
   const ref=resonantPhysicalTime(t);assert.equal(s.driverCurrentsA[k].length,2);assert.equal(s.portVoltages[k].length,2);
   for(let i=0;i<2;i++){
    const ie=Math.abs(finite(s.driverCurrentsA[k][i])-ref.current[i]);currentError=Math.max(currentError,ie);assert.ok(ie<=.01*.02+1e-12);
    const ve=Math.abs(finite(s.portVoltages[k][i])-ref.voltage[i]);voltageError=Math.max(voltageError,ve);assert.ok(ve<=25*.01*.02+1e-12);
    const drive=(i===0?1:.4)*Math.min(1,Math.max(0,(t-(i===0?0:1.5e-7))/2e-7));near(s.driverCurrentsA[k][i],(drive-s.portVoltages[k][i])/25,1e-12);
   }
   const ee=Math.abs(finite(s.energyJ[k])-ref.energy);energyError=Math.max(energyError,ee);assert.ok(ee<=.01*2e-9+1e-20);assert.ok(s.energyJ[k]>=0);
  }
  const q=s.quality;assert.ok(q.relativeResidual<=1e-8);assert.ok(q.normalizedEnergyBalance<=1e-4);
  const norm=finite(q.energyNormalizationJ),balance=finite(q.energyBalanceJ);assert.ok(Math.abs(balance)<=1e-14+1e-7*norm);assert.ok(Math.abs(balance)<=(norm===0?1e-20:1e-4*norm));
  assert.ok(q.dissipatedJ>=0&&q.beNumericalDissipationJ>=0);
  near(q.rawEnergyBalanceJ,q.workJ-q.dissipatedJ-(s.energyJ.at(-1)-s.energyJ[0]),1e-20);
  near(q.energyBalanceJ,q.rawEnergyBalanceJ-q.beNumericalDissipationJ,1e-20);
  for(let i=0;i<2;i++){
   const plot=plots.find((p:any)=>p.title===`${name}:${i===0?'left':'right'}`).data;
   assert.equal(plot.validity.status,'conditional');assert.deepEqual(plot.validity.domain.bandHz,[0,1e8]);assert.deepEqual(plot.validity.domain.sourceResourceHashes,[sha]);
   assert.deepEqual(plot.axis.values,s.axis);assert.deepEqual(plot.values,s.portVoltages.map((v:number[])=>v[i]));assert.equal(plot.unit,'V');
  }
  report[name]={currentErrorA:currentError,voltageErrorV:voltageError,energyErrorJ:energyError,matrixAbsoluteNormalizedError:matrixError,quality:q,resources:s.resources,counters:s.counters};
 }
 const coarse=get('coarse').solution,fine=get('fine').solution;let refinement=0;
 const derivative=plots.find((p:any)=>p.title==='fine:left-abs').data;
 assert.equal(derivative.validity.status,'conditional');assert.deepEqual(derivative.validity.domain.bandHz,[0,1e8]);assert.deepEqual(derivative.validity.domain.sourceResourceHashes,[sha]);
 assert.deepEqual(derivative.axis.values,fine.axis);assert.deepEqual(derivative.values,fine.portVoltages.map((v:number[])=>Math.abs(v[0])));
 for(let k=0;k<fine.axis.length;k++)for(let i=0;i<2;i++)refinement=Math.max(refinement,Math.abs(coarse.driverCurrentsA[k][i]-fine.driverCurrentsA[k][i]));
 assert.ok(refinement<=.001*.02+1e-12);
 const ac=get('ac');let acError=0;
 const derived=result.reports.flatMap((r:any)=>r.descriptors).find((d:any)=>d.id===ac.network.id);
 assert.match(derived.input.model.source,/Computed explicit frequency network; no temporal/);assert.match(derived.input.model.sha256,/^[a-f0-9]{64}$/);
 assert.equal(ac.solution.conditional,true);assert.deepEqual(ac.solution.bandHz,[0,1e8]);assert.equal(ac.solution.sourceHashes[0].sha256,sha);
 for(let k=0;k<ac.solution.frequenciesHz.length;k++){
  const ref=resonantPhysicalAc(ac.solution.frequenciesHz[k]);
  for(let i=0;i<2;i++)for(let j=0;j<2;j++){
   const z=ac.solution.parameters[k][i][j],diff=50*Math.hypot(finite(z.real)-ref[i][j][0],finite(z.imaginary)-ref[i][j][1]);
   const normalized=diff/(50*Math.hypot(...ref[i][j])+2e-6);acError=Math.max(acError,normalized);assert.ok(normalized<=.005);
  }
 }
 for(let i=0;i<2;i++)for(let j=0;j<2;j++){
  const plot=plots.find((p:any)=>p.title===`ac:${i===0?'left':'right'}:${j===0?'left':'right'}`).data;
  assert.equal(plot.validity.status,'conditional');assert.deepEqual(plot.axis.values,ac.solution.frequenciesHz);
  plot.values.forEach((v:number,k:number)=>near(v,Math.hypot(ac.solution.parameters[k][i][j].real,ac.solution.parameters[k][i][j].imaginary),1e-12));
 }
 return{status:'pass',cases:report,refinementErrorA:refinement,acHoldoutMaximum:acError,negativeCount:negatives.length,sourceSha256:sha};
}
