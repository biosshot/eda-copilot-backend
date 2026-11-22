import { channelLossGrams } from './channel-loss.ts';
import {validateChannelAttachments, type ChannelAttachment} from './channel-components.ts';
import {canonical} from './local-model-cache.ts';
import {freeze} from './analysis-dsl/data.ts';
import {selectSectionDomain} from './section-domain.ts';
import type {AnalysisCrossSectionReport,AnalysisCrossSectionRequest,AnalysisFrequencyRequest,AnalysisFrequencyResource} from '../../native/analyze/index.js';
import {createProblemExecutor} from './execute-problem.ts';
import type {ChannelFrequencyReplay} from './segmented-frequency-audit.ts';
import {area,triangles,clipHalfPlane,type Point} from './route-partition.ts';

/** Internal physical cut identity, never a net label. */
export interface ChannelCut {positive:string;negative:string}
/** Start and end physical terminal IDs keyed by the native polygon conductor ID. */
export type SectionEnds = readonly [Readonly<Record<string,string>>,Readonly<Record<string,string>>];
export interface CopperInterval {kind?:'interval';sourcePath:string;startMm:number;stopMm:number}
export interface CopperFragment {kind:'fragment';sourcePath:string;regionId:string;layer:string;domainPolygonMm:Point[];sourcePolygonMm:{outer:Point[];holes:Point[][]};areaMm2:number}
export type CopperOwnership=CopperInterval|CopperFragment;
interface Base {id:string;cuts:ChannelCut[];ownership:CopperOwnership[];bandHz:[number,number];couplingGroup?:string}
export type ChannelBlock = Base & (
 {kind:'section';line:AnalysisCrossSectionRequest['line'];mesh?:AnalysisCrossSectionRequest['mesh'];spatial?:{originMm:Point;direction:Point};sectionEnds?:SectionEnds;dcConductors?:{resistanceOhmPerM:number[][];internalInductanceHPerM:number[][]}}|
 {kind:'local';boardJson:string;geometry:unknown;magneticPatchLevel?:1|2}
);
export interface SegmentedChannel {blocks:ChannelBlock[];ports:{name:string;cut:ChannelCut}[];coverage:CopperOwnership[];attachments?:ChannelAttachment[]}
const fail=(s:string):never=>{throw new Error('CHANNEL_COMPOSITION: '+s);};
const cutKey=(c:ChannelCut)=>JSON.stringify([c.positive,c.negative]);
/** Before FEM, validate original source fragments geometrically and legacy
 * authored route intervals in length units. No area is encoded as a length. */
