import {randomUUID,createHash} from 'node:crypto';
import type {AnalysisFrequencyRequest} from '../../../native/analyze/index.js';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {createProblemExecutor} from '../execute-problem.ts';
import {object} from './descriptions.ts';
import {AnalysisDslError,invalid} from './schema.ts';
import {json} from './data.ts';
import {networkResource,declaredNetworkBand} from './frequency-model.ts';
/** Retain applicability while linking generated ancestor history by hash.
 * Reusing two copies of a result must not duplicate its entire history twice. */
function compactValidity(value:unknown):unknown{
 if(!object(value)||!object(value.domain)||!Array.isArray(value.domain.provenance)||!value.domain.provenance.every(p=>object(p)&&typeof p.sha256==='string'&&typeof p.resourceId==='string'))return value;
 const domain:Record<string,unknown>={...value.domain,provenance:{sha256:createHash('sha256').update(JSON.stringify(json(value.domain.provenance))).digest('hex')}};
 if(domain.sourceValidity!==undefined)domain.sourceValidity={sha256:createHash('sha256').update(JSON.stringify(json(domain.sourceValidity))).digest('hex')};
 return {...value,domain};
}
/** Flatten descriptor instances, never Resource ids: repeated models are separate
 * devices, repeated handles (including nested overlap) are errors. No solver here. */
