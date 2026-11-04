import type { AutoFoilPlan } from './auto-foil-plan.ts';
import type { ChannelLossGram } from './channel-loss.ts';
import type { BoardChannelRequest } from './channel-components.ts';
import { declaredNetworkBand } from './analysis-dsl/frequency-model.ts';
import type { LegacyChannelContact } from './channel-terminals.ts';
import {boardChannelBlockBandHz,boardChannelJointSkinBandHz,createBoardChannelPlanner} from './board-channel-plan.ts';
import {solveSegmentedChannel} from './segmented-channel.ts';
import {auditSegmentedResponse,type ChannelFrequencyReplay} from './segmented-frequency-audit.ts';
import type {FrequencyInterval} from './p7-frequency-planning.ts';
export function channelResponseChange(a:any,b:any,includesComponentLoss=false){
 if(JSON.stringify(a.ports)!==JSON.stringify(b.ports)||JSON.stringify(a.frequenciesHz)!==JSON.stringify(b.frequenciesHz)||JSON.stringify(a.referenceOhm)!==JSON.stringify(b.referenceOhm))throw new Error('CHANNEL_CONVERGENCE: physical output mapping/axis changed');
 let wave=0,loss=0,delay=0,copperLoss=0;let magnetic:number|null=null;const n=b.ports.length,drives=[...Array.from({length:n},(_,i)=>Array.from({length:n},(_,j)=>i===j?1:0)),Array.from({length:n},(_,i)=>i===0?1:-1)];
 const separated=includesComponentLoss&&(a.channelLossGrams!==undefined||b.channelLossGrams!==undefined);
 const copperA=a.channelLossGrams as ChannelLossGram[]|undefined,copperB=b.channelLossGrams as ChannelLossGram[]|undefined;
 if(separated&&(!Array.isArray(copperA)||!Array.isArray(copperB)||copperA.length!==b.frequenciesHz.length||copperB.length!==b.frequenciesHz.length||
  b.frequenciesHz.some((f:number,i:number)=>copperA[i].frequencyHz!==f||copperB[i].frequencyHz!==f)))
  throw new Error('CHANNEL_COPPER_LOSS_EVIDENCE: incomplete matched loss axis');
 for(let f=0;f<b.parameters.length;f++)for(const [driveIndex,d] of drives.entries()){const norm=Math.hypot(...d);let pa=12.5*norm*norm,pb=pa;
  for(let i=0;i<n;i++){let ar=0,ai=0,br=0,bi=0;for(let j=0;j<n;j++){ar+=a.parameters[f][i][j].real*d[j];ai+=a.parameters[f][i][j].imaginary*d[j];br+=b.parameters[f][i][j].real*d[j];bi+=b.parameters[f][i][j].imaginary*d[j];}
   wave=Math.max(wave,Math.hypot(ar-br,ai-bi)/norm/Math.max(.01,Math.hypot(br,bi)/norm));pa-=12.5*(ar*ar+ai*ai);pb-=12.5*(br*br+bi*bi);
  }
  loss=Math.max(loss,Math.abs(pa-pb)/Math.max(1e-18,Math.abs(pb)));
  if(separated){
   const power=(sample:ChannelLossGram)=>driveIndex===n?sample.oppositeCopperW:sample.copperWPerA2[driveIndex][driveIndex];
   const ca=power(copperA![f]),cb=power(copperB![f]);
   if(!Number.isFinite(ca)||!Number.isFinite(cb)||ca<0||cb<0)
    throw new Error('CHANNEL_COPPER_LOSS_EVIDENCE: nonfinite or negative Cu loss for a verified drive');
   copperLoss=Math.max(copperLoss,Math.abs(ca-cb)/Math.max(1e-18,Math.abs(cb)));
  }
 }
 // Adjacent-point phase slopes expose delay changes hidden by nearly unit S.
 // Phase differences are principal unwrapped increments; sparse axes admitting
 // more than pi phase change still require further physical-band refinement.
 for(let f=1;f<b.frequenciesHz.length;f++)for(let i=0;i<n;i++)for(let j=0;j<n;j++)if(i!==j){
  const phases=(r:any)=>{const x=r.parameters[f-1][i][j],y=r.parameters[f][i][j];return Math.atan2(y.imaginary*x.real-y.real*x.imaginary,y.real*x.real+y.imaginary*x.imaginary);};
  const p=b.parameters[f-1][i][j],q=b.parameters[f][i][j];if(Math.min(Math.hypot(p.real,p.imaginary),Math.hypot(q.real,q.imaginary))<.01)continue;
  const scale=2*Math.PI*(b.frequenciesHz[f]-b.frequenciesHz[f-1]),da=-phases(a)/scale,db=-phases(b)/scale;delay=Math.max(delay,Math.abs(da-db)/Math.max(1e-12,Math.abs(db)));
 }
 if(a.loadedEnergyGram||b.loadedEnergyGram){
  if(!Array.isArray(a.loadedEnergyGram)||!Array.isArray(b.loadedEnergyGram)||a.loadedEnergyGram.length!==b.frequenciesHz.length||b.loadedEnergyGram.length!==b.frequenciesHz.length)throw new Error('CHANNEL_MAGNETIC_EVIDENCE: complete matched energy axis required');
  magnetic=0;for(let f=0;f<b.frequenciesHz.length;f++)for(let i=0;i<n;i++)for(let j=0;j<n;j++){const x=a.loadedEnergyGram[f]?.magneticJPerA2?.[i]?.[j],y=b.loadedEnergyGram[f]?.magneticJPerA2?.[i]?.[j];if(!Number.isFinite(x)||!Number.isFinite(y))throw new Error('CHANNEL_MAGNETIC_EVIDENCE: finite complete intrinsic energy Gram required');magnetic=Math.max(magnetic,Math.abs(x-y)/Math.max(5e-12,Math.abs(y)));}
 }
 return {maxRelativeChange:Math.max(wave,loss,copperLoss,delay,magnetic??0),magneticEnergyRelativeChange:magnetic,magneticEnergyAbsoluteFloorJ:5e-12,magneticEnergyConvention:'Native matched-reference mean intrinsic magnetic energy Gram for1A RMS Norton drives; references store no magnetic energy',waveRelativeChange:wave,copperLossRelativeChange:includesComponentLoss?(separated?copperLoss:undefined):loss,totalPassiveLossRelativeChange:includesComponentLoss?loss:undefined,groupDelayRelativeChange:delay,groupDelayAbsoluteFloorS:1e-12,waveAbsoluteAmplitudeFloor:.01,lossAbsoluteFloorW:1e-18,referenceOhm:50,driveConvention:'Unit/opposite1A Norton with50Ohm mathematical reference; incident power50/4*norm(d)^2',lossRecovery:includesComponentLoss?(separated?'Cu-only convergence uses native copper instance R loss; regional recovery remains unverified':'Passive power balance includes supplied package losses; Cu-only and regional convergence remain unverified'):'Passive power balance; only Cu R dissipates; no regional loss certificate'};
}
export function boardChannelApplicability(request: BoardChannelRequest, requestedUpperHz:number=boardChannelBlockBandHz[1]): FrequencyInterval {
 const bands=(request.components??[]).flatMap(component=>{
  const band=declaredNetworkBand(component.model);
  return band?[band]:[];
 });
 const band=requestedUpperHz>boardChannelBlockBandHz[1]?boardChannelJointSkinBandHz:boardChannelBlockBandHz;
 return [Math.max(band[0],...bands.map(component=>component[0])),
  Math.min(band[1],...bands.map(component=>component[1]))];
}
type VerificationRun = {
 level: 1 | 2;
 evidence: ReturnType<typeof solveSegmentedChannel>['evidence'];
 wallSeconds: number;
};
type Comparison = ReturnType<typeof channelResponseChange>;
type DomainVerification = {
 domainScale: number;
 runs: VerificationRun[];
 wallSeconds: number;
 domain?: Comparison;
 spatial?: Comparison;
 boundary?: Comparison & { fractions: number[]; evidence: VerificationRun['evidence'] };
};