export function validateChannelOwnership(plan:SegmentedChannel){
 if(!plan.blocks.length||plan.blocks.length>32||new Set(plan.blocks.map(b=>b.id)).size!==plan.blocks.length)fail('1..32 unique blocks required');
 const attachmentBands=validateChannelAttachments(plan.blocks,plan.attachments??[]);
 const groups=new Map<string,string>();
 for(const b of plan.blocks){
  if(b.bandHz.length!==2||b.bandHz.some(v=>!Number.isFinite(v)||v<=0)||b.bandHz[0]>b.bandHz[1])fail('Invalid component band');
  if(b.cuts.length<1||b.cuts.some(c=>!c.positive||!c.negative||c.positive===c.negative))fail('Invalid oriented physical cut');
  if(b.kind==='section'&&b.sectionEnds)resolveSectionPorts(sectionConductors(b.line),{ends:b.sectionEnds,cuts:b.cuts});
  if(b.couplingGroup){if(groups.has(b.couplingGroup))fail('Nonlocal coupling group must be extracted jointly: '+b.couplingGroup);groups.set(b.couplingGroup,b.id);}
 }
 const bySource=(xs:CopperInterval[])=>{const m=new Map<string,CopperInterval[]>();for(const x of xs){if(!x.sourcePath||!Number.isFinite(x.startMm)||!Number.isFinite(x.stopMm)||x.startMm>=x.stopMm)fail('Invalid SI-boundary copper interval');const a=m.get(x.sourcePath)??[];a.push(x);m.set(x.sourcePath,a);}for(const a of m.values())a.sort((x,y)=>x.startMm-y.startMm);return m;};
 const isFragment=(x:CopperOwnership):x is CopperFragment=>x.kind==='fragment';
 const fragments=plan.blocks.flatMap(b=>b.ownership.filter(isFragment)),coverage=plan.coverage.filter(isFragment);
 const key=(f:CopperFragment)=>JSON.stringify([f.sourcePath,f.regionId,f.layer]);
 const clipped=(ring:Point[],domain:Point[])=>{let p=ring;for(let i=0;i<domain.length;i++){const a=domain[i],b=domain[(i+1)%domain.length],n:Point=[b[1]-a[1],a[0]-b[0]];p=clipHalfPlane(p,n,a[0]*n[0]+a[1]*n[1]);}return p;};
 const sourceArea=(f:CopperFragment,other?:Point[])=>triangles(f.domainPolygonMm).reduce((sum,t)=>{
  const regions=other?triangles(other).map(u=>clipped(t,u)):[t];
  return sum+regions.reduce((s,r)=>s+(r.length<3?0:Math.max(0,area(clipped(f.sourcePolygonMm.outer,r))-f.sourcePolygonMm.holes.reduce((a,h)=>a+area(clipped(h,r)),0))),0);
 },0);
 for(const f of [...fragments,...coverage]){
  if(!f.sourcePath||!f.regionId||!f.layer||!Number.isFinite(f.areaMm2)||f.areaMm2<=0||!f.sourcePolygonMm||!f.domainPolygonMm.length)fail('Invalid physical source fragment');
  if(Math.abs(sourceArea(f)-f.areaMm2)>1e-8)fail('Physical source fragment area does not match original polygon');
 }
 for(const set of [fragments,coverage])for(let i=0;i<set.length;i++)for(let j=0;j<i;j++){
  if(key(set[i])===key(set[j])&&sourceArea(set[i],set[j].domainPolygonMm)>1e-8)fail('Overlapping physical source fragments: '+set[i].sourcePath);
 }
 const expectedFragments=new Map<string,CopperFragment[]>();for(const f of coverage){const a=expectedFragments.get(key(f))??[];a.push(f);expectedFragments.set(key(f),a);}
 const actualFragments=new Map<string,CopperFragment[]>();for(const f of fragments){const a=actualFragments.get(key(f))??[];a.push(f);actualFragments.set(key(f),a);}
 if(expectedFragments.size!==actualFragments.size)fail('Physical fragment/source coverage mismatch');
 for(const [id,expectedSet]of expectedFragments){const actual=actualFragments.get(id)??fail('Missing physical source '+id),expectedArea=expectedSet.reduce((s,f)=>s+f.areaMm2,0);
  for(const f of actual){if(expectedSet.some(e=>JSON.stringify(e.sourcePolygonMm)!==JSON.stringify(f.sourcePolygonMm)))fail('Original source polygon changed across ownership fragments');const covered=expectedSet.reduce((s,e)=>s+sourceArea(f,e.domainPolygonMm),0);if(Math.abs(covered-f.areaMm2)>1e-8)fail('Out-of-domain physical source fragment');}
  if(Math.abs(actual.reduce((s,f)=>s+f.areaMm2,0)-expectedArea)>1e-8)fail('Uncovered physical copper: '+id);
 }
 const owned=bySource(plan.blocks.flatMap(b=>b.ownership.filter((x):x is CopperInterval=>!isFragment(x)))),expected=bySource(plan.coverage.filter((x):x is CopperInterval=>!isFragment(x)));
 if(owned.size!==expected.size)fail('Owned/source coverage mismatch');
 for(const [source,spans]of expected){const actual=owned.get(source)??fail('Missing source '+source);let j=0;
  for(const span of spans){let x=span.startMm;while(j<actual.length&&actual[j].startMm<span.stopMm){const a=actual[j++];if(Math.abs(a.startMm-x)>1e-9||a.stopMm>span.stopMm+1e-9)fail('Copper overlap, gap or out-of-domain ownership: '+source);x=a.stopMm;}if(Math.abs(x-span.stopMm)>1e-9)fail('Uncovered copper: '+source);}
  if(j!==actual.length)fail('Extra copper ownership: '+source);
 }
 const bands=[...plan.blocks.map(b=>b.bandHz),...attachmentBands];
 const band:[number,number]=[Math.max(...bands.map(b=>b[0])),Math.min(...bands.map(b=>b[1]))];if(band[0]>band[1])fail('No common physical band');return band;
}
/** Reuse the original section/cache identity unless the generated vertex cap
 * prevents refinement. Only the retry's vertex and remaining time caps change. */
export function executeSegmentedSection(
 request: AnalysisCrossSectionRequest,
 execute: (request: AnalysisCrossSectionRequest) => AnalysisCrossSectionReport,
 remaining: () => number,
 expandAutomaticSectionMesh = false,
) {
 const result=execute(request);
 const line=request.line as {kind?:string;geometry?:{maxNodes?:number;maxMemoryMib?:number}}|null;
 if(result.status!=='rejected'||!expandAutomaticSectionMesh||line?.kind!=='polygon-section')return {result};
 const errors=result.diagnostics.filter(d=>d.severity==='error');
 if(errors.length!==1||errors[0].code!=='SECTION_MESH_LIMIT'||errors[0].message!=='SECTION_MESH_LIMIT: polygon refinement cap')return {result};

 const geometry=line.geometry;
 const before=geometry?.maxNodes, geometryMemoryMiB=geometry?.maxMemoryMib;
 if(typeof before!=='number'||!Number.isSafeInteger(before)||typeof geometryMemoryMiB!=='number')return {result};
 const effectiveMemoryMiB=Math.min(geometryMemoryMiB,request.budget?.maxMemoryMiB??geometryMemoryMiB);
 // The native field plan reserves 2048 B/node, 512 B/triangle, and 65536 B
 // fixed overhead. A planar mesh has fewer than two triangles per node.
 const memoryNodeCap=Math.floor((effectiveMemoryMiB*1024**2-65536)/3072);
 const after=Math.min(200000,memoryNodeCap,request.mesh?.maxNodes??Infinity,Math.floor((request.mesh?.maxTriangles??Infinity)/2));
 if(!Number.isFinite(effectiveMemoryMiB)||after<=before||!Number.isSafeInteger(after))return {result};

 const meshRetry={reason:errors[0].message,maxNodesBefore:before,maxNodesAfter:after,effectiveMemoryMiB};
 const retry=execute({...request,line:{...line,geometry:{...geometry,maxNodes:after}},budget:{...request.budget,maxSeconds:remaining()}});
 return {result:retry,meshRetry};
}

