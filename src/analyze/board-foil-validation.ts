import type { AnalysisBoardFoilCheckedReport, AnalysisBoardFoilCheckedRequest } from '../../native/analyze/index.js';
import type { BoardFoilProblem } from './types.ts';
import type { CoupledRlcData } from './analysis-dsl/spec-doc.js';

const fail=(message:string):never=>{throw new Error('CHANNEL_FOIL_EVIDENCE: '+message);};
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
const positive=(value:unknown):value is number=>typeof value==='number'&&Number.isFinite(value)&&value>0;

function validateMagneticOperator(report:AnalysisBoardFoilCheckedReport):void {
 const p=report.provenance,operator=p.magneticOperator;
 const columns=report.samples.flatMap(sample=>sample.columns);
 if(p.modelKind==='foil'){
  if(operator!==undefined||report.quality.maxFullKvlUncertaintyRelative!==undefined||
     columns.some(column=>column.kvlUncertaintyRelative!==undefined))
   fail('unexpected mixed magnetic operator certificate on a foil result');
  return;
 }
 const uncertainty=operator?.maxFullKvlUncertaintyRelative;
 if(!operator||typeof operator.scope!=='string'||!operator.scope.trim()||
    typeof uncertainty!=='number'||!Number.isFinite(uncertainty)||uncertainty<0||uncertainty>1e-8||
    columns.some(column=>typeof column.kvlUncertaintyRelative!=='number'||
     !Number.isFinite(column.kvlUncertaintyRelative)||column.kvlUncertaintyRelative<0||
     column.kvlUncertaintyRelative>column.kvlRelative)||
    report.quality.maxFullKvlUncertaintyRelative!==uncertainty||
    Math.max(...columns.map(column=>column.kvlUncertaintyRelative!))!==uncertainty)
  return fail('missing or inconsistent original-coordinate magnetic KVL uncertainty');
 if(operator.policy==='exact-streamed-v1'){
  if(operator.method!=='unchanged direct tetrahedral pairs'||uncertainty!==0)
   fail('exact magnetic operator unexpectedly reports truncation uncertainty');
  return;
 }
 const count=(value:unknown):value is number=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=0;
 if(operator.policy!=='certified-cartesian-tree-v1'||operator.method!=='symmetric dual-tree Newton expansion'||
    operator.expansionOrder!==10||operator.separationRatioMaximum!==0.2||
    !count(operator.treeNodes)||operator.treeNodes<1||
    !count(operator.exactPairs)||operator.exactPairs<1||!count(operator.farClusterPairs)||
    !count(operator.plannedBytes)||operator.plannedBytes<1||operator.plannedBytes>p.plannedBytes||
    !/^fnv1a64:[0-9a-f]{16}$/.test(operator.treeSignature))
  fail('unsupported or incomplete certified magnetic tree identity');
}

function validatePartition(report:AnalysisBoardFoilCheckedReport,problem:BoardFoilProblem):void {
 const p=report.provenance,partition=p.automaticPartition,correction=report.conductiveCorrection;
 const policy=problem.request.partitionPolicy;
 if(policy===undefined){
  if(p.partitionPolicy!==undefined||partition!==undefined||correction!==undefined)
   fail('unexpected conductive partition evidence');
  return;
 }
 const count=(value:unknown):value is number=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=0;
 if(policy!=='automatic-conductive-schur-v1'||p.partitionPolicy!==policy||p.modelKind!=='mixed'||
    !partition||partition.policy!==policy||partition.algorithm!=='longest-extent-midpoint-v1'||
    !['selected','not-applicable'].includes(partition.status)||typeof partition.reason!=='string'||!partition.reason.trim()||
    !/^fnv1a64:[0-9a-f]{16}$/.test(partition.meshOwnershipSignature)||
    partition.originalCopperTetrahedra!==p.nativeCopperTetrahedra||
    ![partition.owners,partition.originalCopperTetrahedra,partition.crossOwnerFaces,partition.freeSharedTraceDofs].every(count)||
    ![0,2].includes(partition.owners)||!Array.isArray(partition.ownerInteriorDofs)||
    partition.ownerInteriorDofs.length!==partition.owners||!partition.ownerInteriorDofs.every(count)||
    partition.freeSharedTraceDofs>p.fullPotentialCount||
    partition.ownerInteriorDofs.reduce((sum,n)=>sum+n,partition.freeSharedTraceDofs)>p.fullPotentialCount)
  return fail('missing or inconsistent original-mesh automatic partition evidence');
 if(partition.status==='not-applicable'){
  if(correction!==undefined)fail('inapplicable partition unexpectedly reports a Schur solve');
  if(partition.owners===2&&partition.crossOwnerFaces>0&&partition.freeSharedTraceDofs>0&&
     partition.ownerInteriorDofs.every(n=>n>0))
   fail('inapplicable partition contradicts native eligibility');
  return;
 }
 if(partition.owners!==2||partition.crossOwnerFaces<1||partition.freeSharedTraceDofs<1||
    !partition.ownerInteriorDofs.every(n=>n>0)||!correction||
    correction.method!=='sparse owner interiors and shared conductive trace Schur'||
    !same(correction.ownerInteriorDofs,partition.ownerInteriorDofs)||
    correction.freeSharedTraceDofs!==partition.freeSharedTraceDofs||
    !count(correction.traceUpperEntries)||correction.traceUpperEntries<1||
    !count(correction.ownedPlannedBytes)||correction.ownedPlannedBytes<1||
    correction.ownedPlannedBytes>p.plannedBytes||
    !Number.isFinite(correction.maxOriginalFreeResidualRelative)||
    correction.maxOriginalFreeResidualRelative<0||correction.maxOriginalFreeResidualRelative>1e-8||
    typeof correction.scope!=='string'||!correction.scope.trim())
  fail('selected automatic partition lacks checked original-coordinate Schur evidence');
}

