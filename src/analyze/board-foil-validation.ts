import type { AnalysisBoardFoilCheckedReport, AnalysisBoardFoilCheckedRequest } from '../../native/analyze/index.js';
import type { BoardFoilProblem } from './types.ts';

const fail=(message:string):never=>{throw new Error('CHANNEL_FOIL_EVIDENCE: '+message);};
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);

export function validateBoardFoilReport(
 report: AnalysisBoardFoilCheckedReport, problem: BoardFoilProblem,
 producerBudget: AnalysisBoardFoilCheckedRequest['budget'],
): void {
 const { expected, request } = problem;
 const { frequenciesHz } = request;
 const n = expected.electrodePairs.length;
 const referenceOhm = request.referenceOhm ?? expected.electrodePairs.map(() => 50);
 if(report?.status!=='complete'||report.coverage!=='unverified'||report.representation!=='S'||
    !same(report.frequenciesHz,frequenciesHz)||!same(report.referenceOhm,referenceOhm)||!same(report.ports,expected.electrodePairs))
  fail('native report did not retain the exact physical grid, ports and references');
 if(!Array.isArray(report.parameters)||!Array.isArray(report.samples)||report.parameters.length!==frequenciesHz.length||report.samples.length!==frequenciesHz.length||
    report.parameters.some(m=>!Array.isArray(m)||m.length!==n||m.some(row=>!Array.isArray(row)||row.length!==n||row.some(s=>!s||!Number.isFinite(s.real)||!Number.isFinite(s.imaginary))))||
    report.samples.some((sample,i)=>!sample||sample.frequencyHz!==frequenciesHz[i]||!Array.isArray(sample.columns)||sample.columns.length!==n||
     sample.columns.some((column,j)=>!column||column.input!==j)))
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
 if(!quality||quality.maxKclRelative!==Math.max(...columns.map(column=>column.kclRelative))||
    quality.maxKvlRelative!==Math.max(...columns.map(column=>column.kvlRelative))||
    quality.maxColumnPowerBalanceErrorW!==Math.max(...columns.map(column=>column.powerBalanceErrorW)))
  fail('native foil quality maxima do not match the per-column evidence');
 const evidence=report.provenance;
 if(!evidence||!Array.isArray(evidence.sourceMapping)||!evidence.electrodeAreasM2||
    !Number.isFinite(evidence.maxSeconds)||evidence.maxSeconds<=0||evidence.maxSeconds>producerBudget.maxSeconds||
    !Number.isFinite(evidence.maxMemoryMiB)||evidence.maxMemoryMiB<1||evidence.maxMemoryMiB>producerBudget.maxMemoryMiB||
    evidence.plannedBytes>evidence.maxMemoryMiB*1048576)
  fail('native producer limits or ownership evidence missing or inconsistent');
 const expectedCopper=expected.sourceCopper.map(r=>JSON.stringify([r.regionId,r.sourcePath,r.layer])).sort();
 const actual=evidence.sourceMapping.map(([id,path,,layer])=>JSON.stringify([id,path,layer])).sort();
 const expectedBarrels=expected.sourceBarrels.map(barrel=>barrel.barrelId).sort();
 const validModel=evidence.modelKind==='foil'
  ? !expectedBarrels.length&&evidence.prisms>=1
  : evidence.modelKind==='mixed'&&Number.isSafeInteger(evidence.volumeTetrahedra)&&
    evidence.volumeTetrahedra>0&&Number.isSafeInteger(evidence.nativeCopperTetrahedra)&&
    Number.isSafeInteger(evidence.fullPotentialCount)&&evidence.fullPotentialCount>0&&
    evidence.fullCurrentCount===3*evidence.nativeCopperTetrahedra&&
    (evidence.prisms===0?evidence.nativeCopperTetrahedra===evidence.volumeTetrahedra:
     evidence.nativeCopperTetrahedra-evidence.volumeTetrahedra>=evidence.prisms)&&
    evidence.magneticQuadratureCoverage==='unverified';
 if(!same(expectedCopper,actual)||!validModel||!Number.isSafeInteger(evidence.prisms)||evidence.prisms<0||
    !Array.isArray(evidence.barrelIds)||!same(expectedBarrels,[...evidence.barrelIds].sort())||
    !Number.isFinite(evidence.plannedBytes)||evidence.plannedBytes<=0||evidence.plannedBytes>producerBudget.maxMemoryMiB*1048576||
    !Number.isFinite(evidence.maxCopperVolumeChange)||evidence.maxCopperVolumeChange<0||evidence.maxCopperVolumeChange>1e-8||
    evidence.spatialCoverage!=='unverified'||evidence.bandCoverage!=='unverified'||
    new Set(expected.electrodePairs.flat()).size!==Object.keys(evidence.electrodeAreasM2).length||
    expected.electrodePairs.flat().some(name=>!Number.isFinite(evidence.electrodeAreasM2[name])||evidence.electrodeAreasM2[name]<=0))
  fail('native copper and barrel ownership, pad areas or scope do not match the closed candidate');
}