/** Real section/local producers, direct cross-block magnetostatics and P6.
 * Legacy authored plans without spatial coordinates retain block-only assembly;
 * an automatic plan with incomplete coordinates is never silently demoted. */
export function solveSegmentedChannel(plan:SegmentedChannel,frequencies:number[],budget={maxSeconds:300,maxMemoryMiB:2048},options:{expandAutomaticSectionMesh?:boolean;captureFrequencyReplay?:(evaluate:ChannelFrequencyReplay)=>void}={}){
 const band=validateChannelOwnership(plan);if(!frequencies.length||frequencies.some(f=>!Number.isFinite(f)||f<band[0]||f>band[1]))fail('Requested axis outside common physical band');
 const execute=createProblemExecutor(true),started=performance.now();const remaining=()=>{const s=budget.maxSeconds-(performance.now()-started)/1000;if(s<=0)fail('Shared channel deadline');return s;};
 const instances:{id:string;model:AnalysisFrequencyResource;ports:string[]}[]=[],evidence:unknown[]=[],magneticBlocks:any[]=[];
 for(const block of plan.blocks){const memoryBefore=process.memoryUsage(),blockStarted=performance.now();let model:AnalysisFrequencyResource;
  if(process.env.P7_BLOCK_LIFECYCLE==='1')process.stderr.write(JSON.stringify({event:'P7_BLOCK_BEGIN',id:block.id,kind:block.kind,memory:memoryBefore})+'\n');
  if(block.kind==='section'){
   const terminals=block.sectionEnds&&{ends:block.sectionEnds,cuts:block.cuts};
   const {result:r,meshRetry}=executeSegmentedSection(
    {line:block.line,mesh:block.mesh,frequencies,budget:{...budget,maxSeconds:remaining()}},
    request=>execute({kind:'cross-section',request}),remaining,options.expandAutomaticSectionMesh,
   );
   if(meshRetry&&process.env.P7_BLOCK_LIFECYCLE==='1')process.stderr.write(JSON.stringify({event:'P7_SECTION_MESH_RETRY',id:block.id,...meshRetry,status:r.status})+'\n');
   if(r.status!=='complete'||!r.network)fail('Section '+block.id+' refused: '+JSON.stringify(r.diagnostics)+(meshRetry?' after mesh budget retry '+JSON.stringify(meshRetry):''));
   if((block.line as {kind?:string})?.kind==='polygon-section'){
    const expected=sectionConductors(block.line).map(m=>m.id),gauge=expected.pop();
    const actual=r.conductorOrder;
    if(!Array.isArray(actual)||actual.length!==expected.length||actual.some((id,i)=>id!==expected[i])||r.reference!==gauge)fail('Native section coefficient order does not match physical conductors: '+block.id);
   }
   const data=sectionDescriptor(r.parameters!,block.line,frequencies,block.dcConductors,terminals);
   model={id:block.id,kind:'network',ports:data.portTerminals.map((_,i)=>'port-'+i),data};
   if(block.spatial)magneticBlocks.push(sectionMagneticBlock(block.id,block.line,r.parameters,frequencies,block.spatial));
   evidence.push({id:block.id,kind:block.kind,parameters:r.parameters,sectionRealization:{type:'physical RLC Pi',cellPhaseLimitRad:.01,dcConductors:block.dcConductors??null,conductorOrder:r.conductorOrder,voltageGaugeConductor:r.reference,ends:block.sectionEnds,units:{length:'mm to m at boundary',R:'Ohm/m',L:'H/m',C:'F/m'}},cache:r.cache,resources:r.resources,...(meshRetry&&{meshRetry})});
  }else{
   const r=execute({kind:'local-extraction',request:{boardJson:block.boardJson,geometry:block.geometry,remainingSeconds:remaining(),bandHz:[1000,100000],frequenciesHz:frequencies,magneticPatchLevel:block.magneticPatchLevel,observablesOnly:true}});
   const ports=(r.prepared as any).request.ports.map((_:unknown,i:number)=>'port-'+i);
   model={id:block.id,kind:'network',ports,data:r.model};magneticBlocks.push(localMagneticBlock(block.id,r.prepared));evidence.push({id:block.id,kind:block.kind,cache:r.cache,stageCounts:r.stageCounts,resources:r.resources,nativePreparation:{stageSeconds:(r.prepared as any).stageSeconds,potentialCount:(r.prepared as any).potentialCount,currentCount:(r.prepared as any).currentCount,plannedBytes:(r.prepared as any).plannedBytes,fieldOperator:(r.prepared as any).candidate?.fieldOperator,trainingJobs:(r.prepared as any).stageSeconds?.trainingJobs}});
  }
  const latest=evidence.at(-1) as any;latest.lifecycle={activeHeavyBlocks:1,concurrency:'synchronous sequential extraction',memoryBefore,memoryAfter:process.memoryUsage(),processHighWaterRssBytes:process.resourceUsage().maxRSS*1024,wallSeconds:(performance.now()-blockStarted)/1000,releaseEvidence:'Native full factors have preparation-scope ownership; returned reduced DTO/cache may retain field/material/training data. RSS includes allocator retention and is not live factor bytes.'};
  if(process.env.P7_BLOCK_LIFECYCLE==='1')process.stderr.write(JSON.stringify({event:'P7_BLOCK_END',id:block.id,kind:block.kind,lifecycle:latest.lifecycle})+'\n');
  if(model.ports.length!==block.cuts.length)fail('Model/cut port count mismatch: '+block.id);
  instances.push({id:block.id,model,ports:model.ports});
 }
 if(magneticBlocks.length&&magneticBlocks.length!==plan.blocks.length)fail('Incomplete spatial metadata for automatic mutual-inductance assembly');
 const instanceInductanceH:{a:string;b:string;localA:number;localB:number;valueH:number}[]=[];let magneticCoupling:any=null;
 if(magneticBlocks.length>1||magneticBlocks.some(b=>b.directSelf)){
  magneticCoupling=execute({kind:'channel-magnetic',request:{blocks:magneticBlocks,order:16,budget:{...budget,maxSeconds:remaining()}}});
  for(const c of magneticCoupling.couplings)for(let i=0;i<c.matrixH.length;i++)for(let j=0;j<c.matrixH[i].length;j++)instanceInductanceH.push({a:c.a,b:c.b,localA:i,localB:j,valueH:c.matrixH[i][j]});
  // A finite section needs the same free-space volume operator for self and
  // mutual terms. Its uniform-current volume integral also includes the DC
  // internal contribution, so replace rather than append the 2D/DC entries.
  for(const self of magneticCoupling.selfMatrices??[]){
   const instance=instances.find(v=>v.id===self.id)??fail('Direct self references an unknown block: '+self.id),data=instance.model.data as any,n=data.currentDofs?.length;
   if(data.format!=='coupled-rlc'||!Number.isInteger(n)||self.matrixH.length!==n||self.matrixH.some((row:any)=>!Array.isArray(row)||row.length!==n||row.some((v:any)=>!Number.isFinite(v))))fail('Invalid direct section self matrix: '+self.id);
   data.inductanceH=self.matrixH.flatMap((row:number[],i:number)=>row.map((value,j)=>[i,j,value]));
  }
 }
 // Package effects belong only to the supplied network. Attachments own
 // no copper fragments and do not enter the geometric mutual operator.
 for(const attachment of plan.attachments??[]){
  const {id,model,cut}=attachment;
  instances.push({id,model,ports:model.ports});
  evidence.push({id,kind:'component-attachment',cut,model,spatialOwnership:'supplied model only'});
 }
 const {connections,connectionContacts,ports}=channelTopology(plan);
 // Every descriptor retains distinct physical signal and return cut terminals.
 // P6 handles the complete branched multiport matrix, including local RLC.
 const hasComponents=Boolean(plan.attachments?.length);
 const evaluate=(axis:number[],maxSeconds:number,recoverLoss=false)=>execute({kind:'frequency-network',request:{instances,connections,connectionContacts,ports,frequencies:axis,representation:'S',referenceOhm:ports.map(()=>50),instanceInductanceH,...(recoverLoss&&{recoverInstanceLoss:true}),budget:{...budget,maxSeconds}}});
 const result=evaluate(frequencies,remaining(),hasComponents);
 if(result.status!=='complete')fail('P6 assembly refused: '+JSON.stringify(result.diagnostics));
 options.captureFrequencyReplay?.((frequencyHz,maxSeconds)=>evaluate([frequencyHz],maxSeconds));
 const lossGrams=hasComponents?channelLossGrams(result.loadedInstanceLoss,frequencies,ports.length,
  plan.blocks.map(block=>block.id),plan.attachments!.map(attachment=>attachment.id)):undefined;
 return {result:lossGrams?{...result,channelLossGrams:lossGrams}:result,evidence,bandHz:band,magneticCoupling,
  ...(lossGrams&&{lossRecovery:{
   total:'Passive power balance includes captured copper and supplied component losses',
   copperOnly:'Native matched R loss Gram of original copper descriptor instances',
   componentOnly:'Native matched R loss Gram of supplied package descriptor instances',
   regionalCopper:'Unverified',
  }}),
  ownership:plan.blocks.map(b=>({id:b.id,ownership:b.ownership})),wallSeconds:(performance.now()-started)/1000};
}

