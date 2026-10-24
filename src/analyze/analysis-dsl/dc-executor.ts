import { randomUUID } from 'node:crypto';
import type { AnalysisDcRequest, AnalysisDcReport } from '../../../native/analyze/index.js';
import { createProblemExecutor } from '../execute-problem.ts';
import type { AnalysisTask, ExecutionResult } from './executor.ts';
import type * as T from './spec-doc.js';
import { AnalysisDslError } from './schema.ts';
import { object } from './descriptions.ts';
import { dcMaterial } from './profile-adapter.ts';
import { json } from './data.ts';
import { dcPadId, dcPadPotentials } from './dc-observation.ts';

function unsupported(message: string): never { throw new AnalysisDslError('UNSUPPORTED_DC_MODEL', message); }
function hasConditionalMaterial(task:AnalysisTask<'dc'>):boolean {
  const descriptor=task.config.conductor?task.descriptors.find(d=>object(d)&&d.id===task.config.conductor!.id):undefined;
  const material=object(descriptor)&&object(descriptor.input)&&object(descriptor.input.material)?descriptor.input.material as T.Material:undefined;
  return [...Object.values(task.config.materials??{}),...(material?[material]:[])].some(m=>m.validity?.status==='conditional'||m.temperatureCoefficientPerK!==undefined);
}
/** One closure per scenario, retaining at most one native preparation/matrix. */
export function createDcExecutor() {
  const execute = createProblemExecutor(true);
  let geometryReference: { key:string; solutionId:string; domains:Set<string> } | undefined;
  return async (task: AnalysisTask<'dc'>): Promise<ExecutionResult> => {
    const c = task.config;
    if (task.variantChanges.length) unsupported('DC variant geometry/material rebuilding is not implemented');
    if (c.circuit) unsupported('General circuit DC/MNA is not supported; use board or rectangular conductor');
    if (c.numerics && Object.keys(c.numerics).length) unsupported('Custom DC solver tolerances/iterations are not implemented; fixed residual/KCL/energy guards are reported');
    const description = (id: string) => task.descriptors.find(d => object(d) && d.id === id) as { input?: unknown } | undefined;
    const terminals = new Map<string, AnalysisDcRequest['terminals'][number]>();
    const ports = new Map<string, [string,string]>();
    const pad = (s: T.Selector): string => {
      const padId = dcPadId(task.board, s);
      if (padId === undefined) unsupported('DC contact requires one unambiguous physical pad');
      const id = `pad:${padId}`; terminals.set(id, { id, pad_id: padId }); return id;
    };
    // Only sources/loads/elements activate electrodes. Port descriptions and
    // passive observations must not change the matrix or selected domains.
    const pair = (port: T.Port): [string,string] => {
      const d = description(port.id);
      if (!object(d?.input) || !d.input.positive || !d.input.negative) unsupported('DC port is not a board pad pair');
      if(d.input.layer!==undefined)unsupported('Surface-specific port layer is not supported by this DC contact model');
      const side = (v: unknown) => {
        const list = Array.isArray(v) ? v : [v];
        if (list.length !== 1) unsupported('DC aggregated port electrodes are not supported; use separate sources or explicit per-pin loads');
        return pad(list[0] as T.Selector);
      };
      const p: [string,string] = [side(d.input.positive), side(d.input.negative)];
      ports.set(port.id, p); return p;
    };
    const elements: AnalysisDcRequest['elements'] = [];
    const portElements = new Map<string,string[]>();
    const add = (id: string, kind: 'voltage'|'current'|'resistance', p: string, n: string, v: number, role: 'source'|'load', port?: string) => {
      elements.push({ id,kind,positive:p,negative:n,value:v,role });
      if (port) portElements.set(port,[...(portElements.get(port)??[]),id]);
    };
    let conductor: AnalysisDcRequest['conductor'] = null;
    if (c.conductor) {
      const input = description(c.conductor.id)?.input as { lengthMm:number; widthMm:number; thicknessMm:number; material:T.Material } | undefined;
      if (!input) unsupported('Unknown conductor');
      const m = dcMaterial(input.material,c.temperatureC);
      conductor = { length_mm:input.lengthMm,width_mm:input.widthMm,thickness_mm:input.thicknessMm,...m };
      add('conductor',c.currentA !== undefined ? 'current' : 'voltage','p','n',c.currentA ?? c.voltageV!,'source');
    } else {
      for (const [i,s] of (c.sources??[]).entries()) {
        if (s.model !== 'ideal' || s.waveform || s.voltageSpectrum || s.currentSpectrum) unsupported('DC source requires explicit model:"ideal" and one constant voltageV/currentA; dynamic/resource sources are unsupported');
        const [p,n] = pair(s.port); add(`source-${i+1}`,s.voltageV !== undefined ? 'voltage' : 'current',p,n,s.voltageV ?? s.currentA!,'source',s.port.id);
      }
      if (!elements.length) unsupported('Board DC requires at least one explicit source');
      for (const [i,l] of (Array.isArray(c.loads) ? c.loads : []).entries()) {
        if ('designator' in l) {
          if (l.waveform || Object.values(l.pinCurrentsA??{}).some(v=>typeof v!=='number') || Object.values(l.returnPinCurrentsA??{}).some(v=>typeof v!=='number')) unsupported('DC pin/load waveforms need an explicit atS operating-point mode, which is not implemented');
          continue; // Use the existing prepare resolver below, never redistribute.
        }
        if (l.powerW !== undefined || l.model || l.waveform) unsupported('DC loads support constant currentA/resistanceOhm only; power, resource and waveform models are unsupported');
        const [p,n]=pair(l.port); add(`load-${i+1}`,l.resistanceOhm !== undefined ? 'resistance' : 'current',p,n,l.resistanceOhm ?? -l.currentA!,'load',l.port.id);
      }
      for (const l of task.resolvedLoads??[]) if (object(l) && Array.isArray(l.distributions)) {
        type Pin = { padId:string; currentA:number };
        const groups = l.distributions as unknown as {side:string;unverified?:boolean;pins:Pin[]}[];
        if (groups.some(g=>g.unverified)) unsupported('Dynamic pin distribution has not been verified');
        const supply=groups.find(g=>g.side==='supply')!.pins.map(p=>({...p}));
        const returns=groups.find(g=>g.side==='return')!.pins.map(p=>({...p}));
        // Realize the already assigned boundary currents as balanced pair entries.
        // This does not add an electrical resistor/short between these contacts.
        let j=0,k=0,branch=0;
        while (j<supply.length && k<returns.length) {
          const s=supply[j],n=returns[k];const current=Math.min(s.currentA,n.currentA);
          const p=pad({kind:'padId',id:s.padId}),q=pad({kind:'padId',id:n.padId});
          add(`component:${l.designator}:${l.rail}:${++branch}`,'current',p,q,-current,'load');
          s.currentA-=current;n.currentA-=current;if(s.currentA===0)j++;if(n.currentA===0)k++;
        }
      }
      for (const e of c.components??[]) {
        if (e.model && ('id' in e.model || !['resistor','capacitor','inductor'].includes(e.kind) || e.model.kind!==e.kind)) unsupported(`${e.designator}: component resource/model is not implemented in DC`);
        if (e.kind==='capacitor') continue; // DC open, no implicit short.
        if (!['resistor','inductor'].includes(e.kind)) continue;
        const modelResistance=e.model && 'resistanceOhm' in e.model ? e.model.resistanceOhm : e.model && 'seriesResistanceOhm' in e.model ? e.model.seriesResistanceOhm : undefined;
        if (e.resistanceOhm!==undefined && modelResistance!==undefined && e.resistanceOhm!==modelResistance) unsupported(`${e.designator}: conflicting explicit DC resistance and model`);
        const resistance=e.resistanceOhm ?? modelResistance ?? e.seriesResistanceOhm;
        if (resistance===undefined || e.ideal) unsupported(`${e.designator}: explicit positive DC resistance/DCR is required`);
        if (e.referenceC !== undefined || e.temperatureCoefficientPerK !== undefined || e.model && ('referenceC' in e.model || 'temperatureCoefficientPerK' in e.model)) unsupported('Component resistance temperature laws are not yet implemented');
        const pads=task.board!.pads.filter(p=>p.component===e.designator);
        if (pads.length!==2 || e.pins && Object.keys(e.pins).length) unsupported(`${e.designator}: supported DC resistor requires exactly two physical pads and no custom pin mapping`);
        add(`component:${e.designator}`,'resistance',pad({kind:'padId',id:pads[0].id}),pad({kind:'padId',id:pads[1].id}),resistance,'load');
      }
    }
    const mesh=c.mesh;
    const request: AnalysisDcRequest = {
      context_id:task.contextId,board_json:conductor ? null : JSON.stringify(task.board),conductor,
      conduction:{ ...(task.conductionProfile ? {profile:task.conductionProfile}:{}), mesh:{min_angle_deg:mesh?.minAngleDeg??0,max_area_mm2:mesh?.maxAreaMm2??1,max_edge_mm:Math.sqrt(.25*(mesh?.maxAreaMm2??1)),max_nodes:mesh?.maxNodes??100_000,max_triangles:mesh?.maxTriangles??200_000,max_refinement_steps:mesh?.maxRefinementSteps??32},
        ...(mesh?.geometryDeviationMm ? {geometry:{max_deviation_mm:mesh.geometryDeviationMm}}:{}) },
      snapshot:c.inputPolicy?.cadFill==='snapshot',terminals:[...terminals.values()],elements,
      limits:{max_memory_mib:c.budget?.maxMemoryMiB??1024,max_unknowns:100_000,max_seconds:c.budget?.maxSeconds??120},
    };
    const startRss=process.memoryUsage().rss;
    const report=execute({kind:'dc',request});
    const measuredRss=process.memoryUsage().rss;
    const id=`dc-${randomUUID()}`;
    const geometryKey=JSON.stringify([task.contextId,task.boardIdentity,request.conduction]);
    const participating=new Set(report.domains?.map(d=>d.domain_id)??[]);
    const reuseGeometry=report.reused_preparation&&geometryReference?.key===geometryKey
      && [...participating].every(id=>geometryReference!.domains.has(id));
    const compactMesh=report.mesh?{...report.mesh,domains:report.mesh.domains.filter(d=>participating.has(d.id)),
      omitted_domains:report.mesh.domains.filter(d=>!participating.has(d.id)).map(({nodes,triangles,boundaries,terminals,...d})=>({...d,omitted:true,node_count:nodes.length,triangle_count:triangles.length,boundary_count:boundaries.length,terminal_count:terminals.length})),
      detail_policy:'Node/element arrays retained only for computed domains. Other domains retain readiness/counts; physical copper/bores remain in geometry. Omitted is not an empty physical domain.'}:undefined;
    const solution=json({...report,conditional:report.conditional||hasConditionalMaterial(task),mesh:compactMesh,...(reuseGeometry?{mesh:undefined,geometry:undefined,meshReference:geometryReference!.solutionId}:{}),id,boardIdentity:task.boardIdentity??null,request:{...request,board_json:undefined},assignments:task.resolvedLoads??[],memory:{rss_before_bytes:startRss,rss_after_bytes:measuredRss,metric:'process RSS at native call boundaries; not peak allocation'}});
    if(report.mesh&&!reuseGeometry)geometryReference={key:geometryKey,solutionId:id,domains:participating};
    return result(task,id,report,solution,ports,portElements);
  };
}
function result(task:AnalysisTask<'dc'>,id:string,r:AnalysisDcReport,solution:T.Json,ports:Map<string,[string,string]>,portElements:Map<string,string[]>):ExecutionResult {
  const diagnostics:T.Diagnostic[]=r.diagnostics.map(d=>({...d,severity:'error'}));
  const conditionalMaterials=hasConditionalMaterial(task);
  const validity:T.Validity=r.status==='rejected' ? {status:'unavailable',reason:r.diagnostics.map(d=>d.message).join('; ')} : r.conditional||conditionalMaterials ? {status:'conditional',reason:[...(r.conditional?['CAD fill freshness unknown: explicitly accepted supplied snapshot']:[]),...(conditionalMaterials?['Supplied conditional material / linear resistivity law with no inferred empirical validity range']:[])].join('; '),domain:{boardIdentity:task.boardIdentity??null,cadFill:task.config.inputPolicy?.cadFill??'strict',originalFreshness:r.mesh?.fills.map(f=>({path:f.source.path,layer:f.layer,freshness:f.freshness}))??[]}} : {status:'valid'};
  const metrics:Record<string,T.DataDescription>={};
  const scalar=(unit:string,v:number|undefined):T.DataDescription=>({kind:'scalar',unit,...(v!==undefined?{value:v}:{}),validity:v!==undefined?validity:{status:'unavailable',reason:'DC quantity has no unambiguous measurement'},diagnostics});
  if(r.standalone){
    metrics.resistance=scalar('Ohm',r.standalone.resistance_ohm);metrics.losses=scalar('W',r.standalone.loss_w);
    for(const [key,unit,v] of [['voltageMap','V',r.standalone.voltage_v],['currentDensityMap','A/mm2',Math.abs(r.standalone.current_density_a_mm2)],['lossMap','W',r.standalone.loss_w]] as const)metrics[key]={...scalar(unit,v),kind:'map',values:[v],data:{solutionId:id,field:key,geometry:'uniform rectangular conductor'}};
  } else {
    metrics.losses=scalar('W',r.copper_loss_w!==undefined?r.copper_loss_w+(r.barrel_loss_w??0):undefined);
    metrics.resistance=scalar('Ohm',undefined);
    if(task.board)Object.assign(metrics,dcPadPotentials(task.board,r,validity,diagnostics));
    for(const [key,unit,field] of [['voltageMap','V','voltage_v'],['currentDensityMap','A/mm2','current_density_a_mm2'],['lossMap','W','loss_w']] as const){
      const values=r.domains?.flatMap(d=>d[field]);
      let offset=0;
      const slices=(r.domains??[]).map(d=>{const start=offset;offset+=d[field].length;return {domainId:d.domain_id,start,count:d[field].length};});
      const mapValidity:T.Validity=r.excluded_domains?.length&&validity.status!=='unavailable'?{status:'conditional',reason:[validity.reason,'Map covers participating domains only; excluded regions have no computed value'].filter(Boolean).join('; '),domain:{...(object(validity.domain)?validity.domain:{}),solutionId:id,excludedDomains:r.excluded_domains}}:validity;
      metrics[key]={kind:'map',unit,validity:values?mapValidity:{status:'unavailable',reason:validity.reason??'DC field unavailable'},diagnostics,...(values?{values}:{}),data:{solutionId:id,field,association:key==='voltageMap'?'mesh-node':'triangle',slices,coverage:'participating physical domains only; excluded/unbuilt regions unknown',domains:r.included_domains??[],excludedDomains:r.excluded_domains??[],reference:'separate mathematical gauge per connected system component',...(key==='voltageMap'?{independentReferences:[...new Set(r.domains?.map(d=>d.reference_node)??[])]}:{}),...(key==='currentDensityMap'?{peak:'mesh-dependent at sharp corners'}:{})}};
    }
    for(const [port,[p,n]]of ports){
      const ids=portElements.get(port)??[],e=ids.length===1?r.elements?.find(e=>e.id===ids[0]):undefined;
      metrics[`current:${port}`]=scalar('A',e?(e.role==='load'?-e.delivered_current_a:e.delivered_current_a):undefined);
    }
    for(const e of r.elements??[])metrics[`current:${JSON.stringify(e.id)}`]=scalar('A',e.role==='load'?-e.delivered_current_a:e.delivered_current_a);
    // Only explicitly assigned external pad currents; arbitrary net current has no cut.
    for(const p of task.board?.pads??[]){
      const terminal=`pad:${p.id}`;const entries=r.elements?.filter(e=>e.positive===terminal||e.negative===terminal);
      if(!entries?.length)continue;
      const current=entries.reduce((sum,e)=>sum+(e.positive===terminal?1:-1)*e.delivered_current_a,0);
      for(const selector of [{kind:'padId',id:p.id},...(p.component?[{kind:'pad',designator:p.component,number:p.number}]:[])])metrics[`current:${JSON.stringify(selector)}`]=scalar('A',current);
    }
  }
  return {id,status:r.status==='complete'?'complete':'partial',method:r.method,diagnostics,assumptions:['Only active source/load/element pads form finite-area equipotential electrodes; passive pads read area-weighted existing first-surface potential without adding conductivity','Barrel ring electrodes joined only through finite segments','No ideal net/GND, material defaults, hidden gmin, refill or DC capacitor short','Maps cover participating domains only; excluded regions remain unknown',...(r.conditional?['Conditional supplied CAD snapshot; pass does not confirm editor freshness']:[])],metrics,solution};
}
