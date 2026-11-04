import type { AnalysisBoardFoilCheckedReport } from '../../native/analyze/index.js';
import type { AutoFoilPlan } from './auto-foil-plan.ts';
import type { BoardFoilProblem } from './types.ts';
import type { FrequencyInterval } from './p7-frequency-planning.ts';

const fail=(message:string):never=>{throw new Error('CHANNEL_FOIL_EVIDENCE: '+message);};
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);

export function runAutoFoil(
 plan:AutoFoilPlan, frequenciesHz:number[], budget:{maxSeconds:number;maxMemoryMiB:number},
 interval:FrequencyInterval|undefined, execute:(problem:BoardFoilProblem)=>AnalysisBoardFoilCheckedReport,
){
 const n=plan.ports.length;
 if(!Number.isFinite(budget.maxSeconds)||budget.maxSeconds<=0)
  throw new Error('CHANNEL_FOIL_DEADLINE: shared budget exhausted before the foil solve');
 if(!n||n>16||!frequenciesHz.length||frequenciesHz.length*n>256||
    [...frequenciesHz,...(interval??[])].some(f=>!Number.isFinite(f)||f<1000||f>10000)||
    frequenciesHz.some((f,i)=>i>0&&f<=frequenciesHz[i-1]))
  throw new Error('CHANNEL_FOIL_GRID: bounded ordered 1-10 kHz grid with at most 256 frequency/port columns required');
 const referenceOhm=plan.ports.map(()=>50);
 const report=execute({kind:'board-foil-checked',boardJson:plan.boardJson,geometry:plan.geometry,
  request:{frequenciesHz,referenceOhm,budget}});
 if(report.status!=='complete'||report.coverage!=='unverified'||report.representation!=='S'||
    !same(report.frequenciesHz,frequenciesHz)||!same(report.referenceOhm,referenceOhm)||!same(report.ports,plan.electrodePairs))
  fail('native report did not retain the exact physical grid, ports and references');
 if(report.parameters.length!==frequenciesHz.length||report.samples.length!==frequenciesHz.length||
    report.parameters.some(m=>m.length!==n||m.some(row=>row.length!==n||row.some(s=>!Number.isFinite(s.real)||!Number.isFinite(s.imaginary))))||
    report.samples.some((sample,i)=>sample.frequencyHz!==frequenciesHz[i]||sample.columns.length!==n||
     sample.columns.some((column,j)=>column.input!==j)))
  fail('incomplete finite S matrix or residual evidence');
 const maxResidual=1e-8;
 const columns=report.samples.flatMap(sample=>sample.columns);
 if(columns.some(({kclRelative,kvlRelative,copperLossW,powerBalanceErrorW})=>
  [kclRelative,kvlRelative].some(x=>!Number.isFinite(x)||x<0||x>maxResidual)||
  [copperLossW,powerBalanceErrorW].some(x=>!Number.isFinite(x)||x<0)))
  fail('native foil residual or power evidence is outside its accepted limits');
 for(const [i,sample] of report.samples.entries())for(const column of sample.columns){
  const outgoing=report.parameters[i].reduce((sum,row)=>{
   const {real,imaginary}=row[column.input];
   return sum+real*real+imaginary*imaginary;
  },0);
  // A matched 1 A RMS Norton drive has incident power R/4.
  const wavePower=(referenceOhm[column.input]/4)*(1-outgoing);
  const threshold=1e-12+1e-8*(Math.abs(wavePower)+column.copperLossW);
  if(!Number.isFinite(threshold)||column.powerBalanceErrorW>threshold||
     Math.abs(wavePower-column.copperLossW)>threshold)
   fail('native foil wave power and copper loss do not pass the matched power gate');
 }
 const {quality}=report;
 if(quality.maxKclRelative!==Math.max(...columns.map(column=>column.kclRelative))||
    quality.maxKvlRelative!==Math.max(...columns.map(column=>column.kvlRelative))||
    quality.maxColumnPowerBalanceErrorW!==Math.max(...columns.map(column=>column.powerBalanceErrorW)))
  fail('native foil quality maxima do not match the per-column evidence');
 const evidence=report.provenance;
 const expected=plan.sourceCopper.map(r=>JSON.stringify([r.regionId,r.sourcePath,r.layer])).sort();
 const actual=evidence.sourceMapping.map(([id,path,,layer])=>JSON.stringify([id,path,layer])).sort();
 if(!same(expected,actual)||!Number.isInteger(evidence.prisms)||evidence.prisms<1||
    !Number.isFinite(evidence.plannedBytes)||evidence.plannedBytes<=0||evidence.plannedBytes>budget.maxMemoryMiB*1048576||
    !Number.isFinite(evidence.maxCopperVolumeChange)||evidence.maxCopperVolumeChange<0||evidence.maxCopperVolumeChange>1e-8||
    evidence.spatialCoverage!=='unverified'||evidence.bandCoverage!=='unverified'||
    new Set(plan.electrodePairs.flat()).size!==Object.keys(evidence.electrodeAreasM2).length||
    plan.electrodePairs.flat().some(name=>!Number.isFinite(evidence.electrodeAreasM2[name])||evidence.electrodeAreasM2[name]<=0))
  fail('native foil ownership, pad areas or scope do not match the closed candidate');
 return report;
}