import test from 'node:test';
import assert from 'node:assert/strict';
import {createElectrothermalExecutor} from '../src/analyze/analysis-dsl/electrothermal-executor.ts';
import type {AnalysisTask} from '../src/analyze/analysis-dsl/executor.ts';

// Standalone circuit + thermal network, no board. Voltage source feeds R1 (rho(T) law) into
// one thermal node with resistance Rth to ambient. Analytic: T - Ta = Rth*V^2/(R0*(1+a*(T-T0))).
const Ta=25,T0=25;
function task(o:{V?:number;R0?:number;alpha?:number;Rth?:number;source?:'voltage'|'current';I?:number;coupling?:Record<string,unknown>;extra?:Record<string,unknown>;couplings?:unknown[];elements?:unknown[];nodes?:unknown[];}={}):AnalysisTask<'electrothermal'>{
 const V=o.V??10,R0=o.R0??10,alpha=o.alpha??0.004,Rth=o.Rth??20;
 const law={referenceC:T0,temperatureCoefficientPerK:alpha};
 const source=o.source==='current'
  ?{id:'I1',kind:'current-source',nodes:['gnd','n1'],currentA:o.I??1}
  :{id:'V1',kind:'voltage-source',nodes:['n1','gnd'],voltageV:V,model:'ideal'};
 const elements=o.elements??[source,{id:'R1',kind:'resistor',nodes:['n1','gnd'],resistanceOhm:R0,...law}];
 const circuit={nodes:[...new Set(['gnd',...(elements as {nodes:string[]}[]).flatMap(e=>e.nodes)])],reference:'gnd',ports:[],elements};
 const network={nodes:o.nodes??[{id:'ambient',boundary:'environment'},{id:'junction',heatCapacityJPerK:1}],links:[{id:'th',from:'junction',to:'ambient',resistanceKPerW:Rth}],sources:[]};
 return {kind:'electrothermal',contextId:'ctx',config:{
  electrical:{circuit:{id:'circuit'}},thermal:{network:{id:'network'},environment:{ambientC:Ta},domain:'steady'},
  couplings:o.couplings??[{element:'R1',thermalNode:'junction'}],...(o.coupling?{coupling:o.coupling}:{}),...(o.extra??{}),
 } as never,variantChanges:[],descriptors:[{id:'circuit',input:circuit},{id:'network',input:network}]} as unknown as AnalysisTask<'electrothermal'>;
}
const run=(t:AnalysisTask<'electrothermal'>)=>createElectrothermalExecutor()(t);
type Sol={status:string;iterations:number;history:{inputTemperaturesC:Record<string,number>;outputTemperaturesC:Record<string,number>;electricalLossW:number}[];conservation:{electricalLossW:number;coupledThermalSourceW:number;relativeError:number};nodeTemperaturesC:Record<string,number>;elements:Record<string,{powerW:number;resistanceOhm:number}>};
const sol=(r:{solution?:unknown})=>r.solution as Sol;
/** Closed form: alpha x^2 + (1+alpha(Ta-T0)) x - K = 0, K = Rth V^2 / R0, T = Ta + x. */
function analytic(V:number,R0:number,alpha:number,Rth:number){
 const K=Rth*V*V/R0,b=1+alpha*(Ta-T0);
 const x=alpha===0?K:(-b+Math.sqrt(b*b+4*alpha*K))/(2*alpha);
 return {T:Ta+x,P:V*V/(R0*(1+alpha*(Ta+x-T0)))};
}

test('rho(T) resistor with thermal resistance matches the closed form (exact fixed point)',async()=>{
 const r=await run(task({coupling:{toleranceK:1e-7,relativeLossTolerance:1e-10,maxIterations:80}}));
 const s=sol(r);
 const exact=analytic(10,10,0.004,20);
 assert.equal(r.status,'complete',JSON.stringify(r.diagnostics));
 assert.equal(s.status,'converged');
 assert.ok(Math.abs(s.nodeTemperaturesC.junction-exact.T)<1e-5,`T ${s.nodeTemperaturesC.junction} vs ${exact.T}`);
 assert.ok(Math.abs(s.elements.R1.powerW-exact.P)<1e-6*exact.P,`P ${s.elements.R1.powerW} vs ${exact.P}`);
 assert.ok(s.iterations>2&&s.iterations<80);
});

test('constant coefficient (alpha 0) gives the uncoupled result in one pass',async()=>{
 const r=await run(task({alpha:0,V:10,R0:10,Rth:20}));
 const s=sol(r);
 assert.equal(r.status,'complete');
 assert.equal(s.iterations,1);
 assert.equal(s.history.length,1);
 // Uncoupled: P = V^2/R0 = 10 W, T = Ta + Rth*P = 225 degC
 assert.ok(Math.abs(s.nodeTemperaturesC.junction-225)<1e-9,String(s.nodeTemperaturesC.junction));
 assert.ok(Math.abs(s.elements.R1.powerW-10)<1e-9);
});

