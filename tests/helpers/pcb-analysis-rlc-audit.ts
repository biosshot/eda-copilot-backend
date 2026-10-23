import assert from 'node:assert/strict';
import type {AnalysisDslOutput} from '../../src/analyze/run-analysis-dsl.ts';
import type {AnalysisElectricalReport} from '../../native/analyze/index.js';
export interface RlcExpectations {voltageV:number;resistanceOhm:number;inductanceH:number;seriesResistanceOhm:number;capacitanceF:number;eslH:number;esrOhm:number[];initialA:number;loadA:number[]}
export const rlcCaseIds=['nominal/dc','nominal/ac','nominal/time','raised/dc','raised/ac','raised/time','refined/time','continued/time'];
type Calculation={id:string;solution:AnalysisElectricalReport};
export function electricalCalculations(r:AnalysisDslOutput):Calculation[]{return r.reports.flatMap(r=>r.calculations).filter((c):c is Record<string,any>=>!!c&&typeof c==='object'&&!Array.isArray(c)&&'phase'in c&&c.phase==='completed').map(c=>c.result);}
/** Closed-form reference, not a second network solver. R+DCR+jwL in
 * parallel with ESR+jwESL+1/jwC, with an explicitly shorted ideal source. */
export function referenceZ(e:RlcExpectations,esr:number,f:number):[number,number]{
 const w=2*Math.PI*f;const inv=([re,im]:[number,number]):[number,number]=>{const d=re*re+im*im;return[re/d,-im/d];};
 const a=inv([e.resistanceOhm+e.seriesResistanceOhm,w*e.inductanceH]),b=inv([esr,w*e.eslH-1/(w*e.capacitanceF)]);return inv([a[0]+b[0],a[1]+b[1]]);
}
export function auditRlcReport(result:AnalysisDslOutput,e:RlcExpectations){
 assert.equal(result.status,'complete',JSON.stringify(result.error));
 const all=electricalCalculations(result),byId=new Map(all.map(r=>[r.id,r]));assert.equal(all.length,8);assert.equal(byId.size,8);
 const cases=result.reports.flatMap(r=>r.logs).filter((l):l is Record<string,any>=>!!l&&typeof l==='object'&&!Array.isArray(l)&&l.message==='rlc.case').map(l=>l.data);
 assert.deepEqual(cases.map(c=>c.caseId),rlcCaseIds);assert.equal(new Set(cases.map(c=>c.resultId)).size,8);
 const byCase=new Map<string,Calculation>();let maxZError=0;
 const finite=(v:unknown)=>{assert.equal(typeof v,'number');assert.ok(Number.isFinite(v as number));return v as number;};
 const axes=(axis:number[],unit:string)=>{assert.ok(axis.length>1);axis.forEach((v,i)=>{finite(v);if(i)assert.ok(v>axis[i-1]);});assert.ok(unit==='s'||unit==='Hz');};
 for(const c of cases){const calc=byId.get(c.resultId);assert.ok(calc,'Case references unknown solution');byCase.set(c.caseId,calc);const s=calc.solution;assert.equal(s.status,'complete');assert.equal(s.mode,c.caseId.endsWith('/dc')?'dc':c.caseId.endsWith('/ac')?'frequency':'time');assert.equal(s.request!.contextId,result.reports[0].contextId);
  assert.ok(s.quality);Object.values(s.quality).forEach(finite);assert.ok(s.quality.relativeResidual<=1e-8);assert.ok(s.quality.kclAbsA<=1e-10);assert.ok(s.counters);assert.ok(s.counters.solves>0);
  const elements=s.request!.circuit.elements;const get=(id:string)=>{const found=elements.filter(x=>x.id===id);assert.equal(found.length,1);return found[0];};
  const raised=c.caseId.startsWith('raised');const esr=e.esrOhm[raised?1:0];assert.equal(get('V1').voltageV,e.voltageV);assert.equal(get('R1').resistanceOhm,e.resistanceOhm);assert.equal(get('L1').inductanceH,e.inductanceH);assert.equal(get('L1').seriesResistanceOhm,e.seriesResistanceOhm);assert.equal(get('C1').capacitanceF,e.capacitanceF);assert.equal(get('C1').eslH,e.eslH);assert.equal(get('C1').esrOhm,esr);assert.equal(get('I1').currentA,e.initialA);assert.deepEqual(get('I1').waveform,{kind:'step',from:e.initialA,to:e.loadA[raised?1:0],atS:100e-6,riseS:1e-6});
  if(s.mode==='dc'){assert.ok(Math.abs(finite(s.nodeVoltages!.load)-(e.voltageV-e.initialA*(e.resistanceOhm+e.seriesResistanceOhm)))<=1e-8);assert.ok(Math.abs(finite(s.currents!.V1)+e.initialA)<=1e-8);assert.ok(Math.abs(s.quality.powerErrorW)<=1e-12+1e-8*s.quality.dissipationW);}
  else{assert.ok(s.axis);axes(s.axis,s.mode==='frequency'?'Hz':'s');
   if(s.mode==='frequency'){assert.equal(s.axis.length,201);const z=s.impedances!.output;assert.equal(z.length,s.axis.length);for(let i=0;i<z.length;i++){const ref=referenceZ(e,esr,s.axis[i]),err=Math.hypot(finite(z[i].real)-ref[0],finite(z[i].imaginary)-ref[1])/Math.max(1e-9,Math.hypot(...ref));maxZError=Math.max(maxZError,err);assert.ok(err<=1e-6,`Complex Z ${err}`);}assert.ok(s.quality.relativeComplexPowerError<=1e-8);}
   else{assert.ok(s.quality.relativeEnergyError<=.005,'Transient energy guard');assert.ok(s.quality.maxAcceptedLocalError<=1);assert.ok(s.counters.acceptedSteps>0);for(const values of [...Object.values(s.nodeVoltages!),...Object.values(s.currents!),s.losses!]){assert.ok(Array.isArray(values));assert.equal(values.length,s.axis.length);values.forEach(finite);}assert.ok(s.finalState);assert.equal((s.finalState as any).timeS,s.axis.at(-1));assert.ok((s.finalState as any).storage.C1);assert.ok((s.finalState as any).storage.L1);}
  }
 }
 const nominal=byCase.get('nominal/time')!.solution,refined=byCase.get('refined/time')!.solution,continued=byCase.get('continued/time')!.solution;
 assert.deepEqual(nominal.axis,refined.axis);assert.ok(refined.quality!.relativeTolerance<nominal.quality!.relativeTolerance);const a=nominal.nodeVoltages!.load as number[],b=refined.nodeVoltages!.load as number[];
 const scale=Math.max(.001,...b.map(v=>Math.abs(e.voltageV-v)));const refinementError=Math.max(...a.map((v,i)=>Math.abs(v-b[i])))/scale;assert.ok(refinementError<=.005,`Refinement ${refinementError}`);assert.ok(refined.quality!.relativeEnergyError<=nominal.quality!.relativeEnergyError+1e-12);
 assert.equal(continued.axis![0],refined.axis!.at(-1));assert.equal((continued.nodeVoltages!.load as number[])[0],b.at(-1));assert.deepEqual(continued.request!.initial,refined.finalState);assert.equal(continued.axis!.at(-1),1e-3);
 const checks=result.reports.flatMap(r=>r.checks);assert.deepEqual(checks.map(c=>[c.name,c.status]),[['nominal/DC','pass'],['nominal/strict droop','fail'],['nominal/after fail','pass'],['raised/DC','pass'],['raised/strict droop','fail'],['raised/after fail','pass']]);checks.forEach(c=>{assert.equal((c.actual as any).validity.status,'valid');assert.equal((c.actual as any).unit,c.expected&&typeof c.expected==='object'&&!Array.isArray(c.expected)?c.expected.unit:undefined);});
 const plots=result.reports.flatMap(r=>r.plots)as any[];assert.equal(plots.length,12);
 for(const plot of plots){const d=plot.data;assert.equal(d.kind,'series');assert.equal(d.validity.status,'valid');assert.ok(['V','A','Ohm','deg'].includes(d.unit));assert.ok(d.values);assert.ok(d.axis);assert.equal(d.axis.unit,d.axis.name==='timeS'?'s':'Hz');axes(d.axis.values,d.axis.unit);assert.equal(d.values.length,d.axis.values.length);d.values.forEach(finite);
  const [label,field]=plot.title.split(':');const s=byCase.get(label)!.solution;assert.deepEqual(d.axis.values,s.axis);
  const expected=field==='voltage'?s.nodeVoltages!.load:field==='current'?s.currents!.L1:field==='magnitude'?s.impedances!.output.map(z=>Math.hypot(z.real,z.imaginary)):s.impedances!.output.map(z=>Math.atan2(z.imaginary,z.real)*180/Math.PI);
  assert.deepEqual(d.values,expected);assert.equal(d.unit,field==='voltage'?'V':field==='current'?'A':field==='magnitude'?'Ohm':'deg');
 }
 return{cases:8,plots:plots.length,checks:checks.length,expectedFailures:2,maxComplexZRelativeError:maxZError,refinementRelativeError:refinementError,maxEnergyRelativeError:Math.max(...all.filter(x=>x.solution.mode==='time').map(x=>x.solution.quality!.relativeEnergyError))};
}
