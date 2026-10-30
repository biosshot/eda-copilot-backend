import type {AnalysisCrossSectionRequest,AnalysisFrequencyResource} from '../../native/analyze/index.js';
import {createProblemExecutor} from './execute-problem.ts';

/** Internal physical cut identity, never a net label. Positive and negative
 * terminals retain orientation; a reference change requires a local block. */
export interface ChannelCut {positive:string;negative:string}
export interface CopperInterval {sourcePath:string;startMm:number;stopMm:number}
interface Base {id:string;cuts:ChannelCut[];ownership:CopperInterval[];bandHz:[number,number];couplingGroup?:string}
export type ChannelBlock = Base & (
 {kind:'section';line:AnalysisCrossSectionRequest['line'];mesh?:AnalysisCrossSectionRequest['mesh'];dcConductors?:{resistanceOhmPerM:number[][];internalInductanceHPerM:number[][]}}|
 {kind:'local';boardJson:string;geometry:unknown;magneticPatchLevel?:1|2}
);
export interface SegmentedChannel {blocks:ChannelBlock[];ports:{name:string;cut:ChannelCut}[];coverage:CopperInterval[]}
const fail=(s:string):never=>{throw new Error('CHANNEL_COMPOSITION: '+s);};
const cutKey=(c:ChannelCut)=>JSON.stringify([c.positive,c.negative]);
/** Validate before any FEM work: every owned longitudinal copper interval is
 * covered exactly once. Sharing an endpoint is allowed; overlap is not. */
export function validateChannelOwnership(plan:SegmentedChannel){
 if(!plan.blocks.length||plan.blocks.length>32||new Set(plan.blocks.map(b=>b.id)).size!==plan.blocks.length)fail('1..32 unique blocks required');
 const groups=new Map<string,string>();
 for(const b of plan.blocks){
  if(b.bandHz.length!==2||b.bandHz.some(v=>!Number.isFinite(v)||v<=0)||b.bandHz[0]>b.bandHz[1])fail('Invalid component band');
  if(b.cuts.length<1||b.cuts.some(c=>!c.positive||!c.negative||c.positive===c.negative))fail('Invalid oriented physical cut');
  if(b.couplingGroup){if(groups.has(b.couplingGroup))fail('Nonlocal coupling group must be extracted jointly: '+b.couplingGroup);groups.set(b.couplingGroup,b.id);}
 }
 const bySource=(xs:CopperInterval[])=>{const m=new Map<string,CopperInterval[]>();for(const x of xs){if(!x.sourcePath||!Number.isFinite(x.startMm)||!Number.isFinite(x.stopMm)||x.startMm>=x.stopMm)fail('Invalid SI-boundary copper interval');const a=m.get(x.sourcePath)??[];a.push(x);m.set(x.sourcePath,a);}for(const a of m.values())a.sort((x,y)=>x.startMm-y.startMm);return m;};
 const owned=bySource(plan.blocks.flatMap(b=>b.ownership)),expected=bySource(plan.coverage);
 if(owned.size!==expected.size)fail('Owned/source coverage mismatch');
 for(const [source,spans]of expected){const actual=owned.get(source)??fail('Missing source '+source);let j=0;
  for(const span of spans){let x=span.startMm;while(j<actual.length&&actual[j].startMm<span.stopMm){const a=actual[j++];if(Math.abs(a.startMm-x)>1e-9||a.stopMm>span.stopMm+1e-9)fail('Copper overlap, gap or out-of-domain ownership: '+source);x=a.stopMm;}if(Math.abs(x-span.stopMm)>1e-9)fail('Uncovered copper: '+source);}
  if(j!==actual.length)fail('Extra copper ownership: '+source);
 }
 const band:[number,number]=[Math.max(...plan.blocks.map(b=>b.bandHz[0])),Math.min(...plan.blocks.map(b=>b.bandHz[1]))];if(band[0]>band[1])fail('No common physical band');return band;
}
/** Real existing section/local producers and P6 assembly. No independent
 * two-port cascade: branch cuts connect all incident matrix ports together. */
