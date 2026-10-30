import {createBoardChannelPlanner} from './board-channel-plan.ts';
import {solveSegmentedChannel} from './segmented-channel.ts';
export function channelResponseChange(a:any,b:any){
 if(JSON.stringify(a.ports)!==JSON.stringify(b.ports)||JSON.stringify(a.frequenciesHz)!==JSON.stringify(b.frequenciesHz))throw new Error('CHANNEL_CONVERGENCE: physical output mapping/axis changed');
 let wave=0,loss=0,delay=0;const n=b.ports.length,drives=[...Array.from({length:n},(_,i)=>Array.from({length:n},(_,j)=>i===j?1:0)),Array.from({length:n},(_,i)=>i===0?1:-1)];
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
 return {maxRelativeChange:Math.max(wave,loss,delay),waveRelativeChange:wave,copperLossRelativeChange:loss,groupDelayRelativeChange:delay,groupDelayAbsoluteFloorS:1e-12,waveAbsoluteAmplitudeFloor:.01,lossAbsoluteFloorW:1e-18,referenceOhm:50,driveConvention:'Unit/opposite1A Norton with50Ohm mathematical reference; incident power50/4*norm(d)^2',lossRecovery:'Passive power balance; only Cu R dissipates; no regional loss certificate'};
}
/** Carry the existing independent spatial and domain2% gates into the complete
 * assembled response, with fixed unit/opposite power-wave drives and floors. */
export function solveBoardChannel(board:any,base:any,contacts:{sourcePath:string;layer:string}[],frequencies:number[],budget:{maxSeconds:number;maxMemoryMiB:number}){
 const start=performance.now(),left=()=>({...budget,maxSeconds:budget.maxSeconds-(performance.now()-start)/1000});
 const planChannel=createBoardChannelPlanner(board,base,contacts);
 const axis=[...frequencies];for(const f of [1000,Math.sqrt(1e7),10000])if(!axis.some(x=>Math.abs(x-f)<=64*Number.EPSILON*Math.max(x,f)))axis.push(f);axis.sort((a,b)=>a-b);let previous:any;const verification:any[]=[];
 for(const domainScale of [1,2,4]){
  let coarse:any,fine:any,plan:any;const runs:any[]=[];
  for(const level of [1,2]as const){plan=planChannel({domainScale,magneticPatchLevel:level});const r=solveSegmentedChannel(plan,axis,left());runs.push({level,evidence:r.evidence,wallSeconds:r.wallSeconds});if(level===1)coarse=r;else fine=r;}
  const spatial=channelResponseChange(coarse.result,fine.result);verification.push({domainScale,spatial,runs,wallSeconds:(performance.now()-start)/1000});
  if(spatial.maxRelativeChange>.02)throw new Error('CHANNEL_SPATIAL_CONVERGENCE: '+JSON.stringify(verification));
  if(previous){const domain=channelResponseChange(previous.result,fine.result);verification.at(-1).domain=domain;if(domain.maxRelativeChange<=.02){const output=JSON.stringify(axis)===JSON.stringify(frequencies)?fine:solveSegmentedChannel(plan,frequencies,left());return {...output,audit:plan.audit,verification,blocks:plan.blocks.map((b:any)=>({id:b.id,kind:b.kind,cuts:b.cuts})),wallSeconds:(performance.now()-start)/1000};}}
  previous=fine;
 }
 throw new Error('CHANNEL_DOMAIN_CONVERGENCE: '+JSON.stringify(verification));
}
