import {createHash,randomUUID} from 'node:crypto';
import type {AnalysisBoardFoilCheckedReport} from '../../../native/analyze/index.js';
import type {AutoFoilPlan} from '../auto-foil-plan.ts';
import type {ExecutionResult} from './executor.ts';
import type {Diagnostic,NetworkResource,Validity} from './spec-doc.js';
import {json} from './data.ts';

export function boardFoilResult(
 contextId:string, plan:AutoFoilPlan, report:AnalysisBoardFoilCheckedReport, diagnostics:Diagnostic[],
):ExecutionResult{
 const terminals=new Map(plan.terminals.map(t=>[t.id,t]));
 const contact=(id:string)=>{const {sourcePath,layer}=terminals.get(id)!;return {sourcePath,layer};};
 const physicalPorts=plan.ports.map(({positive,negative})=>({positive:contact(positive),negative:contact(negative)}));
 const id=contextId+':foil-'+randomUUID();
 const producer={
  boardSha256:createHash('sha256').update(plan.boardJson).digest('hex'),
  geometrySha256:createHash('sha256').update(JSON.stringify(plan.geometry)).digest('hex'),
 };
 const validity:Validity={status:'conditional',reason:'Native residual and power checks cover the exact grid in one bounded planar model. Independent spatial, domain and between-point coverage remain unverified.',
  domain:json({producer,samplePolicy:'exact',evaluatedFrequenciesHz:report.frequenciesHz,spatialCoverage:'unverified',domainCoverage:'unverified',bandCoverage:'unverified',energyVerification:'unverified'})};
 const network:NetworkResource={id,kind:'network',ports:plan.ports.map(port=>port.name),
  source:'Bounded conforming board foil; exact checked S50 samples of original finite contact pairs',
  analyses:['networkParameters'],validity,data:{format:'physical-port-samples',samplePolicy:'exact',
   representation:'S',referenceTopology:'physical-pairs',nature:'passive',
   frequenciesHz:report.frequenciesHz,referenceOhm:report.referenceOhm,parameters:report.parameters,physicalPorts}};
 const evidence={producer,roiMm:plan.roiMm,sourceCopper:plan.sourceCopper,ports:plan.ports,physicalPorts,native:report.provenance};
 return {id,status:'partial',method:'Joint conforming foil; checked matched S samples',
  diagnostics:[...diagnostics,{code:'CHANNEL_FOIL_PHYSICAL_UNVERIFIED',severity:'warning',message:validity.reason!}],
  assumptions:['One finite joint planar copper domain. No equipotential corridor cuts or implicit reference connections.',
   'Exact-grid standalone frequency replay uses the original paired contacts and S50 references.'],
  metrics:{segments:{kind:'table',unit:'1',data:json(evidence),validity,diagnostics:[]}},
  network,solution:json({...report,physicalPorts,portNames:network.ports,foilPlan:evidence,validity})};
}