export function solveSegmentedChannel(plan:SegmentedChannel,frequencies:number[],budget={maxSeconds:300,maxMemoryMiB:2048}){
 const band=validateChannelOwnership(plan);if(!frequencies.length||frequencies.some(f=>!Number.isFinite(f)||f<band[0]||f>band[1]))fail('Requested axis outside common physical band');
 const execute=createProblemExecutor(true),started=performance.now();const remaining=()=>{const s=budget.maxSeconds-(performance.now()-started)/1000;if(s<=0)fail('Shared channel deadline');return s;};
 const instances:{id:string;model:AnalysisFrequencyResource;ports:string[]}[]=[],evidence:unknown[]=[];
 const cuts=new Map<string,number[]>();let count=0;
 for(const block of plan.blocks){let model:AnalysisFrequencyResource;
  if(block.kind==='section'){
   const r=execute({kind:'cross-section',request:{line:block.line,mesh:block.mesh,frequencies,budget:{...budget,maxSeconds:remaining()}}});
   if(r.status!=='complete'||!r.network)fail('Section '+block.id+' refused: '+JSON.stringify(r.diagnostics));
   model={id:block.id,kind:'network',ports:Array.from({length:block.cuts.length},(_,i)=>'port-'+i),data:sectionDescriptor(r.parameters!,block.line,frequencies,block.dcConductors)};
   evidence.push({id:block.id,kind:block.kind,parameters:r.parameters,sectionRealization:{type:'physical RLC Pi',cellPhaseLimitRad:.01,dcConductors:block.dcConductors??null,units:{length:'mm to m at boundary',R:'Ohm/m',L:'H/m',C:'F/m'}},cache:r.cache,resources:r.resources});
  }else{
   const r=execute({kind:'local-extraction',request:{boardJson:block.boardJson,geometry:block.geometry,remainingSeconds:remaining(),bandHz:[1000,100000],frequenciesHz:frequencies,magneticPatchLevel:block.magneticPatchLevel,observablesOnly:true}});
   const ports=(r.prepared as any).request.ports.map((_:unknown,i:number)=>'port-'+i);
   model={id:block.id,kind:'network',ports,data:r.model};evidence.push({id:block.id,kind:block.kind,cache:r.cache,stageCounts:r.stageCounts,resources:r.resources});
  }
  if(model.ports.length!==block.cuts.length)fail('Model/cut port count mismatch: '+block.id);
  instances.push({id:block.id,model,ports:model.ports});
  for(const cut of block.cuts){const k=cutKey(cut),a=cuts.get(k)??[];a.push(count++);cuts.set(k,a);}
 }
 const connections:[number,number][]=[],connectionContacts:('both'|'positive'|'negative')[]=[],welds=new Map<string,number>();
 const terminalGroups=new Map<string,{port:number;contact:'positive'|'negative'}[]>();let offset=0;
 for(const block of plan.blocks)for(const cut of block.cuts){for(const contact of ['positive','negative'] as const){const id=cut[contact],a=terminalGroups.get(id)??[];a.push({port:offset,contact});terminalGroups.set(id,a);}offset++;}
 for(const a of terminalGroups.values())for(let i=1;i<a.length;i++){if(a[i].contact!==a[0].contact)fail('A physical terminal changed signal/return orientation across cuts');const pair=[a[0].port,a[i].port].sort((x,y)=>x-y),key=pair.join(',');welds.set(key,(welds.get(key)??0)|(a[0].contact==='positive'?1:2));}
 for(const [key,mask]of welds){connections.push(key.split(',').map(Number) as [number,number]);connectionContacts.push(mask===3?'both':mask===1?'positive':'negative');}
 const ports=plan.ports.map(p=>({name:p.name,index:cuts.get(cutKey(p.cut))?.[0]??fail('External cut absent: '+p.name)}));
 // Every descriptor retains distinct physical signal and return cut terminals.
 // P6 handles the complete branched multiport matrix, including local RLC.
 const result=execute({kind:'frequency-network',request:{instances,connections,connectionContacts,ports,frequencies,representation:'S',referenceOhm:ports.map(()=>50),budget:{...budget,maxSeconds:remaining()}}});
 if(result.status!=='complete')fail('P6 assembly refused: '+JSON.stringify(result.diagnostics));
 return {result,evidence,bandHz:band,ownership:plan.blocks.map(b=>({id:b.id,ownership:b.ownership})),wallSeconds:(performance.now()-started)/1000};
}