function validateSkinMesh(provenance:AnalysisBoardFoilCheckedReport['provenance'],problem:BoardFoilProblem):void {
 const {request,expected}=problem;
 const mesh=provenance.copperSkinMesh;
 if(request.operatorPolicy===undefined){
  if(provenance.operatorPolicy!==undefined||mesh!==undefined)fail('unexpected skin mesh policy evidence');
  return;
 }
 const requestedUpperHz=request.frequenciesHz.at(-1);
 const effectiveUpperHz=request.profileUpperHz??requestedUpperHz;
 if(request.operatorPolicy!=='skin-volume-v3'||provenance.operatorPolicy!==request.operatorPolicy||
    provenance.modelKind!=='mixed'||!mesh||mesh.version!==3||mesh.requestedUpperHz!==requestedUpperHz||
    mesh.trainingUpperBoundHz!==requestedUpperHz||mesh.effectiveUpperHz!==effectiveUpperHz||
    mesh.skinDepthDivisor!==3||mesh.currentAndElectricShareMesh!==true||
    typeof mesh.scope!=='string'||!mesh.scope.trim())
  return fail('missing or inconsistent shared copper skin mesh provenance');
 const geometry=problem.geometry as {
  stack?: {name:string;copper:boolean;conductivitySM?:number}[];
  barrelConductivitySM?:number;
 };
 const layers=Array.isArray(geometry.stack)?geometry.stack.filter(layer=>layer?.copper===true):[];
 const barrels=expected.sourceBarrels.map(barrel=>barrel.barrelId).sort();
 const validSpacing=(skinDepthM:number,stepM:number,pieces:number,conductivity:unknown)=>{
  if(!positive(skinDepthM)||!positive(stepM)||!Number.isSafeInteger(pieces)||pieces<1||!positive(conductivity))return false;
  const depth=1/(Math.sqrt(Math.PI*(4e-7*Math.PI)*mesh.effectiveUpperHz)*Math.sqrt(conductivity));
  return positive(depth)&&Math.abs(skinDepthM/depth-1)<=1e-8&&stepM<=skinDepthM/3*(1+1e-8);
 };
 if(!layers.length||!Array.isArray(mesh.layers)||mesh.layers.length!==layers.length||
    mesh.layers.some((row,i)=>!row||row.layer!==layers[i].name||
     !validSpacing(row.skinDepthM,row.maximumStepM,row.slabs,layers[i].conductivitySM))||
    !Array.isArray(mesh.roundBarrels)||mesh.roundBarrels.length!==barrels.length||
    mesh.roundBarrels.some(row=>!row||typeof row.id!=='string'||
     !validSpacing(row.skinDepthM,row.maximumRadialStepM,row.shells,geometry.barrelConductivitySM))||
    !same(mesh.roundBarrels.map(row=>row.id).sort(),barrels))
  fail('copper skin mesh rows or physical spacing do not match the candidate geometry');
}

