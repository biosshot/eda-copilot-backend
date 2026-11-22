import {randomUUID} from 'node:crypto';
import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {object} from './descriptions.ts';
import {json} from './data.ts';
import {AnalysisDslError} from './schema.ts';
import {createElectricalExecutor} from './electrical-executor.ts';
import {createThermalExecutor} from './thermal-executor.ts';

function refuse(code:string,message:string):never{throw new AnalysisDslError(code,message);}

interface CoupledResistor{id:string;node:string;r0:number;refC:number;alpha:number}
interface ElementRow{node:string;resistanceOhm:number;currentA:number;powerW:number}
interface Pass{electrical:ExecutionResult;thermal:ExecutionResult;Tout:Record<string,number>;lossW:number;coupledW:number;rows:Record<string,ElementRow>}
type Status='converged'|'runaway'|'not-converged';

/** Runaway: for three consecutive passes the temperature residual has the same sign,
 * grows in magnitude and the electrical loss grows too. Other failures are numerical. */
function isRunaway(trace:readonly{signed:number;abs:number;loss:number}[],tol:number){
 if(trace.length<3)return false;
 const[a,b,c]=trace.slice(-3);
 const sameSign=Math.sign(a.signed)===Math.sign(b.signed)&&Math.sign(b.signed)===Math.sign(c.signed)&&a.signed!==0;
 return sameSign&&c.abs>=tol&&c.abs>b.abs&&b.abs>a.abs&&c.loss>b.loss&&b.loss>a.loss;
}

/** v1 electrothermal coordinator: standalone explicit circuit + thermal network, steady only.
 * Picard fixed point over the existing DC electrical executor and steady thermal executor.
 * No third solver: each pass delegates to those executors through a rewritten task. */
