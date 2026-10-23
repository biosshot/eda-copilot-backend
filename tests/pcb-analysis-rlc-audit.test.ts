import test,{before}from'node:test';
import assert from'node:assert/strict';
import{readFileSync}from'node:fs';
import{runAnalysisDsl,type AnalysisDslOutput}from'../src/analyze/run-analysis-dsl.ts';
import{auditRlcReport,type RlcExpectations}from'./helpers/pcb-analysis-rlc-audit.ts';
const fixture='tests/pcb-analyze/STM32G0_CAN_RS485/';
const expected=JSON.parse(readFileSync(fixture+'rlc.expectations.json','utf8'))as RlcExpectations;
let result:AnalysisDslOutput;
before(async()=>{result=await runAnalysisDsl({code:readFileSync(fixture+'rlc.js','utf8'),board:JSON.parse(readFileSync(fixture+'STM32G0_CAN_RS485.json','utf8'))},{timeoutMs:120000,maxOutputBytes:32*1024*1024});assert.equal(result.status,'complete',JSON.stringify(result.error));auditRlcReport(result,expected);});
test('production authored RLC scenario passes audit including two intentional fails and following plots',()=>{assert.equal(auditRlcReport(result,expected).cases,8);});
const all=(r:any)=>r.reports[0].calculations.filter((c:any)=>c.phase==='completed');
const cases=(r:any)=>r.reports[0].logs.filter((l:any)=>l.message==='rlc.case');
for(const[name,corrupt]of Object.entries<(r:any)=>void>({
 'missing case':r=>cases(r).pop()&&r.reports[0].logs.pop(),
 'duplicate identity':r=>{cases(r)[1].data.caseId=cases(r)[0].data.caseId;},
 'dangling result':r=>{cases(r)[0].data.resultId='unknown';},
 'missing result':r=>{r.reports[0].calculations=r.reports[0].calculations.filter((c:any)=>c!==all(r)[0]);},
 'rejected solver':r=>{all(r)[0].result.solution.status='rejected';},
 'wrong model':r=>{all(r)[0].result.solution.request.circuit.elements[1].resistanceOhm=1;},
 'NaN voltage':r=>{all(r)[2].result.solution.nodeVoltages.load[3]=NaN;},
 'missing voltage':r=>{delete all(r)[2].result.solution.nodeVoltages.load;},
 'axis cardinality':r=>{all(r)[2].result.solution.axis.pop();},
 'axis unordered':r=>{all(r)[1].result.solution.axis[1]=0;},
 'AC imaginary error':r=>{all(r)[1].result.solution.impedances.output[1].imaginary+=.01;},
 'residual guard':r=>{all(r)[0].result.solution.quality.relativeResidual=1;},
 'energy guard':r=>{all(r)[2].result.solution.quality.relativeEnergyError=1;},
 'nonfinite quality':r=>{all(r)[2].result.solution.quality.energyErrorJ=Infinity;},
 'lost continuation':r=>{delete all(r)[7].result.solution.request.initial;},
 'restarted time':r=>{all(r)[7].result.solution.axis[0]=0;},
 'missing storage state':r=>{delete all(r)[6].result.solution.finalState.storage.C1;},
 'wrong check unit':r=>{r.reports[0].checks[0].actual.unit='1';},
 'unavailable check':r=>{r.reports[0].checks[0].actual.validity.status='unavailable';},
 'lost expected fail':r=>{r.reports[0].checks[1].status='pass';},
 'lost following check':r=>{r.reports[0].checks.splice(2,1);},
 'plot unit':r=>{r.reports[0].plots[0].data.unit='V';},
 'plot axis unit':r=>{r.reports[0].plots[0].data.axis.unit='s';},
 'plot axis identity':r=>{r.reports[0].plots[0].data.axis.values[0]=99;},
 'plot values missing':r=>{delete r.reports[0].plots[0].data.values;},
 'plot imaginary transformed wrong':r=>{r.reports[0].plots[0].data.values[2]=0;},
 'lost plot after fail':r=>{r.reports[0].plots.pop();},
 'bad refinement':r=>{all(r)[6].result.solution.nodeVoltages.load[5]-=.1;},
})){test(`saved RLC audit rejects ${name}`,()=>{const copy=structuredClone(result);corrupt(copy);assert.throws(()=>auditRlcReport(copy,expected));});}
