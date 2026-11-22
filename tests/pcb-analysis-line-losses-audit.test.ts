import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
import {auditLineLosses} from './helpers/pcb-analysis-line-losses-audit.ts';
import {sectionResults} from './helpers/pcb-analysis-cross-section-audit.ts';

for(const fixture of ['BandAmpV5','STM32G0_CAN_RS485'])test(`${fixture}: versioned loss audit rejects incomplete series, omitted returns, missing excitation, complex/matrix/material corruption`,async()=>{
 const dir=`tests/pcb-analyze/${fixture}`,board=JSON.parse(readFileSync(`${dir}/${fixture}.json`,'utf8')),expected=JSON.parse(readFileSync(`${dir}/impedance-losses.expectations.json`,'utf8'));
 const result=await runAnalysisDsl({board,models:JSON.parse(readFileSync('tests/fixtures/analysis-cross-section/attenuator-pad.json','utf8')),code:readFileSync(`${dir}/impedance-losses.js`,'utf8')},{timeoutMs:180000,maxOutputBytes:128*1024*1024});assert.equal(auditLineLosses(result,board,expected).status,'pass');
 const report=result.reports[0]as any,sections=sectionResults(result).filter((r:any)=>r.solution?.field),s=sections[3].solution;
 const target=report.plots.find((p:any)=>p.title==='combined single alpha'||p.title==='combined even alpha');assert.ok(target);
 const originalPlot=structuredClone(target),originalPlots=report.plots;
 for(const [label,change]of [
  ['empty',(p:any)=>p.data.values=[]],['truncate',(p:any)=>p.data.values.pop()],['extend',(p:any)=>p.data.values.push(0)],
  ['null',(p:any)=>p.data.values[0]=null],['NaN',(p:any)=>p.data.values[0]=NaN],['unit',(p:any)=>p.data.unit='Ohm'],['frequency',(p:any)=>p.data.axis.values[0]*=1.01],['validity',(p:any)=>p.data.validity.status='valid'],
 ]as const){Object.assign(target,structuredClone(originalPlot));change(target);assert.throws(()=>auditLineLosses(result,board,expected),undefined,label);}
 Object.assign(target,originalPlot);
 report.plots=[...originalPlots,structuredClone(target)];assert.throws(()=>auditLineLosses(result,board,expected),undefined,'duplicate');report.plots=originalPlots.filter((p:any)=>p!==target);assert.throws(()=>auditLineLosses(result,board,expected),undefined,'removed');report.plots=originalPlots;
 function damage(object:any,key:string,value:any,label:string){const before=object[key];object[key]=value;try{assert.throws(()=>auditLineLosses(result,board,expected),undefined,label);}finally{object[key]=before;}}
 const sample=s.parameters.frequencyDependent.samples[1];
 damage(sample,'surfaceContributions',sample.surfaceContributions.slice(0,-1),'omitted reference losses');damage(s.lossIntegrals,'surfaces',s.lossIntegrals.surfaces.slice(0,-1),'omitted reference currents');
 damage(s.lossIntegrals.surfaces[0],'currentAPerMPerA',[],'lost excitation');
 damage(s.lossIntegrals.surfaces[0],'edges',s.lossIntegrals.surfaces[0].edges.slice(1),'dropped corner edge');
 const convergence=s.convergence.find((v:any)=>v.lossConvergence);
 damage(convergence.lossConvergence,'maxNormalizedChange',0,'forged loss convergence');
 damage(convergence.lossConvergence.comparisons[0].comparison.normalizedChanges[0],0,0,'forged raw loss convergence');
 damage(sample,'resistanceOhmPerM',[...sample.resistanceOhmPerM,sample.resistanceOhmPerM[0]],'extra matrix row');
 const impedance=sections[3].characteristicImpedance;
 if(impedance.kind==='series')damage(impedance.values[0],'imaginary',0,'metric imaginary omitted');
 else damage(impedance.data.rows[0],'imaginary',0,'metric imaginary omitted');
 const responses=sectionResults(result).filter((r:any)=>r.solution?.representation==='S');
 const lastResponse=responses.at(-1).solution.parameters[0];
 damage(lastResponse[expected.tracks.length-1][expected.tracks.length-1],'real',.123456,'P6b full matrix corruption');
 damage(sample.modes[0].characteristicImpedanceOhm,'imaginary',-sample.modes[0].characteristicImpedanceOhm.imaginary,'complex imaginary sign');
 damage(sample,'frequencyHz',sample.frequencyHz*1.001,'coefficient frequency');damage(s.input,'lengthMm',s.input.lengthMm*2,'source length');damage(s.validity,'status','valid','conditional promoted');
 damage(sample.regionContributions[0],'lossTangent',0,'regional tangent removed');damage(sample.surfaceContributions.at(-1),'resistanceOhmPerM',Array.from({length:expected.tracks.length},()=>Array(expected.tracks.length).fill(0)),'reference loss zeroed');
 if(expected.tracks.length===2){damage(sample.resistanceOhmPerM[0],1,-sample.resistanceOhmPerM[0][1],'mutual sign');damage(s.lossIntegrals.surfaces[0],'currentAPerMPerA',s.lossIntegrals.surfaces[0].currentAPerMPerA.slice(0,1),'one pair excitation');}
 assert.equal(auditLineLosses(result,board,expected).status,'pass');
});