export function frequencyTopology(task:AnalysisTask<'networkParameters'|'si'>){
 const descriptors=new Map(task.descriptors.filter((d):d is Record<string,T.Json>=>object(d)).map(d=>[String(d.id),d]));
 const instances:AnalysisFrequencyRequest['instances']=[],connections:[number,number][]=[],seen=new Set<string>();const connectionContacts:('both'|'positive'|'negative')[]=[];let explicitContacts=false;let count=0;
 function visit(id:string,depth=0):Map<string,number>{
  if(depth>32)throw new AnalysisDslError('NETWORK_BUDGET','Nested topology depth exceeds 32');
  const d=descriptors.get(id);if(!d||!object(d.input))invalid('Unknown network descriptor');
  if(d.kind==='network'){
   if(seen.has(id))invalid('Repeated instance handle / overlapping nested topology');seen.add(id);
   const model=networkResource(d.input.model),mapping=d.input.ports;
   if(model.data.format==='physical-port-samples'&&(depth!==0||task.kind!=='networkParameters'))
    throw new AnalysisDslError('NETWORK_PHYSICAL_SAMPLE_TOPOLOGY','Physical exact-grid samples require standalone frequency replay; a physical model is required for connections or SI');
   if(!object(mapping)||Object.values(mapping).length!==model.ports.length||new Set(Object.values(mapping)).size!==model.ports.length||Object.values(mapping).some(p=>!model.ports.includes(p as string)))invalid('Incomplete/unknown port mapping');
   const names=model.ports.map(p=>Object.keys(mapping).find(k=>mapping[k]===p)!);
   const start=count;count+=names.length;if(count>128||instances.length>=32)throw new AnalysisDslError('NETWORK_BUDGET','Composition exceeds 128 ports or 32 blocks');
   instances.push({id,model:{...model,ports:[...model.ports],analyses:model.analyses?[...model.analyses]:undefined},ports:names});
   return new Map(names.map((name,i)=>[name,start+i]));
  }
  if(d.kind!=='connect')throw new AnalysisDslError('UNSUPPORTED_NETWORK_MODEL','Frequency networks require explicit tabulated resources, not PCB lines');
  if(!Array.isArray(d.input.networks)||!Array.isArray(d.input.connections)||!object(d.input.ports))invalid('Invalid composition');
  const child=new Map<string,Map<string,number>>();
  for(const h of d.input.networks){if(!object(h)||child.has(String(h.id)))invalid('Repeated child handle');child.set(String(h.id),visit(String(h.id),depth+1));}
  const resolve=(port:unknown)=>{if(!object(port))invalid('Invalid connection port');const pd=descriptors.get(String(port.id));if(!pd||!object(pd.input))invalid('Unknown port handle');const index=child.get(String(pd.input.owner))?.get(String(pd.input.name));if(index===undefined)invalid('Port must belong to a listed network');return index;};
  for(const pair of d.input.connections){if(!object(pair))invalid('Invalid pair');connections.push([resolve(pair.from),resolve(pair.to)]);const contacts=pair.contacts??'both';if(!['both','positive','negative'].includes(String(contacts)))invalid('Unknown physical contact selection');connectionContacts.push(contacts as 'both'|'positive'|'negative');explicitContacts ||= pair.contacts!==undefined;}
  return new Map(Object.entries(d.input.ports).map(([name,port])=>[name,resolve(port)]));
 }
 const ports=[...visit(task.config.network!.id)].map(([name,index])=>({name,index}));
 return {instances,connections,ports,...(explicitContacts?{connectionContacts}:{})};
}
export function createFrequencyExecutor(){const execute=createProblemExecutor(true);return async(task:AnalysisTask<'networkParameters'>):Promise<ExecutionResult>=>{
 const c=task.config;if(task.variantChanges.length)throw new AnalysisDslError('UNSUPPORTED_NETWORK_MODEL','Variants require an explicitly rebuilt frequency model');
 if(c.numerics)throw new AnalysisDslError('UNSUPPORTED_NETWORK_MODEL','Frequency blocks use fixed validated F64 residual/conditioning/passivity guards; numerics overrides unsupported');
 const graph=frequencyTopology(task),n=graph.ports.length,points=c.frequencyHz===undefined?c.frequency?.points??0:1;
 if(!Number.isSafeInteger(points)||points<1||n<1||n>16||points>100000||points>131072/n/n||points*graph.instances.reduce((s,i)=>s+i.ports.length,0)**3>1e8)throw new AnalysisDslError('NETWORK_BUDGET','Frequency output/work cap exceeded before axis allocation');
 if(c.representation!=='S'&&c.referenceOhm!==undefined)invalid('Z/Y output cannot specify referenceOhm');
 const references=c.representation==='S'?graph.ports.map(p=>typeof c.referenceOhm==='number'?c.referenceOhm:c.referenceOhm![p.name]):undefined;
 const frequencies=c.frequencyHz!==undefined?[c.frequencyHz]:Array.from({length:points},(_,i)=>{const f=c.frequency!,t=points===1?0:i/(points-1);return f.scale==='log'?f.startHz*(f.stopHz/f.startHz)**t:f.startHz+(f.stopHz-f.startHz)*t;});
 const sourceBands=graph.instances.map(i=>declaredNetworkBand(i.model));
 for(const band of sourceBands)if(band&&frequencies.some(f=>f<band[0]||f>band[1]))throw new AnalysisDslError('NETWORK_BAND','Requested frequency lies outside producer applicability');
 const input=graph.instances[0]?.model.data;
 const exact=object(input)&&input.format==='physical-port-samples'?input as unknown as T.PhysicalPortSampleData:undefined;
 let instances=graph.instances;
 if(exact){
  const instance=instances[0];
  if(instances.length!==1||graph.connections.length||graph.ports.length!==instance.model.ports.length||
     graph.ports.some((port,i)=>port.index!==i))
   throw new AnalysisDslError('NETWORK_PHYSICAL_SAMPLE_TOPOLOGY','Standalone replay requires all physical pairs in producer order');
  if(c.representation==='S'&&references?.some((value,i)=>value!==exact.referenceOhm[i]))
   throw new AnalysisDslError('NETWORK_PHYSICAL_SAMPLE_REFERENCE','Exact physical S samples retain their original mathematical references');
  const allowed=new Set(exact.frequenciesHz);
  if(frequencies.some(f=>!allowed.has(f)))
   throw new AnalysisDslError('NETWORK_EXACT_GRID','Requested frequency is absent from the physical producer grid');
  // P6 checks a single matrix table algebraically. No network joins or
  // interpolation are possible after the guards above.
  const table={format:'network-parameters',representation:exact.representation,referenceTopology:'common',nature:'passive',
   frequenciesHz:exact.frequenciesHz,parameters:exact.parameters,
   ...(exact.representation==='S'&&{referenceOhm:Object.fromEntries(instance.model.ports.map((name,i)=>[name,exact.referenceOhm[i]]))})};
  instances=[{...instance,model:{...instance.model,data:table}}];
 }
 const report=execute({kind:'frequency-network',request:{...graph,instances,frequencies,representation:c.representation!,referenceOhm:references,budget:c.budget}});
 if(report.status!=='complete')throw new AnalysisDslError(report.diagnostics[0]?.code??'NETWORK_REJECTED',report.diagnostics.map(d=>d.message).join('; '),report.diagnostics);
 report.resources={peakRssBytes:process.resourceUsage().maxRSS*1024,metric:'OS process high-water RSS through native call; includes Node/earlier solves'};
 const sourceValidity=graph.instances[0].model.validity;
 const sourceDomain=object(sourceValidity)&&object(sourceValidity.domain)?sourceValidity.domain:undefined;
 const validity:T.Validity=exact?{
  status:'conditional',reason:exact.samplePolicy==='exact'?'Standalone replay of native-checked physical S samples at exact producer points':'Standalone replay of physical S samples; points outside directFrequenciesHz come from a rational model of the producer',
  domain:json({samplePolicy:exact.samplePolicy,evaluatedFrequenciesHz:report.frequenciesHz,
   ...(sourceDomain?.producer?{producer:sourceDomain.producer}:{}),
   sourceValiditySha256:createHash('sha256').update(JSON.stringify(json(compactValidity(sourceValidity)??null))).digest('hex'),
  }),
 }:{status:report.conditional?'conditional':'valid',...(report.conditional?{reason:'Interpolated source coefficients or conditional input validity; see solution provenance/intervals'}:{})};
 const unit=c.representation==='S'?'1':c.representation==='Z'?'Ohm':'S';
 const metrics:Record<string,T.DataDescription>={};for(let i=0;i<n;i++)for(let j=0;j<n;j++)metrics[`parameter:${JSON.stringify({out:graph.ports[i].name,in:graph.ports[j].name})}`]={kind:'series',unit,values:report.parameters!.map(m=>m[i][j]),axis:{name:'frequencyHz',unit:'Hz',values:report.frequenciesHz!},validity,diagnostics:[]};
 const sourceHashes=graph.instances.map(i=>({instanceId:i.id,resourceId:i.model.id,sha256:createHash('sha256').update(JSON.stringify(json(i.model))).digest('hex')}));
 if(exact){
  if(JSON.stringify(report.frequenciesHz)!==JSON.stringify(frequencies)||
     (c.representation==='S'&&JSON.stringify(report.referenceOhm)!==JSON.stringify(exact.referenceOhm))||
     report.ports?.length!==graph.ports.length||
     report.ports.some((port,i)=>port.name!==graph.ports[i].name||port.index!==graph.ports[i].index))
   throw new AnalysisDslError('NETWORK_EXACT_GRID','Native replay changed physical sample coordinates');
  const network:T.NetworkResource={id:'physical-replay-'+randomUUID(),kind:'network',ports:graph.ports.map(p=>p.name),
   source:'Exact native-checked physical-port samples',analyses:['networkParameters'],validity,
   data:{...exact,representation:c.representation!,frequenciesHz:report.frequenciesHz!,parameters:report.parameters!,
    ...(c.representation==='S'?{producerParameters:undefined}:{producerParameters:frequencies.map(f=>(exact.representation==='S'?exact.parameters:exact.producerParameters!)[exact.frequenciesHz.indexOf(f)])})}};
  const {provenance,bandHz,interpolation,topology,validityLimitations,...response}=report as typeof report & {topology?:unknown};
  return {id:task.contextId+':physical-replay-'+randomUUID(),status:'complete',method:'Exact-grid physical sample replay',
   diagnostics:report.diagnostics,assumptions:['Only original physical port pairs, references and sampled frequencies are supported.'],
   metrics,network,solution:json({...response,
    topology:{referenceTopology:'physical-pairs',connections:[]},
    physicalPorts:exact.physicalPorts,samplePolicy:exact.samplePolicy,validity,sourceHashes})};
 }
 const compactProvenance=report.provenance!.map((p:any,i)=>({instanceId:p.instanceId,resourceId:p.resource.id,sha256:sourceHashes[i].sha256,source:p.resource.source,sourcePorts:p.sourcePorts,mapping:p.mapping,sourceRepresentation:p.sourceRepresentation,sourceReferenceOhm:p.sourceReferenceOhm,bandHz:p.bandHz,nature:p.nature,sourceValidity:compactValidity(p.resource.validity)??null,allowedAnalyses:p.resource.analyses??null}));
 const physical=object(report.assembledModel);
 const descriptor=physical||graph.instances.length===1&&object(graph.instances[0].model.data)&&graph.instances[0].model.data.format==='coupled-rlc';
 const analytic=descriptor||graph.instances.length===1&&object(graph.instances[0].model.data)&&graph.instances[0].model.data.format==='delayed-scattering'&&graph.connections.length===0&&graph.ports.length===graph.instances[0].ports.length&&graph.ports.every((p,i)=>p.index===i);
 const originalValidity:Partial<T.Validity>=analytic&&object(graph.instances[0].model.validity)?compactValidity(graph.instances[0].model.validity) as T.Validity:{};
 const originalDomain=object(originalValidity.domain)?originalValidity.domain:{};
 const band=[Math.max(report.bandHz![0],...sourceBands.map(b=>b?.[0]??0),...(analytic?[]:[frequencies[0]])),Math.min(report.bandHz![1],...sourceBands.map(b=>b?.[1]??Number.MAX_VALUE),...(analytic?[]:[frequencies.at(-1)!]))];
 const originalData=graph.instances[0].model.data;
 const analyticData=physical?report.assembledModel:analytic&&object(originalData)&&originalData.format==='delayed-scattering'&&object(originalData.referenceOhm)?{...originalData,referenceOhm:Object.fromEntries(graph.ports.map((p,i)=>[p.name,(originalData.referenceOhm as Record<string,T.Json>)[graph.instances[0].model.ports[i]]]))}:originalData;
 const resource:T.NetworkResource={id:`computed-${randomUUID()}`,kind:'network',ports:graph.ports.map(p=>p.name),source:analytic?'Evaluated supplied analytic model; original terminal/reference/temporal semantics retained':'Computed explicit frequency network; no temporal or PCB model',analyses:physical?(['networkParameters',...(graph.instances.every(i=>!i.model.analyses||i.model.analyses.includes('si'))?['si']:[])] as T.AnalysisKind[]):analytic?(graph.instances[0].model.analyses??['networkParameters','si']) as T.AnalysisKind[]:['networkParameters'],validity:{...originalValidity,...validity,domain:json({...originalDomain,bandHz:band,provenance:compactProvenance,interpolation:report.interpolation,...(physical?{physicalCompositionAssumption:report.validityLimitations}:{}),...(analytic?{sourceValidity:originalValidity}:{})})},data:analytic?analyticData as T.NetworkResource['data']:{format:'network-parameters',representation:c.representation!,referenceTopology:'common',nature:report.nature!,frequenciesHz:report.frequenciesHz!,parameters:report.parameters!,...(c.representation==='S'?{referenceOhm:Object.fromEntries(graph.ports.map((p,i)=>[p.name,references![i]]))}:{})}};
 // Include exact resource hashes independently of report Resource compaction.
 const solution=json({...report,sourceHashes});
 return{id:`${task.contextId}:frequency-${randomUUID()}`,status:'complete',method:descriptor?'Full passive RLC descriptor with physical differential ports; CPU/F64':analytic?'Exact causal real-K delayed scattering; CPU/F64':'Explicit common-return multiport power waves; faer real-block LU CPU/F64',diagnostics:report.diagnostics,assumptions:descriptor?['Supplied full RLC matrices, original reference topology and differential port incidence; no hidden terminal connections or PCB accuracy inferred.','Physical mapped composition preserves conductor charge; absent intermodel mutual C/L is not extracted. Close objects require a jointly extracted model.']:analytic?['Exact supplied passive real-K delay model; original ordered terminals, references and applicability retained.']:['Synthetic/supplied tabulated frequency behavior; no PCB extraction, implicit terminations, temporal response or causal/stability proof.','Complex linear interpolation in each source representation/reference BEFORE conversion and composition.','Passivity checked at evaluated points only; active and nonreciprocal models allowed.'],metrics,network:resource,solution};
 };}
