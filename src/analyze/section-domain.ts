import {canonical} from './local-model-cache.ts';
import {createProblemExecutor} from './execute-problem.ts';
import type {AnalysisProblem} from './types.ts';
export interface SectionDomainStep {
 axis:'baseline'|'physical-width'|'air-boundary'|'mesh';
 /** Full original copper/materials, physical ports and boundary policy, never net labels. */
 physicalIdentity:unknown;
 conductorIdentity:unknown;
 request:Extract<AnalysisProblem,{kind:'cross-section'}>['request'];
}
const fail=(s:string):never=>{throw Error('SECTION_DOMAIN: '+s);};
/** A six-step comparison stencil uses the same final request for all three
 * holdouts. Baselines are not certificates. Identical requests are reuse only. */
export function validateSectionDomainPlan(steps:SectionDomainStep[]){
 const key=(v:unknown)=>canonical(v);
 const rect=(c:any):number[]=>{if(!c||c.holesM?.length||c.outerM?.length!==4)fail('rectangular refinement contour required');const ps=c.outerM, b=[Math.min(...ps.map((p:any)=>p[0])),Math.min(...ps.map((p:any)=>p[1])),Math.max(...ps.map((p:any)=>p[0])),Math.max(...ps.map((p:any)=>p[1]))];if(ps.some((p:any)=>!(p[0]===b[0]||p[0]===b[2])||!(p[1]===b[1]||p[1]===b[3])))fail('nonrectangular refinement');return b;};
 const requestKey=(step:SectionDomainStep)=>{const r=structuredClone(step.request) as any;delete r.budget;delete r.line.geometry.maxSeconds;delete r.line.geometry.maxMemoryMib;return key(r);};
 const proofs:(string|undefined)[]=[];
 for(let i=0;i<steps.length;i++){
  const b=steps[i],a=steps[i-1];if(b.axis==='baseline'){proofs.push(undefined);continue;}if(!a)fail('first step must be baseline');
  if(requestKey(a)===requestKey(b))fail('identical request is cache reuse, not refinement');
  const x=structuredClone(a.request) as any,y=structuredClone(b.request) as any,X=x.line?.geometry,Y=y.line?.geometry;
  if(!X||!Y)fail('missing polygon geometry');
  if(b.axis==='mesh'){
   if(!(Y.maxTriangleAreaM2>0&&Y.maxTriangleAreaM2<X.maxTriangleAreaM2))fail('mesh must refine');
   X.maxTriangleAreaM2=Y.maxTriangleAreaM2;
  }else if(b.axis==='physical-width'){
   if(X.conductors.length!==Y.conductors.length)fail('conductor topology changed');let changed=false;
   X.conductors.forEach((c:any,j:number)=>{const d=Y.conductors[j];if(key(c)===key(d))return;if(c.id!==d.id||c.role!==d.role||c.role!=='reference')fail('only physical reference width may expand');const u=rect(c.contour),v=rect(d.contour);if(u[1]!==v[1]||u[3]!==v[3]||v[0]>u[0]||v[2]<u[2]||v[0]===u[0]&&v[2]===u[2])fail('reference must expand without moving vertically');c.contour=d.contour;changed=true;});if(!changed)fail('no physical copper expansion');
  }else if(b.axis==='air-boundary'){
   const u=rect(X.domain),v=rect(Y.domain);if(v[0]>u[0]||v[1]>u[1]||v[2]<u[2]||v[3]<u[3]||key(u)===key(v))fail('field boundary must expand');
   if(X.dielectrics.length!==Y.dielectrics.length)fail('material topology changed');
   X.dielectrics.forEach((m:any,j:number)=>{const n=Y.dielectrics[j];if(m.id!==n.id||m.relativePermittivity!==n.relativePermittivity)fail('materials changed');if(key(m.contour)===key(n.contour))return;const q=rect(m.contour),r=rect(n.contour);for(let k=0;k<4;k++)if(r[k]!==q[k]&&!(q[k]===u[k]&&r[k]===v[k]))fail('dielectric interior boundary changed');m.contour=n.contour;});X.domain=Y.domain;
  }
  for(const z of [X,Y]){delete z.maxSeconds;delete z.maxMemoryMib;}delete x.budget;delete y.budget;
  if(key(x)!==key(y))fail('simultaneous independent geometry/mesh changes');proofs.push(requestKey(b));
 }
 return proofs;
}
/** Bounded, sequential coefficients-only admission. Cache and cancellation remain
 * owned by the existing executor/disposable caller, with no second cache. */