export function validateBoardFoilReport(
 report: AnalysisBoardFoilCheckedReport, problem: BoardFoilProblem,
 producerBudget: AnalysisBoardFoilCheckedRequest['budget'],
): void {
 const { expected, request } = problem;
 const { frequenciesHz } = request;
 const attachments=(request.attachments??[]).map(({id,electrodes,model})=>{
  const entries=(model.data as CoupledRlcData|undefined)?.resistanceOhm;
  if(!Array.isArray(entries)||entries.length>1||entries.some(entry=>
   !Array.isArray(entry)||entry.length!==3||entry[0]!==0||entry[1]!==0||!Number.isFinite(entry[2])||entry[2]<=0))
   return fail('invalid authored series package resistance evidence');
  return {id,electrodes,resistanceOhm:entries[0]?.[2]??0};
 });
 const electrodeNames=new Set([...expected.electrodePairs.flat(),...attachments.flatMap(a=>a.electrodes)]);
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
  const packages=column.packages??[];
  if(!Array.isArray(packages)||packages.length!==attachments.length||packages.some((value,j)=>
   !value||value.id!==attachments[j].id||!value.currentA||!Number.isFinite(value.currentA.real)||
   !Number.isFinite(value.currentA.imaginary)||!Number.isFinite(value.lossW)||value.lossW<0))
   fail('missing or invalid ordered private package current and loss evidence');
  for(const [j,{currentA,lossW}] of packages.entries()){
   const resistance=attachments[j].resistanceOhm;
   const weight=Math.sqrt(resistance);
   const lossFromCurrent=Math.hypot(weight*currentA.real,weight*currentA.imaginary)**2;
   const tolerance=1e-12+1e-8*(lossFromCurrent+lossW);
   if(!Number.isFinite(tolerance)||(resistance===0?lossW!==0:Math.abs(lossFromCurrent-lossW)>tolerance))
    fail('package dissipation does not match authored series resistance and reported current');
  }
  const passiveLoss=packages.reduce((total,value)=>total+value.lossW,column.copperLossW);
  const outgoing=report.parameters[i].reduce((sum,row)=>{
   const {real,imaginary}=row[column.input];
   return sum+real*real+imaginary*imaginary;
  },0);
  // A matched 1 A RMS Norton drive has incident power R/4.
  const wavePower=(referenceOhm[column.input]/4)*(1-outgoing);
  const threshold=1e-12+1e-8*(Math.abs(wavePower)+passiveLoss);
  if(!Number.isFinite(threshold)||column.powerBalanceErrorW>threshold||
     Math.abs(wavePower-passiveLoss)>threshold)
   fail('native foil wave power and separate copper/package losses do not pass the matched power gate');
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
 validateSkinMesh(evidence,problem);
 validatePartition(report,problem);
 validateMagneticOperator(report);
 const expectedCopper=expected.sourceCopper.map(r=>JSON.stringify([r.regionId,r.sourcePath,r.layer])).sort();
 const actual=evidence.sourceMapping.map(([id,path,,layer])=>JSON.stringify([id,path,layer])).sort();
 const expectedBarrels=expected.sourceBarrels.map(barrel=>barrel.barrelId).sort();
 const validModel=evidence.modelKind==='foil'
  ? !expectedBarrels.length&&!attachments.length&&evidence.prisms>=1
  : evidence.modelKind==='mixed'&&Number.isSafeInteger(evidence.volumeTetrahedra)&&
    (evidence.volumeTetrahedra>0||evidence.volumeTetrahedra===0&&
     (attachments.length>0||request.operatorPolicy==='skin-volume-v3'||request.partitionPolicy==='automatic-conductive-schur-v1')&&!expectedBarrels.length&&evidence.prisms>0)&&
    Number.isSafeInteger(evidence.nativeCopperTetrahedra)&&
    Number.isSafeInteger(evidence.fullPotentialCount)&&evidence.fullPotentialCount>0&&
    evidence.fullCurrentCount===3*evidence.nativeCopperTetrahedra&&
    (evidence.prisms===0?evidence.nativeCopperTetrahedra===evidence.volumeTetrahedra:
     evidence.nativeCopperTetrahedra-evidence.volumeTetrahedra>=evidence.prisms)&&
    evidence.magneticQuadratureCoverage==='unverified';
 const reportedAttachments=evidence.attachments??[];
 const validAttachments=Array.isArray(reportedAttachments)&&reportedAttachments.length===attachments.length&&
  attachments.every(({id,electrodes},i)=>reportedAttachments[i]?.id===id&&same(reportedAttachments[i].electrodes,electrodes));
 if(!same(expectedCopper,actual)||!validAttachments||!validModel||
    !Number.isSafeInteger(evidence.prisms)||evidence.prisms<0||
    !Array.isArray(evidence.barrelIds)||!same(expectedBarrels,[...evidence.barrelIds].sort())||
    !Number.isFinite(evidence.plannedBytes)||evidence.plannedBytes<=0||evidence.plannedBytes>producerBudget.maxMemoryMiB*1048576||
    !Number.isFinite(evidence.maxCopperVolumeChange)||evidence.maxCopperVolumeChange<0||evidence.maxCopperVolumeChange>1e-8||
    evidence.spatialCoverage!=='unverified'||evidence.bandCoverage!=='unverified'||
    electrodeNames.size!==Object.keys(evidence.electrodeAreasM2).length||
    [...electrodeNames].some(name=>!Number.isFinite(evidence.electrodeAreasM2[name])||evidence.electrodeAreasM2[name]<=0))
  fail('native copper and barrel ownership, pad areas or scope do not match the closed candidate');
}