export function createElectrothermalExecutor(electrical=createElectricalExecutor(),thermal=createThermalExecutor()){
 return async(task:AnalysisTask<'electrothermal'>):Promise<ExecutionResult>=>{
  const c=task.config,e=c.electrical,t=c.thermal;
  if(task.variantChanges.length)refuse('UNSUPPORTED_ELECTROTHERMAL_MODEL','Electrothermal variants are not implemented');
  if(c.time!==undefined||t?.domain!=='steady')refuse('ELECTROTHERMAL_TRANSIENT_NOT_IMPLEMENTED','Transient electrothermal coupling is not implemented in v1; use thermal domain steady without time');
  if(!e?.circuit||!t?.network||task.board)refuse('ELECTROTHERMAL_BOARD_NOT_IMPLEMENTED','Board electrothermal coupling is not implemented in v1; use electrical.circuit with thermal.network');
  if(e.operating?.length)refuse('ELECTROTHERMAL_NOT_IMPLEMENTED','Gate operating points belong to board devices and are not supported in v1');
  if(e.temperatureC!==undefined)refuse('UNSUPPORTED_ELECTROTHERMAL_MODEL','Global electrical temperature is not used; temperatures come from thermal couplings');
  const circuitId=(e.circuit as {id:string}).id,networkId=(t.network as {id:string}).id;
  const circuitDescriptor=task.descriptors.find(d=>object(d)&&d.id===circuitId),networkDescriptor=task.descriptors.find(d=>object(d)&&d.id===networkId);
  if(!object(circuitDescriptor)||!object(circuitDescriptor.input)||!object(networkDescriptor)||!object(networkDescriptor.input))refuse('UNSUPPORTED_ELECTROTHERMAL_MODEL','Unknown electrical circuit or thermal network');
  const circuit=circuitDescriptor.input as unknown as T.CircuitInput,network=networkDescriptor.input as unknown as T.ThermalNetworkInput;

  const coupled=new Map<string,CoupledResistor>();
  for(const coupling of c.couplings??[]){
   if(!('element' in coupling))refuse('ELECTROTHERMAL_UNSUPPORTED_COUPLING','Board targets cannot be coupled in standalone electrothermal v1');
   const el=circuit.elements.find(x=>x.id===coupling.element);
   if(!el||el.kind!=='resistor')refuse('ELECTROTHERMAL_UNSUPPORTED_COUPLING',`Coupled element ${coupling.element} must be an explicit resistor`);
   if(coupled.has(el.id))refuse('ELECTROTHERMAL_UNSUPPORTED_COUPLING',`Element ${el.id} is coupled more than once`);
   if((el.referenceC===undefined)!==(el.temperatureCoefficientPerK===undefined))refuse('ELECTROTHERMAL_MODEL',`${el.id}: temperature law needs both referenceC and temperatureCoefficientPerK`);
   coupled.set(el.id,{id:el.id,node:coupling.thermalNode,r0:el.resistanceOhm,refC:el.referenceC??0,alpha:el.temperatureCoefficientPerK??0});
  }
  if(!coupled.size)refuse('ELECTROTHERMAL_UNSUPPORTED_COUPLING','At least one resistor coupling is required');
  const links=[...coupled.values()];
  const nodes=[...new Set(links.map(x=>x.node))];
  const hasLaw=links.some(x=>x.alpha!==0);
  const t0=typeof t.initialC==='number'?t.initialC:t.environment?.ambientC;
  if(typeof t0!=='number')refuse('ELECTROTHERMAL_INITIAL_UNDEFINED','Initial coupled temperature needs numeric initialC or environment ambientC');
  const options=c.coupling??{};
  const tol=options.toleranceK??0.01,relTol=options.relativeLossTolerance??1e-4,maxIterations=options.maxIterations??30,omega=options.relaxation??1;

  const resistance=(x:CoupledResistor,T:Record<string,number>)=>{
   const factor=1+x.alpha*(T[x.node]-x.refC);
   if(!(factor>0)||!Number.isFinite(factor))refuse('ELECTROTHERMAL_NONPHYSICAL_RESISTANCE',`${x.id}: 1+alpha(T-Tref) = ${factor} at ${T[x.node]} degC`);
   return x.r0*factor;
  };
  const pass=async(Tin:Record<string,number>):Promise<Pass>=>{
   const resistances=new Map<string,number>();
   const elements=circuit.elements.map(el=>{
    const x=coupled.get(el.id);if(!x)return el;
    const R=resistance(x,Tin);resistances.set(x.id,R);
    const copy={...el} as Record<string,unknown>;delete copy.referenceC;delete copy.temperatureCoefficientPerK;copy.resistanceOhm=R;
    return copy as unknown as typeof el;
   });
   const er=await electrical({...task,kind:'dc',config:{circuit:e.circuit,budget:c.budget},variantChanges:[],
    descriptors:task.descriptors.map(d=>object(d)&&d.id===circuitId?{...d,input:{...circuit,elements}}:d)} as unknown as AnalysisTask<'dc'>);
   const solution=er.solution as unknown as {losses?:number;currents?:Record<string,number>}|undefined;
   if(typeof solution?.losses!=='number'||!solution.currents)refuse('ELECTROTHERMAL_MODEL','Electrical solution has no DC losses or element currents');
   const rows:Record<string,ElementRow>={},nodeW:Record<string,number>={};let coupledW=0;
   for(const x of links){
    const I=solution.currents[x.id];if(typeof I!=='number'||!Number.isFinite(I))refuse('ELECTROTHERMAL_MODEL',`No DC current for coupled element ${x.id}`);
    const R=resistances.get(x.id)!,P=I*I*R;
    rows[x.id]={node:x.node,resistanceOhm:R,currentA:I,powerW:P};nodeW[x.node]=(nodeW[x.node]??0)+P;coupledW+=P;
   }
   const lossW=solution.losses;
   if(Math.abs(lossW-coupledW)>1e-9*Math.max(Math.abs(lossW),Math.abs(coupledW)))
    refuse('ELECTROTHERMAL_POWER_MISMATCH',`Electrical loss ${lossW} W differs from coupled thermal source ${coupledW} W; all dissipation must be coupled`);
   const sources=[...network.sources.map(s=>({...s})),...nodes.map(n=>({node:n,powerW:nodeW[n]??0}))];
   const tr=await thermal({...task,kind:'thermal',config:{network:t.network,environment:t.environment,domain:'steady',budget:c.budget},variantChanges:[],
    descriptors:task.descriptors.map(d=>object(d)&&d.id===networkId?{...d,input:{...network,sources}}:d)} as unknown as AnalysisTask<'thermal'>);
   const temps=(tr.solution as unknown as {temperaturesC?:Record<string,number[]>}|undefined)?.temperaturesC;
   const Tout:Record<string,number>={};
   for(const n of nodes){const v=temps?.[n];if(!Array.isArray(v)||v.length!==1||typeof v[0]!=='number'||!Number.isFinite(v[0]))refuse('ELECTROTHERMAL_MODEL',`Steady thermal temperature missing for node ${n}`);Tout[n]=v[0];}
   return{electrical:er,thermal:tr,Tout,lossW,coupledW,rows};
  };

  let Tin:Record<string,number>=Object.fromEntries(nodes.map(n=>[n,t0]));
  const history:Record<string,unknown>[]=[],trace:{signed:number;abs:number;loss:number}[]=[];
  let last:Pass|undefined,prevLoss:number|undefined,status:Status='not-converged',iterations=0;
  for(let k=1;k<=maxIterations;k++){
   const p=await pass(Tin);last=p;iterations=k;
   const deltas=nodes.map(n=>p.Tout[n]-Tin[n]);
   let idx=0;for(let i=1;i<deltas.length;i++)if(Math.abs(deltas[i])>Math.abs(deltas[idx]))idx=i;
   const signed=deltas[idx],abs=Math.abs(signed);
   const lossChange=prevLoss===undefined?undefined:Math.abs(p.lossW-prevLoss)/Math.max(Math.abs(p.lossW),Math.abs(prevLoss),Number.MIN_VALUE);
   history.push({iteration:k,inputTemperaturesC:{...Tin},outputTemperaturesC:{...p.Tout},electricalLossW:p.lossW,maxResidualK:abs,relativeLossChange:lossChange??null});
   if(!hasLaw){status='converged';break;} // constant resistances: the first pass is exact
   if(lossChange!==undefined&&abs<tol&&lossChange<relTol){status='converged';break;}
   trace.push({signed,abs,loss:p.lossW});
   if(isRunaway(trace,tol)){status='runaway';break;}
   prevLoss=p.lossW;
   Tin=Object.fromEntries(nodes.map(n=>[n,Tin[n]+omega*(p.Tout[n]-Tin[n])]));
  }
  const final=last!;
  const converged=status==='converged';
  const code=status==='runaway'?'ELECTROTHERMAL_RUNAWAY':'ELECTROTHERMAL_NOT_CONVERGED';
  const message=status==='runaway'
   ?`Thermal runaway: residual and electrical loss increased for three passes (last residual ${trace.at(-1)?.abs.toFixed(3)} K, loss ${trace.at(-1)?.loss.toPrecision(6)} W)`
   :`Electrothermal coupling did not converge in ${maxIterations} passes (tolerance ${tol} K, relative loss ${relTol})`;
  const id=`electrothermal-${randomUUID()}`;
  const scalar=(value:number,unit:string):T.DataDescription=>({kind:'scalar',value,unit,validity:{status:'valid'},diagnostics:[]});
  const metrics:Record<string,T.DataDescription>={};
  const thermalMetrics:Record<string,T.DataDescription>={...final.thermal.metrics};
  if(converged){
   metrics.losses=scalar(final.lossW,'W');metrics.convergence=scalar(iterations,'1');
   for(const n of nodes)thermalMetrics[`temperature:${JSON.stringify(n)}`]=scalar(final.Tout[n],'degC');
  }
  const diagnostics=[...(converged?[]:[{code,severity:'error' as const,message}]),...final.electrical.diagnostics,...final.thermal.diagnostics];
  return{
   id,status:converged?'complete':'failed',
   method:'Picard electrothermal fixed point: existing DC electrical executor x existing steady thermal executor (standalone lumped)',
   diagnostics,
   assumptions:[
    'Standalone lumped v1: explicit resistors coupled to explicit thermal nodes. No board mesh; thermal-loss-transfer.ts (DC triangle maps) is not used here.',
    'Coupled resistor law R(T)=R_ref(1+alpha(T-T_ref)), linear resistivity as in the board DC profile; alpha 0 or absent means constant R.',
    'Each pass solves the DC circuit at the input temperatures, then the steady thermal network with the coupled I^2R powers. Stop when max|T_out-T_in| < toleranceK and relative electrical loss change < relativeLossTolerance; with constant R the first pass is exact and counts as one iteration.',
    'All dissipation in the circuit must be coupled: native DC loss equals the coupled thermal source to 1e-9 relative, otherwise ELECTROTHERMAL_POWER_MISMATCH.',
    'Runaway = residual and loss both grow for three consecutive passes; other failures are reported as ELECTROTHERMAL_NOT_CONVERGED with the iteration history. Transient coupling is not implemented.',
  ],
  metrics,
  solution:json({
   status:status==='converged'?'converged':status,iterations,maxIterations,history,
   couplingOptions:{toleranceK:tol,relativeLossTolerance:relTol,maxIterations,relaxation:omega},
   hasTemperatureDependence:hasLaw,
   conservation:{electricalLossW:final.lossW,coupledThermalSourceW:final.coupledW,relativeError:Math.abs(final.lossW-final.coupledW)/Math.max(Math.abs(final.lossW),Number.MIN_VALUE)},
   nodeTemperaturesC:final.Tout,elements:final.rows,electricalId:final.electrical.id,thermalId:final.thermal.id,
  }),
  electrical:final.electrical,
  thermal:{...final.thermal,metrics:thermalMetrics},
  };
 };
}
