import {randomUUID} from 'node:crypto';
import type {AnalysisThermalCooling,AnalysisThermalNetwork} from '../../../native/analyze/index.js';
import {createProblemExecutor} from '../execute-problem.ts';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {AnalysisDslError} from './schema.ts';
import {object} from './descriptions.ts';
import {json} from './data.ts';
import {thermalBoard} from './thermal-board.ts';
import type {ThermalSolutionResolver} from './thermal-loss-transfer.ts';
function unsupported(message:string):never{throw new AnalysisDslError('UNSUPPORTED_THERMAL_MODEL',message);}
/** Resolve one requested observation from complete authoritative arrays. Missing
 * or partial fields stay unavailable; no eager descriptor per internal FEM edge. */
export function thermalObservedData(task:AnalysisTask<'thermal'>,name:string,argument:unknown,payload:ExecutionResult):T.DataDescription|undefined{
 if(typeof argument!=='string'||!['temperature','heatFlow'].includes(name)||!object(payload.solution))return;
 const solution=payload.solution,table=solution[name==='temperature'?'temperaturesC':'heatFlowW'];if(!object(table))return;
 const values=table[argument];if(!Array.isArray(values)||!values.length||!values.every(v=>typeof v==='number'&&Number.isFinite(v)))return;
 const template=payload.metrics.energyBalance??payload.metrics.temperatureMap;if(!template)return;
 const unit=name==='temperature'?'degC':'W',validity=template.validity,diagnostics=template.diagnostics;
 if((task.config.domain??'time')==='steady')return values.length===1?{kind:'scalar',value:values[0]as number,unit,validity,diagnostics}:undefined;
 const axis=solution.axis;if(!Array.isArray(axis)||axis.length!==values.length||!axis.every(v=>typeof v==='number'&&Number.isFinite(v)))return;
 return{kind:'series',values:values as number[],axis:{name:'timeS',unit:'s',values:axis as number[]},unit,validity,diagnostics};
}
export function thermalCooling(model:T.Model|undefined,resistance?:number):AnalysisThermalCooling{
 if(resistance!==undefined)return{resistanceKPerW:resistance};
 if(!model||'id'in model||!['thermal-interface','cooling'].includes(model.kind))unsupported('Thermal links require local explicit resistance/interface/cooling model');
 const {kind,...law}=model as Extract<T.LocalModel,{kind:'cooling'|'thermal-interface'}>;
 return json(law) as unknown as AnalysisThermalCooling;
}
/** Existing DSL adapter; all numerical assembly and time integration remain Rust. */
export function createThermalExecutor(resolve?:ThermalSolutionResolver){
 const execute=createProblemExecutor(true);
 return async(task:AnalysisTask<'thermal'>):Promise<ExecutionResult>=>{
  const c=task.config;if(task.variantChanges.length)unsupported('Thermal variants are not implemented');
  const deadline=c.mesh?.refinement?Math.min(task.deadlineAtMs??Infinity,performance.now()+1000*(c.budget?.maxSeconds??120)):Infinity;
  const checkDeadline=()=>{if(performance.now()>deadline)throw new AnalysisDslError('THERMAL_DEADLINE','Total thermal refinement budget exceeded, including adapter/transport/report construction');};
  let network:AnalysisThermalNetwork;let board:ReturnType<typeof thermalBoard>|undefined;
  if(c.network){
   if(c.mesh||c.inputPolicy)unsupported('Board mesh/input policy cannot apply to explicit thermal network');
   const descriptor=task.descriptors.find(d=>object(d)&&d.id===c.network!.id);
   if(!object(descriptor)||!object(descriptor.input))unsupported('Unknown thermal network');
   const input=descriptor.input as unknown as T.ThermalNetworkInput;
   network={nodes:input.nodes.map(n=>({...n})),links:input.links.map(l=>({id:l.id??`${l.from}->${l.to}`,from:l.from,to:l.to,cooling:thermalCooling(l.model,l.resistanceKPerW)})),sources:input.sources.map(s=>({...s}))};
  }else{board=thermalBoard(task,resolve??((contextId,id)=>contextId===task.contextId?task.completedSolutions?.find(s=>object(s)&&s.id===id):undefined));network=board.network;}
  const mode=c.domain??'time';
  checkDeadline();
  const budget=Number.isFinite(deadline)?{...c.budget,maxSeconds:(deadline-performance.now())/1000}:c.budget;
  const report=execute({kind:'thermal',request:{contextId:task.contextId,network,mode,ambientC:c.environment!.ambientC,initialC:c.initialC,initial:c.initial==='steady'?'steady':c.initial?.data,time:c.time,atS:c.atS,numerics:c.numerics,budget,...(board?{board:board.board}:{})}});
  checkDeadline();
  if(report.status!=='complete')throw new AnalysisDslError(report.diagnostics[0]?.code??'THERMAL_REJECTED',report.diagnostics.map(d=>d.message).join('; '),report.diagnostics);
  report.resources={peakRssBytes:process.resourceUsage().maxRSS*1024,metric:'Process OS high-water RSS through native call; includes Node/prior calculations'};
  const id=`thermal-${randomUUID()}`;
  const metrics:Record<string,T.DataDescription>={};
  const unconverged=report.refinement?.converged===false;
  if(unconverged)report.diagnostics.push({code:'THERMAL_MESH_UNCONVERGED',severity:'warning',message:`Sampled mesh convergence not established: ${report.refinement!.reason}`});
  const validity:T.Validity=unconverged||board?.conditional?{status:'conditional',reason:unconverged?`Numerically computed field; sampled mesh convergence not established (${report.refinement!.reason})`:'Explicit supplied CAD snapshot or conditional material; thermal coefficients are authored assumptions',domain:{boardIdentity:task.boardIdentity??null}}:{status:'valid'};
  const datum=(values:readonly number[],unit:string):T.DataDescription=>({kind:mode==='time'?'series':'scalar',unit,validity,diagnostics:[],...(mode==='time'?{values,axis:{name:'timeS',unit:'s',values:report.axis!}}:{value:values[0]})});
  for(const[key,id]of board?.componentNodes??[])if(report.temperaturesC?.[id])metrics[`temperature:${key}`]=datum(report.temperaturesC[id],'degC');
  if(board&&object(report.spatial)&&object(report.spatial.mesh)&&Array.isArray(report.spatial.mesh.fieldNodes)){
   const fields=report.spatial.mesh.fieldNodes as unknown as {id:string;layer:string;pointMm:[number,number]}[];
   metrics.temperatureMap={kind:'map',unit:'degC',validity,diagnostics:[],values:fields.map(f=>report.temperaturesC![f.id].at(-1)!),data:json({solutionId:id,boardIdentity:task.boardIdentity,timeS:report.axis?.at(-1),association:'layered-FEM-node',fieldReference:'spatial.mesh.fieldNodes',coverage:'Every physical board mesh support; no inferred missing values'})};
  }
  if(report.energyBalance)metrics.energyBalance=datum(report.energyBalance,mode==='time'?'J':'W');
  const spatialAssumptions=board&&object(report.spatial)&&Array.isArray(report.spatial.assumptions)?report.spatial.assumptions.filter((v):v is string=>typeof v==='string'):[];
  if(board&&object(report.spatial)&&object(report.spatial.geometry)&&object(report.spatial.mesh))spatialAssumptions.push(`Requested curve deviation ${report.spatial.mesh.geometryDeviationMm} mm; prepared region approximation bound ${report.spatial.geometry.curveApproximationBoundMm} mm; Boolean bound ${report.spatial.geometry.booleanErrorBoundMm} mm; measured constraint displacement ${report.spatial.mesh.constraintDeviationMm} mm. Bounds are not exact measured geometry errors.`);
  checkDeadline();
  const result:ExecutionResult={id,status:'complete',method:report.method!,diagnostics:report.diagnostics,assumptions:['Explicit thermal nodes, capacities and paths supplied by scenario. No package defaults or inferred junction temperatures.','Cooling tables use linear interpolation only within signed declared delta-T range. Cooling scale is dimensionless.','Adaptive backward Euler step doubling; source/ambient/cooling events align with internal steps. Report step specifies observation times.','Energy balance uses the same accepted half-step quadrature as the integrator; sampled extrema may miss intersample peaks.',...spatialAssumptions],metrics,solution:json({...report,id,request:{...c,network},lossTransfer:board?.lossTransfer,boardIdentity:task.boardIdentity??null}),...(report.finalState?{finalState:json(report.finalState)}:{})};
  checkDeadline();return result;
 };
}