/** Physical face topology shared by extraction and diagnostic replay. */
export function channelTopology(plan:SegmentedChannel){
 const cuts=new Map<string,number[]>();let count=0;
 for(const block of plan.blocks)for(const cut of block.cuts){const k=cutKey(cut),a=cuts.get(k)??[];a.push(count++);cuts.set(k,a);}
 const connections:[number,number][]=[],connectionContacts:NonNullable<AnalysisFrequencyRequest['connectionContacts']>=[],welds=new Map<string,number>();
 const terminalGroups=new Map<string,{port:number;contact:'positive'|'negative'}[]>();let offset=0;
 // The block ports occupy the original indices. Private component ports are
 // appended and weld each pin independently to its exposed copper contact.
 const groups=[...plan.blocks.map(block=>block.cuts),...(plan.attachments??[]).map(attachment=>[attachment.cut])];
 for(const group of groups)for(const cut of group){for(const contact of ['positive','negative'] as const){const id=cut[contact],a=terminalGroups.get(id)??[];a.push({port:offset,contact});terminalGroups.set(id,a);}offset++;}
 for(const [first,...others] of terminalGroups.values())for(const other of others){
  // Occurrences were visited in port order. A contact may change polarity
  // across cuts without changing the physical conductor being welded.
  const key=[first.port,other.port].join(',');
  const bit=first.contact==='positive'?(other.contact==='positive'?1:4):(other.contact==='negative'?2:8);
  welds.set(key,(welds.get(key)??0)|bit);
 }
 const add=(pair:[number,number],contacts:typeof connectionContacts[number])=>{connections.push(pair);connectionContacts.push(contacts);};
 for(const [key,mask]of welds){
  const pair=key.split(',').map(Number) as [number,number];
  const straight=mask&3;
  if(straight)add(pair,straight===3?'both':straight===1?'positive':'negative');
  if(mask&4)add(pair,'positive-negative');
  if(mask&8)add(pair,'negative-positive');
 }
 const ports=plan.ports.map(p=>({name:p.name,index:cuts.get(cutKey(p.cut))?.[0]??fail('External cut absent: '+p.name)}));
 return {connections,connectionContacts,ports};
}

