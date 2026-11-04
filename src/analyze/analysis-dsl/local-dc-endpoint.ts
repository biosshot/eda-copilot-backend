import { randomUUID } from 'node:crypto';
import type { AnalysisLocalDcReport } from '../../../native/analyze/index.js';
import { componentReachableIslands, MAX_CHANNEL_COMPONENTS } from '../channel-components.ts';
import type { PhysicalDcComponentRequest } from './channel-component-request.ts';
import { capturedBoardGeometry } from '../geometry-snapshot.ts';
import type { LocalDcExtractionProblem } from '../types.ts';
import { json } from './data.ts';
import type { AnalysisTask, ExecutionResult } from './executor.ts';
import type { localRequest } from './local-request.ts';
import { AnalysisDslError } from './schema.ts';
import type { Diagnostic, Validity } from './spec-doc.js';

/** Exact DC of a bounded copper mesh. The selected drive is a probe, not a
 * schematic operating point or an independent excitation of floating ports. */
export function localDcEndpoint(
 task: AnalysisTask<'impedance'>, geometry: ReturnType<typeof localRequest>,
 diagnostics: Diagnostic[], remainingSeconds: number,
 solve: (problem: LocalDcExtractionProblem) => AnalysisLocalDcReport,
 components: readonly PhysicalDcComponentRequest[] = [],
): ExecutionResult {
 if(!Number.isFinite(remainingSeconds)||remainingSeconds<=0)
  throw new AnalysisDslError('LOCAL_DC_DEADLINE','Shared DC planning deadline exhausted',diagnostics);
 const start=performance.now();
 const boardJson=JSON.stringify(task.board), captured=capturedBoardGeometry(boardJson);
 const islands=new Map(captured.islands.flatMap(island=>island.region_ids.map(id=>[id,island.id] as const)));
 const electrodes=[...geometry.electrodes];
 const nativePorts=geometry.ports.map(([a,b]):[string,string]=>[a,b]);
 const nativeLoads:(number|null)[]=nativePorts.map(()=>null);
 const sourceKey=(pin:{sourcePath:string;layer:string})=>JSON.stringify([pin.sourcePath,pin.layer]);
 const existing=new Map(electrodes.map(electrode=>[sourceKey(electrode),electrode.name as string] as const));
 const copperLayers=geometry.stack.filter(layer=>layer.copper).map(layer=>layer.name);
 if(components.length>MAX_CHANNEL_COMPONENTS)
  throw new AnalysisDslError('LOCAL_DC_PORTS','Component bridge count exceeds '+MAX_CHANNEL_COMPONENTS,diagnostics);
 const packages=components.map(component=>{
  const pair=component.pins.map(pin=>{
   const key=sourceKey(pin), known=existing.get(key);
   if(known!==undefined)return known;
   if(!copperLayers.includes(pin.layer))
    throw new AnalysisDslError('LOCAL_DC_PORTS','Component contact layer absent in stack: '+pin.id,diagnostics);
   electrodes.push({name:pin.id,sourcePath:pin.sourcePath,layer:pin.layer,upper:pin.layer===copperLayers.at(-1)});
   existing.set(key,pin.id);
   return pin.id;
  }) as [string,string];
  if(component.resistanceOhm===null)return {...component,electrodes:pair,resistanceOhm:null};
  const privatePortIndex=nativePorts.length;
  nativePorts.push(pair);
  nativeLoads.push(component.resistanceOhm);
  return {...component,electrodes:pair,resistanceOhm:component.resistanceOhm,privatePortIndex};
 });
 if(nativePorts.length>16||electrodes.length>32)
  throw new AnalysisDslError('LOCAL_DC_PORTS','Physical DC endpoint exceeds 16 ports or 32 electrodes',diagnostics);
 const electrodeIsland=new Map(electrodes.map(({name,sourcePath,layer})=>{
  const matches=captured.regions.filter(region=>region.source.path===sourcePath&&region.layer===layer);
  return [name as string,matches.length===1?islands.get(matches[0].id):undefined] as const;
 }));
 const conductiveEdges=packages.flatMap(component=>{
  if(component.resistanceOhm===null)return [];
  const [a,b]=component.electrodes.map(name=>electrodeIsland.get(name));
  if(a===undefined||b===undefined)
   throw new AnalysisDslError('LOCAL_DC_PORTS','Package pins lack confirmed copper islands: '+component.designator,diagnostics);
  return [[a,b] as const];
 });
 const labels=task.config.target?.kind==='differential'?
  ['positive-input','positive-output','negative-input','negative-output']:['input','output'];
 const ports=geometry.ports.map(([positive,negative],i)=>({name:labels[i],positive,negative}));
 const sameLoadedComponent=(a:string,b:string)=>{
  const source=electrodeIsland.get(a),target=electrodeIsland.get(b);
  return source!==undefined&&target!==undefined&&componentReachableIslands([source],conductiveEdges).has(target);
 };
 const relativeSigns=ports.map((from,i)=>{
  const to=ports[i+1];
  if(i%2!==0||!to)return 0;
  if(sameLoadedComponent(from.positive,to.positive)&&sameLoadedComponent(from.negative,to.negative))return -1;
  return sameLoadedComponent(from.positive,to.negative)&&sameLoadedComponent(from.negative,to.positive)?1:0;
 });
 const loop=relativeSigns.findIndex(sign=>sign!==0);
 const drive=loop<0?undefined:nativePorts.map((_,i)=>i===loop?1:i===loop+1?relativeSigns[loop]:0);
 let report: AnalysisLocalDcReport;
 try{
  const left=remainingSeconds-(performance.now()-start)/1000;
  if(left<=0)throw new Error('LOCAL_DC_DEADLINE: shared DC planning deadline exhausted');
  report=solve({kind:'local-dc-extraction',boardJson,request:{
   geometry:{...geometry,electrodes,ports:nativePorts,allowInsulatedCopperCuts:false},observablesOnly:true,externalPortCount:ports.length,
   ...(drive&&{drive}),...(nativePorts.length>ports.length&&{loadsOhm:nativeLoads}),
  },remainingSeconds:left});
 }catch(error){
  const message=error instanceof Error?error.message:String(error);
  throw new AnalysisDslError(message.match(/^([A-Z][A-Z0-9_]+):/)?.[1]??'LOCAL_DC_NATIVE_REJECTED',message,diagnostics);
 }
 if(report.status!=='complete'||report.frequencyHz!==0||report.allowInsulatedCopperCuts!==false||JSON.stringify(report.ports)!==JSON.stringify(nativePorts)||
    JSON.stringify(report.driveA)!==JSON.stringify(drive??nativePorts.map(()=>0))||
    JSON.stringify(report.loadsOhm)!==JSON.stringify(nativeLoads)||
    !Number.isFinite(report.drivePowerW)||report.drivePowerW<0)
  throw new AnalysisDslError('LOCAL_DC_EVIDENCE','Native DC result does not match the ordered physical probe and explicit package loads',diagnostics);

 const externalPortCount=ports.length;
 const nativePortMapping={externalPortCount,ports:report.ports,driveA:report.driveA,loadsOhm:report.loadsOhm,
  portRealV:report.portRealV,floatingPortIndices:report.floatingPortIndices};
 const externalResult={ports:report.ports.slice(0,externalPortCount),driveA:report.driveA.slice(0,externalPortCount),
  loadsOhm:report.loadsOhm.slice(0,externalPortCount),portRealV:report.portRealV.slice(0,externalPortCount),
  floatingPortIndices:report.floatingPortIndices.filter(index=>index<externalPortCount)};
 const componentDc=packages.map(component=>{
  if(component.resistanceOhm===null)return {...component,dc:'open',currentA:0,lossW:0};
  const voltageV=report.portRealV[component.privatePortIndex];
  if(typeof voltageV!=='number'||!Number.isFinite(voltageV))
   throw new AnalysisDslError('LOCAL_DC_EVIDENCE','Missing finite voltage across package '+component.designator,diagnostics);
  const currentA=voltageV/component.resistanceOhm,lossW=(voltageV/Math.sqrt(component.resistanceOhm))**2;
  if(!Number.isFinite(currentA)||!Number.isFinite(lossW))
   throw new AnalysisDslError('LOCAL_DC_EVIDENCE','Nonfinite current or loss for package '+component.designator,diagnostics);
  return {...component,dc:'resistive',voltageV,currentA,lossW};
 });
 const packageLossW=componentDc.reduce((sum,component)=>sum+component.lossW,0);
 if(!Number.isFinite(report.loadLossW)||report.loadLossW<0||
    Math.abs(report.loadLossW-packageLossW)>1e-12*Math.max(report.loadLossW,packageLossW,Number.MIN_VALUE))
  throw new AnalysisDslError('LOCAL_DC_EVIDENCE','Per-package DC losses do not match the native load total',diagnostics);
 const loopProbe=drive?{from:ports[loop].name,to:ports[loop+1].name,currentA:1,driveA:externalResult.driveA,resistanceOhm:report.drivePowerW}:undefined;
 const packageEvidence=components.length?{componentDc,nativePortMapping,copperLossW:report.totalLossW,packageLossW,
  dcConductanceScope:'Copper electrode Schur matrix before package load stamps'}:{};
 const validity:Validity={status:'conditional',reason:report.validity.reason+' Independent mesh/domain accuracy and AC continuity remain unverified.',
  domain:{frequencyHz:0,continuousFrequencyCoverage:false}};
 return {
  id:task.contextId+':local-dc-'+randomUUID(),status:'complete',method:'Bounded physical volume conduction, exact DC endpoint',
  diagnostics,assumptions:[
   'Original source contacts and copper galvanic islands are preserved. Floating port voltages are unavailable.',
   ...(components.length?['Resistive package stamps contribute separately to loaded connectivity and dissipation. Capacitors are open at DC.']:[]),
   ...(loopProbe?['A galvanically balanced 1 A Norton probe gives the loop resistance from drive power, including any supplied resistive package loss.']:['No primary channel-loop probe was selected. The primary electrode response uses zero drive. The separate modal response reports any admissible balanced probes.']),
  ],
  metrics:{segments:{kind:'table',unit:'1',data:json({frequencyHz:0,ports,loopProbe,
   electrodeNames:report.electrodeNames,electrodeGalvanicComponents:report.electrodeGalvanicComponents,
   balancedPortResponse:report.balancedPortResponse,
   dcConductanceS:report.dcConductanceS,portRealV:externalResult.portRealV,floatingPortIndices:externalResult.floatingPortIndices,...packageEvidence}),validity,diagnostics:[]}},
  solution:json({...report,...externalResult,physicalPorts:ports,loopProbe,validity,...packageEvidence}),
 };
}