export function selectSectionDomain(steps:SectionDomainStep[],budget={maxSeconds:120,maxMemoryMiB:1024},execute=createProblemExecutor(true),cancelled=()=>false){
 if(steps.length<4||steps.length>10||!Number.isFinite(budget.maxSeconds)||budget.maxSeconds<=0||budget.maxSeconds>300||!Number.isFinite(budget.maxMemoryMiB)||budget.maxMemoryMiB<=0||budget.maxMemoryMiB>2048)fail('bounded refinement plan required');
 const proofs=validateSectionDomainPlan(steps);
 const physical=canonical(steps[0].physicalIdentity),conductors=canonical(steps[0].conductorIdentity);
 const start=performance.now();let previous:any;let invariant:string|undefined;const audit:any[]=[];const passed=new Map<string,Set<string>>();
 const finite=(x:any):number=>typeof x==='number'&&Number.isFinite(x)?x:fail('nonfinite or missing observable');
 const flatten=(x:any):number[]=>Array.isArray(x)?x.flatMap(flatten):x&&typeof x==='object'&&'real'in x?[finite(x.real),finite(x.imaginary)]:[finite(x)];
 for(const [index,step]of steps.entries()){
  if(cancelled())fail('cancelled');
  if(canonical(step.physicalIdentity)!==physical||canonical(step.conductorIdentity)!==conductors)fail('physical problem or conductor topology changed');
  const line=step.request.line as any;if(line.kind!=='polygon-section'||line.geometry?.loss)fail('PEC polygon coefficients only');
  const signature=canonical({frequencies:step.request.frequencies,lengthMm:line.lengthMm,signals:line.geometry?.conductors?.filter((c:any)=>c.role!=='reference'),roles:line.geometry?.conductors?.map((c:any)=>[c.id,c.role]),materials:line.geometry?.dielectrics?.map((d:any)=>[d.id,d.relativePermittivity])});if(invariant!==undefined&&signature!==invariant)fail('signal geometry/materials/ports changed');invariant=signature;
  const remaining=budget.maxSeconds-(performance.now()-start)/1000;if(remaining<=0)fail('shared deadline');
  const wall=performance.now();const r:any=execute({kind:'cross-section',request:{...step.request,budget:{maxSeconds:remaining,maxMemoryMiB:budget.maxMemoryMiB}}});
  if(cancelled())fail('cancelled');if(r.status!=='complete')fail('incomplete solve '+JSON.stringify(r.diagnostics));
  const p=r.parameters;const order=r.conductorOrder??p?.conductorOrder;if(!p||!Array.isArray(order)||!order.length||!r.network)fail('missing joint matrices/network');
  const n=order.length;if(!Array.isArray(p.modes)||p.modes.length!==n||!Array.isArray(r.network.parameters)||!r.network.parameters.length)fail('missing modes/response');
  for(const key of ['capacitanceFPerM','vacuumCapacitanceFPerM','inductanceHPerM'])if(!Array.isArray(p[key])||p[key].length!==n||p[key].some((row:any)=>!Array.isArray(row)||row.length!==n))fail('invalid joint matrix');
  const C=p.capacitanceFPerM,L=p.inductanceHPerM;let z:number[]=[];if(n===1)z=[Math.sqrt(L[0][0]/C[0][0])];else if(n===2){const det=C[0][0]*C[1][1]-C[0][1]*C[1][0];const a=(C[1][1]*L[0][0]-C[0][1]*L[1][0])/det,b=(C[1][1]*L[0][1]-C[0][1]*L[1][1])/det,c=(-C[1][0]*L[0][0]+C[0][0]*L[1][0])/det,d=(-C[1][0]*L[0][1]+C[0][0]*L[1][1])/det;const root=Math.sqrt((a-d)**2+4*b*c);z=[Math.sqrt((a+d-root)/2),Math.sqrt((a+d+root)/2)];}else fail('modal impedance admission currently supports one/two signals');
  const observables={C:flatten(p.capacitanceFPerM),C0:flatten(p.vacuumCapacitanceFPerM),L:flatten(p.inductanceHPerM),Z:flatten(z),S:flatten(r.network.parameters)};
  const topology=canonical([order,r.network.ports,r.frequenciesHz]);
  const floors={C:1e-15,C0:1e-15,L:1e-12,Z:.01,S:1e-4};let errors:Record<string,number>|undefined;
  if(previous&&step.axis!=='baseline'){if(previous.topology!==topology)fail('output topology/ports/axis changed');errors={};for(const k of Object.keys(observables) as (keyof typeof observables)[]){const a=observables[k],b=previous.observables[k];if(a.length!==b.length)fail('observable shape changed');errors[k]=Math.max(...a.map((v,i)=>Math.abs(v-b[i])/Math.max(Math.abs(v),Math.abs(b[i]),floors[k])));}const proof=proofs[index]!;const axes=passed.get(proof)??new Set<string>();if(Math.max(...Object.values(errors))<=.02)axes.add(step.axis);else axes.delete(step.axis);passed.set(proof,axes);}
  audit.push({index,axis:step.axis,errors,cache:r.cache,resources:r.resources,wallSeconds:(performance.now()-wall)/1000,convergence:r.convergence,observables});
  previous={topology,observables};
  if(proofs[index]&&['physical-width','air-boundary','mesh'].every(a=>passed.get(proofs[index]!)?.has(a)))return {status:'coefficients-only' as const,report:r,audit,limitations:['PEC C/L only; finite-R longitudinal return-current model and distributed interfaces D remain unvalidated'],wallSeconds:(performance.now()-start)/1000};
 }
 return {status:'unresolved' as const,audit,limitations:['boundary/mesh sensitivity exceeds 2%; no channel admission'],wallSeconds:(performance.now()-start)/1000};
}