/** Use the same coarse discretization to select a domain. Then independently
 * verify mesh and cut placement on the domain whose fine result will be returned. */
export function createBoardChannelExecutor<FoilResult=never>(
 createPlanner = createBoardChannelPlanner, solve = solveSegmentedChannel,
 solveFoil?:(plan:AutoFoilPlan,frequencies:number[],budget:{maxSeconds:number;maxMemoryMiB:number},interval?:FrequencyInterval)=>FoilResult,
){
 return (board:any,base:any,contacts:LegacyChannelContact[]|BoardChannelRequest,frequencies:number[],budget:{maxSeconds:number;maxMemoryMiB:number},requestedIntervalHz?:FrequencyInterval)=>{
  const start=performance.now(),elapsed=()=>(performance.now()-start)/1000;
  const left=()=>({...budget,maxSeconds:budget.maxSeconds-elapsed()});
  const planChannel=createPlanner(board,base,contacts);
  const demandUpper=frequencies.reduce((upper,f)=>Math.max(upper,f),requestedIntervalHz?.[1]??0);
  if(solveFoil&&demandUpper>boardChannelBlockBandHz[1]){
   const foil=planChannel({closedJointDeadlineMs:start+budget.maxSeconds*1000});
   return solveFoil(foil,frequencies,left(),requestedIntervalHz);
  }
  const sectionOptions={expandAutomaticSectionMesh:true};
  const axis=[...frequencies];
  const components=Array.isArray(contacts)?[]:contacts.components??[];
  const [low,high]=Array.isArray(contacts)?boardChannelBlockBandHz:boardChannelApplicability(contacts);
  if(components.length&&(!axis.length||low>high||axis.some(f=>!Number.isFinite(f)||f<low||f>high)))
   throw new Error('CHANNEL_COMPONENT_BAND: requested axis exceeds common board and supplied component AC band');
  for(const f of [low,Math.sqrt(low*high),high])if(!axis.some(x=>Math.abs(x-f)<=64*Number.EPSILON*Math.max(x,f)))axis.push(f);
  axis.sort((a,b)=>a-b);
  const useFineAsOutput=JSON.stringify(axis)===JSON.stringify(frequencies);
  let replay:ChannelFrequencyReplay|undefined;
  const auditedOptions=requestedIntervalHz?{...sectionOptions,captureFrequencyReplay:(evaluate:ChannelFrequencyReplay)=>{replay=evaluate;}}:sectionOptions;
  let previous: ReturnType<typeof solve> | undefined;
  const verification: DomainVerification[]=[];
  for(const domainScale of [1,2,4]){
   const boundaryFraction=1/3;
   let coarsePlan:ReturnType<typeof planChannel>;
   try{coarsePlan=planChannel({domainScale,magneticPatchLevel:1,boundaryFraction,
     ...(solveFoil&&domainScale===1&&{autoFoilDeadlineMs:start+budget.maxSeconds*1000})});}
   catch(error){
    const candidate=error as {code?:string;foilCandidate?:AutoFoilPlan};
    if(!solveFoil||domainScale!==1||candidate?.code!=='CHANNEL_CORRIDOR_UNSUPPORTED'||!candidate.foilCandidate)throw error;
    return solveFoil(candidate.foilCandidate,frequencies,left(),requestedIntervalHz);
   }
   const coarse=solve(coarsePlan,axis,left(),sectionOptions);
   const runs: VerificationRun[]=[{level:1,evidence:coarse.evidence,wallSeconds:coarse.wallSeconds}];
   const entry: DomainVerification={domainScale,runs,wallSeconds:elapsed()};
   verification.push(entry);
   if(!previous){
    previous=coarse;
    continue;
   }
   const domain=channelResponseChange(previous.result,coarse.result,components.length>0);
   if(domain.magneticEnergyRelativeChange===null)throw new Error('CHANNEL_MAGNETIC_EVIDENCE: board domain admission requires intrinsic energy');
   entry.domain=domain;
   if(!(domain.maxRelativeChange<=.02)){
    previous=coarse;
    continue;
   }

   const finePlan=planChannel({domainScale,magneticPatchLevel:2,boundaryFraction});
   const fine=solve(finePlan,axis,left(),useFineAsOutput?auditedOptions:sectionOptions);
   runs.push({level:2,evidence:fine.evidence,wallSeconds:fine.wallSeconds});
   entry.wallSeconds=elapsed();
   const spatial=channelResponseChange(coarse.result,fine.result,components.length>0);
   if(spatial.magneticEnergyRelativeChange===null)throw new Error('CHANNEL_MAGNETIC_EVIDENCE: board admission requires intrinsic energy');
   entry.spatial=spatial;
   if(!(spatial.maxRelativeChange<=.02))throw new Error('CHANNEL_SPATIAL_CONVERGENCE: '+JSON.stringify(verification));

   const shiftedFraction=1/4;
   const shiftedPlan=planChannel({domainScale,magneticPatchLevel:1,boundaryFraction:shiftedFraction});
   const shifted=solve(shiftedPlan,axis,left(),sectionOptions);
   entry.wallSeconds=elapsed();
   const boundary=channelResponseChange(coarse.result,shifted.result,components.length>0);
   if(boundary.magneticEnergyRelativeChange===null)throw new Error('CHANNEL_MAGNETIC_EVIDENCE: board cut admission requires intrinsic energy');
   entry.boundary={...boundary,fractions:[shiftedFraction,boundaryFraction],evidence:shifted.evidence};
   if(!(boundary.maxRelativeChange<=.02))throw new Error('CHANNEL_BOUNDARY_CONVERGENCE: '+JSON.stringify(verification));

   const output=useFineAsOutput?fine:solve(finePlan,frequencies,left(),auditedOptions);
   const frequencyAudit=requestedIntervalHz?auditSegmentedResponse({
    requestedIntervalHz,applicabilityBandHz:output.bandHz,portNames:output.result.ports!.map(port=>port.name),
    deadlineMs:start+budget.maxSeconds*1000,
    evaluate:(frequencyHz,maxSeconds)=>{
     if(!replay)throw new Error('CHANNEL_FREQUENCY_REPLAY: admitted model evaluator unavailable');
     return replay(frequencyHz,maxSeconds);
    },
   }):undefined;
   return {...output,...(frequencyAudit&&{frequencyAudit}),audit:finePlan.audit,verification,blocks:finePlan.blocks.map(({id,kind,cuts})=>({id,kind,cuts})),wallSeconds:elapsed()};
  }
  throw new Error('CHANNEL_DOMAIN_CONVERGENCE: '+JSON.stringify(verification));
 };
}

export const solveBoardChannel=createBoardChannelExecutor();