function sectionLayout(p:any,line:any,frequencies:number[],minimumCells=1){
 if(p.frequencyDependent||line.geometry?.loss)fail('Dispersive descriptor requires verified causal realization');
 const n=p.capacitanceFPerM.length,length=line.lengthMm*1e-3;
 if(!(length>0)||!Number.isFinite(length)||n<1||n>16)fail('Positive finite propagation length and conductor count required');
 const delay=Math.max(...p.modes.map((m:any)=>m.delaySPerM));
 const cells=Math.max(1,minimumCells,Math.ceil(2*Math.PI*Math.max(...frequencies)*length*delay/.01));if(cells>256||(n+1)*(cells+1)>1024||n*cells>1536)fail('Distributed descriptor cell budget');
 return {n,length,cells,cellLength:length/cells};
}

interface SectionConductor {
 id:string;
 role:'signal'|'reference';
 contour:{outerM:Point[];holesM?:Point[][]};
}

function sectionConductors(line:unknown):SectionConductor[]{
 const polygon=line as {kind?:string;geometry?:{conductors?:SectionConductor[]}};
 if(polygon?.kind!=='polygon-section'||!Array.isArray(polygon.geometry?.conductors))fail('Physical section mapping requires an exact polygon section');
 const conductors=polygon.geometry!.conductors!;
 const independent=conductors.filter(m=>m.role==='signal'),gauge=conductors.filter(m=>m.role==='reference');
 if(gauge.length!==1||independent.length<1||independent.length+1!==conductors.length)fail('Section requires one mathematical gauge and independent coordinates for all other conductors');
 // The native electric operator orders all non-gauge conductors in input order.
 return [...independent,gauge[0]];
}

/** Columns are e_i - e_g. The same entries lift relative capacitance to
 * physical potentials and map longitudinal loop currents to every conductor. */
function sectionCurrentBasis(n:number){
 return Array.from({length:n},(_,i)=>[[i,1],[n,-1]] as const);
}

function resolveSectionPorts(conductors:SectionConductor[],terminals:{ends:SectionEnds;cuts:readonly ChannelCut[]}){
 if(terminals.ends.length!==2)fail('Section requires start and end terminal maps');
 const ids=conductors.map(m=>m.id);
 if(ids.some(id=>!id)||new Set(ids).size!==ids.length)fail('Unique physical polygon conductor IDs required');
 const contacts=new Map<string,{end:number;conductor:number}>();
 for(const [end,mapping] of terminals.ends.entries()){
  if(Object.keys(mapping).length!==ids.length)fail('Section end must map every polygon conductor exactly once');
  for(const [conductor,id] of ids.entries()){
   const terminal=Object.hasOwn(mapping,id)&&mapping[id];
   if(typeof terminal!=='string'||!terminal||contacts.has(terminal))return fail('Section end needs a distinct physical terminal for every conductor');
   contacts.set(terminal,{end,conductor});
  }
 }
 return terminals.cuts.map(cut=>{
  const positive=contacts.get(cut.positive),negative=contacts.get(cut.negative);
  if(!positive||!negative||positive.end!==negative.end||positive.conductor===negative.conductor)return fail('Section port must select distinct mapped conductors on the same end');
  return {end:positive.end,positive:positive.conductor,negative:negative.conductor};
 });
}
/** Native local current basis in board coordinates. The mesh XY origin is ROI
 * relative; Z is already relative to the common board stack datum. */
export function localMagneticBlock(id:string,p:any){
 const candidate=p?.candidate,dofs=candidate?.model?.currentDofs?.length,basis=candidate?.currentBasis,nodes=p?.nodesM,cells=p?.copperCells,density=p?.cellDensityPerAmpM2,origin=p?.originMm;
 if(!Number.isInteger(dofs)||dofs<1||!Array.isArray(basis)||basis.length!==dofs||!Array.isArray(nodes)||!Array.isArray(cells)||!cells.length||!Array.isArray(density)||density.length!==cells.length||!Array.isArray(origin)||origin.length!==2||origin.some((v:number)=>!Number.isFinite(v)))fail('Incomplete local magnetic current basis: '+id);
 return {id,dofs,cells:cells.map((t:number[],c:number)=>{
  if(t.length!==4||t.some(i=>!Number.isInteger(i)||i<0||i>=nodes.length)||!Number.isFinite(density[c]))fail('Invalid local magnetic cell: '+id);
  const verticesM=t.map(i=>{const q=nodes[i];if(!Array.isArray(q)||q.length!==3||q.some((v:number)=>!Number.isFinite(v)))fail('Invalid local magnetic vertex: '+id);return [q[0]+origin[0]*1e-3,q[1]+origin[1]*1e-3,q[2]];});
  const currentsPerAmpM2=basis.map((row:number[],d:number)=>{if(!Array.isArray(row)||row.length!==3*cells.length)fail('Local current basis shape mismatch: '+id);return [0,1,2].map(axis=>{const v=density[c]*row[3*c+axis];if(!Number.isFinite(v))fail('Nonfinite local magnetic basis: '+id);return v;});});
  return {verticesM,currentsPerAmpM2};
 })};
}
/** Constant longitudinal J on the exact triangular-prism decomposition of each
 * section conductor/cell in the same current basis as the descriptor. */
