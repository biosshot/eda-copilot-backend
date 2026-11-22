import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {auditElectricalMixed,mixedCases} from './helpers/electrical-mixed-audit.ts';
test('mixed saved-result independent audit rejects axis/amplitude/energy/validity/port/case corruption',async(t)=>{
 let result:any;
 try{result=JSON.parse(await readFile('debugging/p6-mixed-verification/direct-dsl/standalone/result.json','utf8')).result;}catch{t.skip('Generate current mixed saved evidence first');return;}
 assert.equal(auditElectricalMixed(result).status,'pass');
 const solution=(r:any)=>mixedCases(r).get('rc').solution;
 const corruptions=[
  (r:any)=>solution(r).axis[3]=solution(r).axis[2],
  (r:any)=>solution(r).portVoltages[40][1]+=.1,
  (r:any)=>solution(r).driverCurrentsA[5][0]=null,
  (r:any)=>solution(r).quality.beNumericalDissipationJ+=1,
  (r:any)=>solution(r).delayEnergyJ.pop(),
  (r:any)=>solution(r).quality.historyQuadratureDefectJ=null,
  (r:any)=>mixedCases(r).get('rlc frequency').solution.parameters[1][0][1].real+=.1,
  (r:any)=>{r.reports.flatMap((x:any)=>x.plots).find((p:any)=>p.title==='rc:input<-output').data.values.pop();},
  (r:any)=>solution(r).sourceResources[0].sourceValidity={status:'unavailable'},
  (r:any)=>solution(r).portMapping.outputs[1].name='wrong',
  (r:any)=>{r.reports.flatMap((x:any)=>x.logs).find((l:any)=>l.message==='mixed.case').data.name='missing';},
  (r:any)=>{r.reports.flatMap((x:any)=>x.plots).find((p:any)=>p.title==='rc:output').data.values=[];},
 ];
 for(const change of corruptions){const copy=structuredClone(result);change(copy);assert.throws(()=>auditElectricalMixed(copy));}
});