/** Passive Pi discretization of the actual FEM coefficients. Independent
 * signal/return terminals and structural common-potential gauges survive P6.
 * Each cell phase <=0.01rad; this is a discretization guard, not PCB accuracy. */
export function sectionDescriptor(p:any,line:any,frequencies:number[],dc?:{resistanceOhmPerM:number[][];internalInductanceHPerM:number[][]}){
 if(p.frequencyDependent)fail('Dispersive descriptor requires verified causal realization');
 const n=p.capacitanceFPerM.length,length=line.lengthMm*1e-3,C=reciprocalTransport(p.capacitanceFPerM),L=reciprocalTransport(p.inductanceHPerM);
 if(!(length>0)||!Number.isFinite(length)||n<1||n>16)fail('Positive finite propagation length and conductor count required');
 const delay=Math.max(...p.modes.map((m:any)=>m.delaySPerM));
 const cells=Math.max(1,Math.ceil(2*Math.PI*Math.max(...frequencies)*length*delay/.01));if(cells>256||(n+1)*(cells+1)>1024||n*cells>1536)fail('Distributed descriptor cell budget');
 const potentialNodes=Array.from({length:(n+1)*(cells+1)},(_,i)=>String(i));
 const incidence:number[][]=[],inductanceH:number[][]=[],resistanceOhm:number[][]=[];
 const cap=new Map<string,number>();const stamp=(i:number,j:number,v:number)=>{const k=i+','+j;cap.set(k,(cap.get(k)??0)+v);};
 const modes:number[][]=[];
 for(let k=0;k<=cells;k++){
  const base=k*(n+1),ref=base+n,weight=length/cells*(k===0||k===cells ? .5 : 1);
  for(let i=0;i<n;i++)for(let j=0;j<n;j++){const c=C[i][j]*weight;stamp(base+i,base+j,c);stamp(base+i,ref,-c);stamp(ref,base+j,-c);stamp(ref,ref,c);}
  const mode=potentialNodes.map(()=>0);for(let i=0;i<=n;i++)mode[base+i]=1;modes.push(mode);
  if(k<cells){for(let i=0;i<n;i++){
   const dof=k*n+i;incidence.push([base+i,dof,1],[base+n+1+i,dof,-1],[ref,dof,-1],[ref+n+1,dof,1]);
   for(let j=0;j<n;j++){
    const l=(L[i][j]+(dc?.internalInductanceHPerM[i][j]??0))*length/cells;if(l!==0)inductanceH.push([dof,k*n+j,l]);
    const r=(dc?.resistanceOhmPerM[i][j]??0)*length/cells;if(r!==0)resistanceOhm.push([dof,k*n+j,r]);
   }
  }}
 }
 const ends=[0,cells*(n+1)];const portIncidence=ends.flatMap((a,k)=>Array.from({length:n},(_,i)=>[[a+i,k*n+i,1],[a+n,k*n+i,-1]]).flat());
 return {format:'coupled-rlc',referenceTopology:'infinity',potentialNodes,currentDofs:Array.from({length:cells*n},(_,i)=>'loop'+i),incidence,portIncidence,resistanceOhm,inductanceH,capacitanceF:[...cap].filter(([,v])=>v!==0).map(([key,v])=>[...key.split(',').map(Number),v]),conservedPotentialModes:modes,structuralGaugeModes:modes,portTerminals:ends.flatMap(a=>Array.from({length:n},(_,i)=>({positive:{node:String(a+i)},negative:{node:String(a+n)}})))};
}

/** One stored coefficient per reciprocal pair. Only IEEE-roundoff-sized
 * differences from the already audited FEM producer may be transported;
 * no averaging, tolerance repair, eigenvalue clipping or dropped mutuals. */
export function reciprocalTransport(a:number[][]){
 const scale=Math.max(...a.map((row,i)=>Math.abs(row[i])));
 return a.map((row,i)=>row.map((v,j)=>{if(!Number.isFinite(v)||!Number.isFinite(a[j][i])||Math.abs(v-a[j][i])>64*Number.EPSILON*scale)fail('Producer reciprocity exceeds roundoff-only coefficient transport');return a[Math.min(i,j)][Math.max(i,j)];}));
}