export function sectionMagneticBlock(id:string,line:any,p:any,frequencies:number[],spatial:{originMm:Point;direction:Point}){
 const {n,length,cells,cellLength}=sectionLayout(p,line,frequencies);
 if(line.kind!=='polygon-section')fail('Spatial mutuals require an exact polygon section: '+id);
 const norm=Math.hypot(...spatial.direction),origin=spatial.originMm,d=spatial.direction;
 if(origin.length!==2||d.length!==2||[...origin,...d].some(v=>!Number.isFinite(v))||Math.abs(norm-1)>1e-12)fail('Invalid section spatial frame: '+id);
 const tangent:Point=[-d[1],d[0]],conductors=sectionConductors(line),basis=sectionCurrentBasis(n);
 if(conductors.length!==n+1)fail('Section magnetic basis dimension mismatch: '+id);
 const out:any[]=[];
 for(const [conductorIndex,metal]of conductors.entries()){
  const contour=metal.contour,ring=contour?.outerM;
  if(!Array.isArray(ring)||ring.length<3||contour.holesM?.length)fail('Section magnetic support requires a simple contour without holes: '+id);
  const mesh=triangles(ring),conductorArea=mesh.reduce((s,t)=>s+area(t),0);if(!(conductorArea>0)||!Number.isFinite(conductorArea))fail('Nonpositive section magnetic area: '+id);
  const density=basis.map(column=>(column.find(([index])=>index===conductorIndex)?.[1]??0)/conductorArea);
  for(let k=0;k<cells;k++)for(const triangle of mesh){
   const longitudinal=[-length/2+k*cellLength,-length/2+(k+1)*cellLength];
   // sectionLayout and outerM are already SI; only the authored origin is mm.
   const prism=triangle.map(q=>[0,1].map(z=>{const xy=[origin[0]*1e-3+d[0]*longitudinal[z]+tangent[0]*q[0],origin[1]*1e-3+d[1]*longitudinal[z]+tangent[1]*q[0]];return [xy[0],xy[1],q[1]];}));
   const b=[prism[0][0],prism[1][0],prism[2][0]],t=[prism[0][1],prism[1][1],prism[2][1]];
   for(const verticesM of [[b[0],b[1],b[2],t[0]],[b[1],b[2],t[0],t[1]],[b[2],t[0],t[1],t[2]]]){
    const currentsPerAmpM2=Array.from({length:n*cells},(_,dof)=>{
     const scale=Math.floor(dof/n)===k?density[dof%n]:0;return [d[0]*scale,d[1]*scale,0];
    });out.push({verticesM,currentsPerAmpM2});
   }
  }
 }
 return {id,dofs:n*cells,directSelf:true,cells:out};
}

/** Passive Pi discretization of the actual FEM coefficients. Independent
 * signal/return terminals and structural common-potential gauges survive P6.
 * Each cell phase <=0.01rad; this is a discretization guard, not PCB accuracy. */
export function sectionDescriptor(p:any,line:any,frequencies:number[],dc?:{resistanceOhmPerM:number[][];internalInductanceHPerM:number[][]},terminals?:{ends:SectionEnds;cuts:readonly ChannelCut[]}){
 const layout=sectionLayout(p,line,frequencies),C=reciprocalTransport(p.capacitanceFPerM),L=reciprocalTransport(p.inductanceHPerM);
 const conductors=line.kind==='polygon-section'?sectionConductors(line):undefined;
 const totalL=dc?L.map((row,i)=>row.map((v,j)=>v+(dc.internalInductanceHPerM[i][j]??0))):L;
 return stampSectionDescriptor(layout,C,totalL,dc?.resistanceOhmPerM,conductors,terminals);
}

