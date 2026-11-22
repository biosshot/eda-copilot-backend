import {randomUUID} from 'node:crypto';
import type {ExecutionResult} from './executor.ts';
import type {Diagnostic,NetworkResource,Validity} from './spec-doc.js';
import {json} from './data.ts';
import type {CascadeSolved} from '../channel-cascade.ts';
import type {ChannelModelChoice} from '../channel-cascade-policy.ts';

/** Report entry of the channel cascade: the selection, sections, joints and checks next to the S50 network. */
export function channelCascadeResult(contextId:string,solved:CascadeSolved,choice:Extract<ChannelModelChoice,{mode:'cascade'}>,
 contacts:{sourcePath:string;layer:string}[],diagnostics:Diagnostic[]):ExecutionResult{
 const id=contextId+':cascade-'+randomUUID();
 const physicalPorts=[{positive:contacts[0],negative:contacts[2]},{positive:contacts[1],negative:contacts[3]}];
 const validity:Validity={status:'conditional',reason:'Approximation: a chain of finite-conductivity 2D sections stitched by S-parameters at each frequency, ideal joints without 3D blocks. Return is the continuous reference copper under the channel; return pads are not resolved. Physical accuracy is unverified.',
  domain:json({modelKind:'channel-cascade',samplePolicy:'exact',evaluatedFrequenciesHz:solved.frequenciesHz,spatialCoverage:'unverified',domainCoverage:'unverified',bandCoverage:'unverified',energyVerification:'unverified'})};
 const network:NetworkResource={id,kind:'network',ports:['input','output'],
  source:'Channel cascade of 2D wave sections, S50 stitched per frequency point',analyses:['networkParameters'],validity,
  data:{format:'physical-port-samples',samplePolicy:'exact',representation:'S',referenceTopology:'physical-pairs',nature:'passive',
   frequenciesHz:solved.frequenciesHz,referenceOhm:[50,50],parameters:solved.parameters,physicalPorts}};
 const evidence={mode:'cascade',status:'approximation',layer:choice.layer,portPlaneMm:choice.portPlaneMm,notModelled:choice.notModelled,
  sections:solved.sections,joints:solved.joints,checks:{maxSingularValue:solved.maxSingularValue,maxReciprocityError:solved.maxReciprocityError},
  delayPsByFrequency:solved.delayPs,totalLengthMm:solved.totalLengthMm,wallSeconds:solved.wallSeconds,physicalAccuracy:'unverified'};
 const warnings:Diagnostic[]=solved.warnings.map(message=>({code:'CHANNEL_CASCADE_JOINT_SHORT',severity:'warning',message}));
 return {id,status:'partial',method:'Channel cascade of 2D wave sections; S-parameters stitched per frequency point',
  diagnostics:[...diagnostics,...warnings,{code:'CHANNEL_CASCADE_APPROXIMATION',severity:'warning',message:validity.reason!}],
  assumptions:['Sections are uniform 2D cross-sections with R(f), L(f) sampled at each frequency; non-signal copper in a section is shorted to the reference at the section ends.',
   'Joints (bends, width steps, pad to track) are ideal; the estimated error per joint is reported. No 3D block is used.',
   'Input and output pads other than the signal track are not modelled beyond the planar section.'],
  metrics:{segments:{kind:'table',unit:'1',data:json(evidence),validity,diagnostics:[]}},
  network,solution:json({mode:'cascade',frequenciesHz:solved.frequenciesHz,referenceOhm:[50,50],parameters:solved.parameters,physicalPorts,portNames:network.ports,cascade:evidence,validity})};
}
