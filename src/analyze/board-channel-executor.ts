import {createBoardChannelPlanner} from './board-channel-plan.ts';
import {solveSegmentedChannel} from './segmented-channel.ts';
export function channelResponseChange(a:any,b:any){
 if(JSON.stringify(a.ports)!==JSON.stringify(b.ports)||JSON.stringify(a.frequenciesHz)!==JSON.stringify(b.frequenciesHz)||JSON.stringify(a.referenceOhm)!==JSON.stringify(b.referenceOhm))throw new Error('CHANNEL_CONVERGENCE: physical output mapping/axis changed');
 let wave=0,loss=0,delay=0;let magnetic:number|null=null;const n=b.ports.length,drives=[...Array.from({length:n},(_,i)=>Array.from({length:n},(_,j)=>i===j?1:0)),Array.from({length:n},(_,i)=>i===0?1:-1)];
 for(let f=0;f<b.parameters.length;f++)for(const d of drives){const norm=Math.hypot(...d);let pa=12.5*norm*norm,pb=pa;
  for(let i=0;i<n;i++){let ar=0,ai=0,br=0,bi=0;for(let j=0;j<n;j++){ar+=a.parameters[f][i][j].real*d[j];ai+=a.parameters[f][i][j].imaginary*d[j];br+=b.parameters[f][i][j].real*d[j];bi+=b.parameters[f][i][j].imaginary*d[j];}
   wave=Math.max(wave,Math.hypot(ar-br,ai-bi)/norm/Math.max(.01,Math.hypot(br,bi)/norm));pa-=12.5*(ar*ar+ai*ai);pb-=12.5*(br*br+bi*bi);
  }
  loss=Math.max(loss,Math.abs(pa-pb)/Math.max(1e-18,Math.abs(pb)));
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
 return {maxRelativeChange:Math.max(wave,loss,delay,magnetic??0),magneticEnergyRelativeChange:magnetic,magneticEnergyAbsoluteFloorJ:5e-12,magneticEnergyConvention:'Native matched-reference mean intrinsic magnetic energy Gram for1A RMS Norton drives; references store no magnetic energy',waveRelativeChange:wave,copperLossRelativeChange:loss,groupDelayRelativeChange:delay,groupDelayAbsoluteFloorS:1e-12,waveAbsoluteAmplitudeFloor:.01,lossAbsoluteFloorW:1e-18,referenceOhm:50,driveConvention:'Unit/opposite1A Norton with50Ohm mathematical reference; incident power50/4*norm(d)^2',lossRecovery:'Passive power balance; only Cu R dissipates; no regional loss certificate'};
}
/** Carry the existing independent spatial and domain2% gates into the complete
 * assembled response, with fixed unit/opposite power-wave drives and floors. */
export function solveBoardChannel(board:any,base:any,contacts:{sourcePath:string;layer:string}[],frequencies:number[],budget:{maxSeconds:number;maxMemoryMiB:number}){
 const start=performance.now(),left=()=>({...budget,maxSeconds:budget.maxSeconds-(performance.now()-start)/1000});
 const planChannel=createBoardChannelPlanner(board,base,contacts);
 const axis=[...frequencies];for(const f of [1000,Math.sqrt(1e7),10000])if(!axis.some(x=>Math.abs(x-f)<=64*Number.EPSILON*Math.max(x,f)))axis.push(f);axis.sort((a,b)=>a-b);let previous:any,accepted:any,acceptedCoarse:any,acceptedPlan:any;const verification:any[]=[];
 for(const domainScale of [1,2,4]){
  let coarse:any,fine:any,plan:any;const runs:any[]=[];
  // Prove mesh convergence once, then compare domains at the same coarse
  // discretization. Repeating the already admitted fine mesh on every larger
  // domain is redundant and can exhaust the shared cold deadline.
  for(const level of (domainScale===1?[1,2]:[1])as (1|2)[]){plan=planChannel({domainScale,magneticPatchLevel:level,boundaryFraction:1/3});const r=solveSegmentedChannel(plan,axis,left());runs.push({level,evidence:r.evidence,wallSeconds:r.wallSeconds});if(level===1)coarse=r;else fine=r;}
  const entry:any={domainScale,runs,wallSeconds:(performance.now()-start)/1000};verification.push(entry);
  if(domainScale===1){const spatial=channelResponseChange(coarse.result,fine.result);if(spatial.magneticEnergyRelativeChange===null)throw new Error('CHANNEL_MAGNETIC_EVIDENCE: board admission requires intrinsic energy');entry.spatial=spatial;if(spatial.maxRelativeChange>.02)throw new Error('CHANNEL_SPATIAL_CONVERGENCE: '+JSON.stringify(verification));accepted=fine;acceptedCoarse=coarse;acceptedPlan=plan;previous=coarse;continue;}
  const domain=channelResponseChange(previous.result,coarse.result);if(domain.magneticEnergyRelativeChange===null)throw new Error('CHANNEL_MAGNETIC_EVIDENCE: board domain admission requires intrinsic energy');entry.domain=domain;if(domain.maxRelativeChange<=.02){const shiftedPlan=planChannel({domainScale:1,magneticPatchLevel:1,boundaryFraction:1/4});const shifted=solveSegmentedChannel(shiftedPlan,axis,left());const boundary=channelResponseChange(acceptedCoarse.result,shifted.result);if(boundary.magneticEnergyRelativeChange===null)throw new Error('CHANNEL_MAGNETIC_EVIDENCE: board cut admission requires intrinsic energy');entry.boundary={...boundary,fractions:[1/4,1/3],evidence:shifted.evidence};if(boundary.maxRelativeChange>.02)throw new Error('CHANNEL_BOUNDARY_CONVERGENCE: '+JSON.stringify(verification));const output=JSON.stringify(axis)===JSON.stringify(frequencies)?accepted:solveSegmentedChannel(acceptedPlan,frequencies,left());return {...output,audit:acceptedPlan.audit,verification,blocks:acceptedPlan.blocks.map((b:any)=>({id:b.id,kind:b.kind,cuts:b.cuts})),wallSeconds:(performance.now()-start)/1000};}
  previous=coarse;
 }
 throw new Error('CHANNEL_DOMAIN_CONVERGENCE: '+JSON.stringify(verification));
}
