import {createHash,randomUUID} from 'node:crypto';
import type {BoardFoilRunReport} from '../auto-foil-response.ts';
import type {AutoFoilPlan} from '../auto-foil-plan.ts';
import type {ExecutionResult} from './executor.ts';
import type {Diagnostic,NetworkResource,Validity} from './spec-doc.js';
import {json} from './data.ts';
import {boardFoilPortObservations} from '../board-foil-port-observations.ts';
import {boardFoilMixedMode} from '../board-foil-mixed-mode.ts';

export function boardFoilResult(
 contextId:string, plan:AutoFoilPlan, report:BoardFoilRunReport, diagnostics:Diagnostic[],
):ExecutionResult{
 plan=report.resolvedPlan??plan;
 const terminals=new Map(plan.terminals.map(t=>[t.id,t]));
 const contact=(id:string)=>{const {sourcePath,layer}=terminals.get(id)!;return {sourcePath,layer};};
 const physicalPorts=plan.ports.map(({positive,negative})=>({positive:contact(positive),negative:contact(negative)}));
 const id=contextId+':foil-'+randomUUID();
 const modelKind=report.provenance.modelKind;
 const model=report.provenance.modelKind==='mixed'&&report.provenance.compactOperator?(report.provenance.prisms===0?'local volumetric copper':
  report.provenance.volumeTetrahedra===0?'distributed planar copper':'coupled planar and local copper'):modelKind==='foil'?'planar foil copper':
  report.provenance.prisms===0?'volumetric copper':
  report.provenance.modelKind==='mixed'&&report.provenance.volumeTetrahedra===0?'foil copper with full volume audit':'mixed foil and volume copper';
 const producer={
  boardSha256:createHash('sha256').update(plan.boardJson).digest('hex'),
  geometrySha256:createHash('sha256').update(JSON.stringify(plan.geometry)).digest('hex'),
  ...(plan.attachments.length&&{attachmentsSha256:createHash('sha256').update(JSON.stringify(plan.attachments)).digest('hex')}),
 };
 const jointSensitivity=report.jointSensitivity?.checks.map(check=>({kind:check.kind,status:check.status}));
 const sensitivityDiagnostics:Diagnostic[]=(jointSensitivity??[]).flatMap(({kind,status})=>status==='pass'?[]:[{
  code:status==='requires-review'?'CHANNEL_FOIL_SENSITIVITY_REVIEW':'CHANNEL_FOIL_SENSITIVITY_UNVERIFIED',
  severity:'warning',message:'Joint '+kind+' sensitivity is '+status+'. See jointSensitivity evidence.',
 }]);
 const frequency=report.frequencyAudit?.interpolation;
 const frequencyDiagnostics:Diagnostic[]=[];
 if(frequency?.unverified.length)frequencyDiagnostics.push({
  code:'CHANNEL_FOIL_FREQUENCY_UNVERIFIED',severity:'warning',
  message:'Joint frequency audit left requested intervals unverified. See frequencyAudit evidence.',
 });
 if(frequency?.holdouts.some(holdout=>holdout.status==='requires-review'))frequencyDiagnostics.push({
  code:'CHANNEL_FOIL_FREQUENCY_REVIEW',severity:'warning',
  message:'At least one frequency holdout exceeds the 2% sensitivity gate and requires review.',
 });
 const validity:Validity={status:'conditional',reason:'Native residual and power checks cover the exact grid in one bounded '+model+' model. Independent spatial, domain and between-point coverage remain unverified.',
  domain:json({producer,modelKind,samplePolicy:'exact',evaluatedFrequenciesHz:report.frequenciesHz,spatialCoverage:'unverified',domainCoverage:'unverified',bandCoverage:'unverified',energyVerification:'unverified',jointSensitivity,frequencyAudit:frequency&&{requestedIntervalHz:frequency.requestedIntervalHz,status:frequency.status}})};
 const network:NetworkResource={id,kind:'network',ports:plan.ports.map(port=>port.name),
  source:'Bounded conforming '+model+'; exact checked S50 samples of original finite contact pairs',
  analyses:['networkParameters'],validity,data:{format:'physical-port-samples',samplePolicy:'exact',
   representation:'S',referenceTopology:'physical-pairs',nature:'passive',
   frequenciesHz:report.frequenciesHz,referenceOhm:report.referenceOhm,parameters:report.parameters,physicalPorts}};
 const electrodes=new Map(plan.geometry.electrodes.map(({name,sourcePath,layer})=>[name,{sourcePath,layer}]));
 const packages=plan.attachments.map(({id,electrodes:pair,model})=>({
  id,electrodes:pair,physicalContacts:pair.map(name=>electrodes.get(name)!),modelId:model.id,
 }));
 const evidence={...(packages.length&&{packages}),producer,roiMm:plan.roiMm,domain:plan.domain,sourceCopper:plan.sourceCopper,sourceBarrels:plan.sourceBarrels,ports:plan.ports,physicalPorts,native:report.provenance};
 return {id,status:'partial',method:report.provenance.method+'; checked matched S samples',
  diagnostics:[...diagnostics,...frequencyDiagnostics,...sensitivityDiagnostics,{code:'CHANNEL_FOIL_PHYSICAL_UNVERIFIED',severity:'warning',message:validity.reason!}],
  assumptions:['One finite joint '+model+' domain. Original physical contacts and retained barrel spans are preserved without equipotential corridor cuts or implicit reference connections.',
   ...(packages.length?['Explicit package R/C/L models connect their original finite pads. Copper and package dissipation are reported separately.']:[]),
   'Exact-grid standalone frequency replay uses the original paired contacts and S50 references.'],
  metrics:{segments:{kind:'table',unit:'1',data:json(evidence),validity,diagnostics:[]}},
  network,solution:json({...report,physicalPorts,portNames:network.ports,portObservations:boardFoilPortObservations(plan,report),
   ...(plan.modePairs?.length&&{mixedMode:boardFoilMixedMode(report,network.ports,plan.modePairs)}),foilPlan:evidence,validity})};
}