test('electrical loss equals coupled thermal source to 1e-9 relative for two coupled resistors on two nodes',async()=>{
 const elements=[
  {id:'V1',kind:'voltage-source',nodes:['n1','gnd'],voltageV:6,model:'ideal'},
  {id:'R1',kind:'resistor',nodes:['n1','a'],resistanceOhm:4,referenceC:25,temperatureCoefficientPerK:0.003},
  {id:'R2',kind:'resistor',nodes:['a','gnd'],resistanceOhm:8,referenceC:25,temperatureCoefficientPerK:0.005},
 ];
 const nodes=[{id:'ambient',boundary:'environment'},{id:'hot',heatCapacityJPerK:1},{id:'cold',heatCapacityJPerK:1}];
 const t=task({elements,nodes,couplings:[{element:'R1',thermalNode:'hot'},{element:'R2',thermalNode:'cold'}],coupling:{toleranceK:1e-6,maxIterations:80}});
 // second link on the cold node
 const net=(t.descriptors[1] as {input:{links:unknown[]}}).input;
 net.links=[{id:'th1',from:'hot',to:'ambient',resistanceKPerW:15},{id:'th2',from:'cold',to:'ambient',resistanceKPerW:30}];
 const r=await run(t);const s=sol(r);
 assert.equal(r.status,'complete',JSON.stringify(r.diagnostics));
 assert.ok(s.conservation.relativeError<=1e-9,`relative error ${s.conservation.relativeError}`);
 assert.ok(Math.abs(s.conservation.electricalLossW-s.conservation.coupledThermalSourceW)<=1e-9*s.conservation.electricalLossW);
 const sum=s.elements.R1.powerW+s.elements.R2.powerW;
 assert.ok(Math.abs(sum-s.conservation.electricalLossW)<=1e-9*sum);
 assert.ok(s.elements.R2.powerW>s.elements.R1.powerW&&s.nodeTemperaturesC.cold>s.nodeTemperaturesC.hot);
});

test('forced thermal runaway (current source, Rth*I^2*R0*alpha = 2.5 > 1) reports ELECTROTHERMAL_RUNAWAY',async()=>{
 const r=await run(task({source:'current',I:1,R0:1,alpha:0.05,Rth:50}));
 assert.equal(r.status,'failed');
 assert.equal(r.diagnostics[0].code,'ELECTROTHERMAL_RUNAWAY');
 const s=sol(r);assert.ok(s.history.length>=3&&s.history.length<=30);
 const losses=s.history.map(h=>h.electricalLossW);
 assert.ok(losses.every((v,i)=>i===0||v>losses[i-1]),'losses increase monotonically');
});

test('contracting coupling with too few passes reports NOT_CONVERGED with history, not runaway',async()=>{
 const r=await run(task({coupling:{maxIterations:2,toleranceK:1e-9}}));
 assert.equal(r.status,'failed');
 assert.equal(r.diagnostics[0].code,'ELECTROTHERMAL_NOT_CONVERGED');
 assert.equal(sol(r).history.length,2);
 assert.equal(r.metrics.losses,undefined);
});

test('coupling tolerance changes pass count and result by no more than the loose tolerance allows',async()=>{
 const loose=sol(await run(task({coupling:{toleranceK:0.5,maxIterations:80}})));
 const tight=sol(await run(task({coupling:{toleranceK:1e-6,maxIterations:80}})));
 const d=Math.abs(loose.nodeTemperaturesC.junction-tight.nodeTemperaturesC.junction);
 assert.ok(loose.iterations<tight.iterations,`${loose.iterations} vs ${tight.iterations}`);
 assert.ok(d>0&&d<0.5,`difference ${d} K`);
});

test('transient coupling and board targets are refused with explicit codes',async()=>{
 await assert.rejects(run(task({extra:{time:{stopS:1,sampleStepS:0.1}}})),(e:{code?:string})=>e.code==='ELECTROTHERMAL_TRANSIENT_NOT_IMPLEMENTED');
 await assert.rejects(run(task({couplings:[{target:{kind:'component',designator:'Q1'},thermalNode:'junction'}]})),(e:{code?:string})=>e.code==='ELECTROTHERMAL_UNSUPPORTED_COUPLING');
 await assert.rejects(run(task({couplings:[{element:'V1',thermalNode:'junction'}]})),(e:{code?:string})=>e.code==='ELECTROTHERMAL_UNSUPPORTED_COUPLING');
});