function stampSectionDescriptor(
 {n,length,cells}:ReturnType<typeof sectionLayout>,C:number[][],L:number[][],R:number[][]|undefined,
 conductors:SectionConductor[]|undefined,terminals?:{ends:SectionEnds;cuts:readonly ChannelCut[]},
){
 const basis=sectionCurrentBasis(n);
 if(conductors&&conductors.length!==n+1)fail('Section electric basis dimension mismatch');
 const potentialNodes=Array.from({length:(n+1)*(cells+1)},(_,i)=>String(i));
 const incidence:number[][]=[],inductanceH:number[][]=[],resistanceOhm:number[][]=[];
 const cap=new Map<string,number>();const stamp=(i:number,j:number,v:number)=>{const k=i+','+j;cap.set(k,(cap.get(k)??0)+v);};
 const modes:number[][]=[];
 for(let k=0;k<=cells;k++){
  const base=k*(n+1),weight=length/cells*(k===0||k===cells ? .5 : 1);
  for(let i=0;i<n;i++)for(let j=0;j<n;j++)for(const [a,u] of basis[i])for(const [b,v] of basis[j])stamp(base+a,base+b,u*v*C[i][j]*weight);
  const mode=potentialNodes.map(()=>0);for(let i=0;i<=n;i++)mode[base+i]=1;modes.push(mode);
  if(k<cells){for(let i=0;i<n;i++){
   const dof=k*n+i;for(const [conductor,sign] of basis[i])incidence.push([base+conductor,dof,sign],[base+n+1+conductor,dof,-sign]);
   for(let j=0;j<n;j++){
    const l=L[i][j]*length/cells;if(l!==0)inductanceH.push([dof,k*n+j,l]);
    const r=(R?.[i][j]??0)*length/cells;if(r!==0)resistanceOhm.push([dof,k*n+j,r]);
   }
  }}
 }
 const pairs=terminals?resolveSectionPorts(conductors??fail('Explicit section ends require polygon conductor IDs'),terminals):[0,1].flatMap(end=>basis.map((_,i)=>({end,positive:i,negative:n})));
 const ports=pairs.map(({end,positive,negative})=>{const offset=end*cells*(n+1);return [offset+positive,offset+negative];});
 const portIncidence=ports.flatMap(([positive,negative],i)=>[[positive,i,1],[negative,i,-1]]);
 return {format:'coupled-rlc',referenceTopology:'infinity',potentialNodes,currentDofs:Array.from({length:cells*n},(_,i)=>'loop'+i),incidence,portIncidence,resistanceOhm,inductanceH,capacitanceF:[...cap].filter(([,v])=>v!==0).map(([key,v])=>[...key.split(',').map(Number),v]),conservedPotentialModes:modes,structuralGaugeModes:modes,portTerminals:ports.map(([positive,negative])=>({positive:{node:String(positive)},negative:{node:String(negative)}}))};
}

type PolygonSectionLine = {
 kind:'polygon-section'; lengthMm:number; geometry:{conductors:SectionConductor[];loss?:unknown};
};
type TransitionSample = {
 frequencyHz:number;
 conductorOrder:readonly string[];
 signalIds:readonly string[];
 referenceId:string;
 resistanceOhmPerM:readonly (readonly number[])[];
 totalInductanceHPerM:readonly (readonly number[])[];
};
type TransitionElectric = {
 conductorOrder:readonly string[];
 reference:string;
 capacitanceFPerM:readonly (readonly number[])[];
 modes:readonly {delaySPerM:number}[];
 frequencyDependent?:boolean;
};

function checkedLoopMatrix(matrix:readonly (readonly number[])[], n:number, name:string){
 if(!Array.isArray(matrix)||matrix.length!==n||matrix.some(row=>!Array.isArray(row)||row.length!==n))
  fail('Transition '+name+' must match the physical loop basis');
 return reciprocalTransport(matrix.map(row=>[...row]));
}

/** Internal proof for one isolated section and one checked frequency. The
 * variational L already owns the complete 2D self field. Spatial assemblies
 * need a different basis and common self/mutual operator. */
export function sectionTransitionFrequencyRequest(input:{
 id:string; line:PolygonSectionLine; electric:TransitionElectric; sample:TransitionSample;
 terminals:{ends:SectionEnds;cuts:readonly ChannelCut[]};
 budget?:AnalysisFrequencyRequest['budget'];
 spatial?:never; directSelf?:never; instanceInductanceH?:never; channelMagnetic?:never;
}):AnalysisFrequencyRequest{
 if('spatial' in input||'directSelf' in input||'instanceInductanceH' in input||'channelMagnetic' in input)
  fail('Isolated transition rejects channel magnetic, direct self and mutual terms');
 const {id,line,electric,sample,terminals,budget}=input;
 const {frequencyHz}=sample,omega=2*Math.PI*frequencyHz;
 if(!id.trim()||!Number.isFinite(omega)||omega<=0)
  fail('Isolated transition requires an ID and one positive finite exact frequency');
 if(terminals.cuts.length<1||terminals.cuts.length>16||new Set(terminals.cuts.map(cutKey)).size!==terminals.cuts.length)
  fail('Isolated transition requires 1..16 distinct oriented physical cuts');
 const conductors=sectionConductors(line);
 const physicalIds=new Set(line.geometry.conductors.map(m=>m.id));
 const signalIds=conductors.slice(0,-1).map(m=>m.id),referenceId=conductors.at(-1)!.id;
 const same=(a:readonly string[],b:readonly string[])=>a.length===b.length&&a.every((v,i)=>v===b[i]);
 // The full producer order labels per-conductor loss. Only signalIds
 // determines the axes of the loop R/L Grams.
 const declaredIds=new Set(sample.conductorOrder);
 if(physicalIds.size!==conductors.length||sample.conductorOrder.length!==physicalIds.size||
    declaredIds.size!==physicalIds.size||[...declaredIds].some(id=>!physicalIds.has(id)))
  fail('Transition conductor IDs must be a unique bijection with the physical polygon');
 if(!same(sample.signalIds,signalIds)||sample.referenceId!==referenceId||
    !same(electric.conductorOrder,signalIds)||electric.reference!==referenceId)
  fail('Transition electric and magnetic coefficients must share exact physical conductor and reference order');
 const n=signalIds.length;
 if(electric.modes.length!==n||electric.modes.some(({delaySPerM})=>!Number.isFinite(delaySPerM)||delaySPerM<=0))
  fail('Transition requires valid electric mode delays for the physical loop basis');
 const C=checkedLoopMatrix(electric.capacitanceFPerM,n,'capacitance');
 const R=checkedLoopMatrix(sample.resistanceOhmPerM,n,'resistance');
 const L=checkedLoopMatrix(sample.totalInductanceHPerM,n,'total inductance');
 // For a general coupled line γ² is an eigenvalue of ZY. The infinity
 // norm product bounds |γ| and thus also phase per cell. Keep the legacy
 // electric delay guard as an independent lower bound on cell count.
 const zNorm=Math.max(...R.map((row,i)=>row.reduce((sum,v,j)=>sum+Math.hypot(v,omega*L[i][j]),0)));
 const yNorm=omega*Math.max(...C.map(row=>row.reduce((sum,v)=>sum+Math.abs(v),0)));
 const complexCells=Math.ceil(Math.sqrt(zNorm)*Math.sqrt(yNorm)*(line.lengthMm*1e-3)/.01);
 if(!Number.isFinite(complexCells))fail('Nonfinite transition propagation bound');
 const layout=sectionLayout(electric,line,[frequencyHz],complexCells);
 const data=stampSectionDescriptor(layout,C,L,R,conductors,terminals);
 const ports=data.portTerminals.map((_,i)=>'port-'+i);
 const model:AnalysisFrequencyResource={
  id,kind:'network',ports,data,analyses:['networkParameters'],
  source:'Isolated finite-conductivity 2D section at one checked frequency',
  validity:{status:'conditional',reason:'Exact-frequency section proof; physical and band accuracy remain unverified',
   domain:{bandHz:[frequencyHz,frequencyHz],sectionTransition:'isolated-exact-frequency',conductorLossOrder:[...sample.conductorOrder]}},
 };
 return freeze({instances:[{id,model,ports}],connections:[],ports:ports.map((name,index)=>({name,index})),
  frequencies:[frequencyHz],representation:'S',referenceOhm:ports.map(()=>50),...(budget&&{budget:{...budget}})});
}


/** One stored coefficient per reciprocal pair. Only IEEE-roundoff-sized
 * differences from the already audited FEM producer may be transported;
 * no averaging, tolerance repair, eigenvalue clipping or dropped mutuals. */
export function reciprocalTransport(a:number[][]){
 const scale=Math.max(...a.map((row,i)=>Math.abs(row[i])));
 return a.map((row,i)=>row.map((v,j)=>{if(!Number.isFinite(v)||!Number.isFinite(a[j][i])||Math.abs(v-a[j][i])>64*Number.EPSILON*scale)fail('Producer reciprocity exceeds roundoff-only coefficient transport');return a[Math.min(i,j)][Math.max(i,j)];}));
}

/** Coefficients-only board path for B2 candidates. Deliberately separate from
 * sectionDescriptor: no cropped-width DC resistance enters channel assembly. */
export function extractWideBoardSectionCoefficients(input:{boardJson:string;stack:unknown;slices:{axis:import('./section-domain.ts').SectionDomainStep['axis'];slice:unknown;maxTriangleAreaM2:number}[];frequencies:number[];lengthMm:number},budget={maxSeconds:120,maxMemoryMiB:1024}){
 if(!Number.isFinite(input.lengthMm)||input.lengthMm<=0)fail('Positive finite section length required');
 if(input.slices.length<4||input.slices.length>10)fail('Bounded board-section refinement plan required');
 const execute=createProblemExecutor(true),started=performance.now();
 const anchor=(slice:any)=>canonical([slice.atMm,slice.transverseUnit,slice.referenceLayer,slice.referenceSourcePath,slice.referenceXMm]);
 const physicalSlice=anchor(input.slices[0].slice);if(input.slices.some(s=>anchor(s.slice)!==physicalSlice))fail('Section position/reference contact changed');
 const sliceCache=new Map<string,any>();let boardSliceCalls=0;
 const slices=input.slices.map(s=>{
  if((performance.now()-started)/1000>=budget.maxSeconds)fail('Board adapter shared deadline');
  const key=canonical(s.slice);let r:any=sliceCache.get(key);
  if(!r){r=execute({kind:'board-section',boardJson:input.boardJson,stack:input.stack,slice:s.slice});sliceCache.set(key,r);boardSliceCalls++;}
  if(!r.geometry||!r.sourceMapping)fail('Incomplete authoritative board section');
  const conductorIdentity=r.sourceMapping.map((m:any)=>({id:m.conductorId,role:m.role,layer:m.layer,sourcePaths:m.sourcePaths,sourceRegionIds:m.sourceRegionIds}));
  return {mapping:r.sourceMapping,axis:s.axis,physicalIdentity:{boardJson:input.boardJson,stack:input.stack,frequencies:input.frequencies,lengthMm:input.lengthMm,boundary:'native natural exterior; one physical reference'},conductorIdentity,request:{line:{kind:'polygon-section' as const,geometry:{...r.geometry,maxTriangleAreaM2:s.maxTriangleAreaM2},lengthMm:input.lengthMm},frequencies:input.frequencies}};
 });
 // Width holdout uses the same full field/material domain as its final partner.
 for(let i=1;i<slices.length;i++)if(slices[i].axis==='physical-width'){const a=slices[i-1].request.line.geometry as any,b=slices[i].request.line.geometry as any;a.domain=structuredClone(b.domain);a.dielectrics=structuredClone(b.dielectrics);}
 const result=selectSectionDomain(slices,{...budget,maxSeconds:budget.maxSeconds-(performance.now()-started)/1000},execute);
 return {...result,sourceMapping:slices.at(-1)?.mapping,boardSliceCalls,boardAdapterWallSeconds:(performance.now()-started)/1000};
}
