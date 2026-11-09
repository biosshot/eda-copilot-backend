import type { AnalysisBoardFoilCheckedReport, AnalysisBoardFoilCheckedRequest, AnalysisMagneticCertificate } from '../../native/analyze/index.js';
import type { BoardFoilProblem } from './types.ts';
import type { CoupledRlcData } from './analysis-dsl/spec-doc.js';
import {validateBoardFoilLossGrams} from './board-foil-loss-grams.ts';
import {validateReturnBoundary} from './return-boundary.ts';

function fail(message:string):never {throw new Error('CHANNEL_FOIL_EVIDENCE: '+message);}
const same=(a:unknown,b:unknown)=>JSON.stringify(a)===JSON.stringify(b);
const positive=(value:unknown):value is number=>typeof value==='number'&&Number.isFinite(value)&&value>0;

function validateMagneticCover(cover:Omit<AnalysisMagneticCertificate,'policy'>|undefined,rhs:number,originals:number,minimumColumns=1):void {
 const count=(value:unknown):value is number=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=0;
 if(!cover||!/^fnv1a64:[0-9a-f]{16}$/.test(cover.coverSignature)||
    !/^fnv1a64:[0-9a-f]{16}$/.test(cover.envelopeSignature)||!count(rhs)||rhs<minimumColumns||
    !Array.isArray(cover.absoluteActionGoals)||cover.absoluteActionGoals.length!==rhs||
    !cover.absoluteActionGoals.every(positive)||
    !Array.isArray(cover.maximumAbsoluteErrorBounds)||cover.maximumAbsoluteErrorBounds.length!==rhs||
    cover.maximumAbsoluteErrorBounds.some((value,i)=>!Number.isFinite(value)||value<0||value>cover.absoluteActionGoals[i])||
    !Array.isArray(cover.chosenOrderHistogram)||cover.chosenOrderHistogram.length!==13||
    !cover.chosenOrderHistogram.every(count)||
    ![cover.farClusterPairs,cover.exactPairs,cover.zeroBlocks,cover.activeEnvelopeTerminals].every(count)||
    cover.chosenOrderHistogram.reduce((sum,n)=>sum+n,0)!==cover.farClusterPairs||
    cover.activeEnvelopeTerminals>originals||(rhs===0&&cover.activeEnvelopeTerminals!==0)||
    !Number.isFinite(cover.actionWallSeconds)||cover.actionWallSeconds<0)
  fail('missing or inconsistent adaptive common-cover action certificate');
 const batch=cover as import('../../native/analyze/index.js').AnalysisMagneticBasisCover;
 if(cover.nearQuadraturePolicy!==undefined||cover.nearQuadratureErrorCertified!==undefined) {
  if(cover.nearQuadraturePolicy!=='axial-tetra-estimated-v1'||cover.nearQuadratureErrorCertified!==false)
   fail('invalid estimated near-quadrature provenance');
 }
 if(cover.nearBoundEnabled!==undefined||cover.boundedOmittedBlocks!==undefined||cover.nearBoundPolicy!==undefined) {
  if(typeof cover.nearBoundEnabled!=='boolean'||!count(cover.boundedOmittedBlocks)||
     cover.nearBoundPolicy!=='summed-per-tetra-volume-v1'||(!cover.nearBoundEnabled&&cover.boundedOmittedBlocks!==0))
   fail('invalid bounded near-omission provenance');
 }
 if(batch.chunkCertificates!==undefined || batch.chunkCount!==undefined || batch.batchColumns!==undefined || batch.coverAggregation!==undefined) {
  if(!Array.isArray(batch.chunkCertificates)||!count(batch.batchColumns)||batch.batchColumns<1||batch.batchColumns>64||batch.coverAggregation!=='sum-of-sequential-chunk-covers'||batch.chunkCount!==batch.chunkCertificates.length||batch.chunkCount<2) fail('invalid magnetic chunk provenance');
  let offset=0;
  for(const chunk of batch.chunkCertificates) {
   if(!count(chunk.rhsColumns)||chunk.rhsColumns<1||chunk.rhsColumns>batch.batchColumns||chunk.chunkCertificates!==undefined) fail('invalid magnetic chunk size');
   validateMagneticCover(chunk,chunk.rhsColumns,originals);
   if(!same(chunk.absoluteActionGoals,cover.absoluteActionGoals.slice(offset,offset+chunk.rhsColumns))||!same(chunk.maximumAbsoluteErrorBounds,cover.maximumAbsoluteErrorBounds.slice(offset,offset+chunk.rhsColumns))) fail('inconsistent magnetic chunk values');
   offset+=chunk.rhsColumns;
  }
  if(offset!==rhs||(['farClusterPairs','exactPairs','zeroBlocks'] as const).some(key=>batch.chunkCertificates!.reduce((sum,c)=>sum+c[key],0)!==cover[key])) fail('inconsistent magnetic chunk totals');
  if(batch.chunkCertificates.some(c=>c.nearQuadraturePolicy!==cover.nearQuadraturePolicy||c.nearQuadratureErrorCertified!==cover.nearQuadratureErrorCertified))
   fail('inconsistent estimated near-quadrature chunk provenance');
  if(cover.nearBoundEnabled!==undefined&&(batch.chunkCertificates.some(c=>c.nearBoundEnabled!==cover.nearBoundEnabled||c.nearBoundPolicy!==cover.nearBoundPolicy)||
     batch.chunkCertificates.reduce((sum,c)=>sum+(c.boundedOmittedBlocks??0),0)!==cover.boundedOmittedBlocks)) fail('inconsistent near-omission chunk totals');
 }
}

function validateMagneticOperator(report:AnalysisBoardFoilCheckedReport):void {
 const p=report.provenance,operator=p.magneticOperator;
 const columns=report.samples.flatMap(sample=>sample.columns);
 if(p.compactOperator!==undefined){
  const c=p.compactOperator;
  // Approximation tier: pair changes and KVL widths are recorded estimates, not certificates.
  const approximate=(report.quality as {residualPolicy?:{limit?:unknown}}).residualPolicy?.limit===1e-3;
  const adaptive=c.basis==='adaptive-mixed-closure';
  const validOrder=Number.isSafeInteger(c.planarMaxOrder)&&((p.currentModes===0||c.foilSupports===0)?c.planarMaxOrder===0:
   adaptive&&c.planarMaxOrder===0&&c.planarMaxRelativeChange===0||c.planarMaxOrder>=8&&c.planarMaxOrder<=64&&c.planarMaxOrder%4===0);
  if(p.modelKind!=='mixed'||c.version!=='compact-planar-v1'||!['full-mixed','dc-harmonic-only','adaptive-mixed-closure'].includes(c.basis)||c.approximationError!=='unverified'||c.scope!=='compact-discrete'||c.originalVolumeActions!==0||c.physicalAccuracy!=='unverified'||report.quality.scope!=='compact-discrete'||report.quality.physicalError!=='unverified'||
     !((c.electricOperator==='homogeneous-charge-gram'&&report.operator==='compact-planar-homogeneous-prototype')||(c.electricOperator==='layered-spatial-field-mesh'&&report.operator==='compact-planar-layered-prototype'))||
     c.foilSupports!==p.prisms||c.localTetraSupports!==p.volumeTetrahedra||
     !Number.isSafeInteger(c.potentialCoordinates)||c.potentialCoordinates<1||c.potentialCoordinates>p.fullPotentialCount||
     !Number.isSafeInteger(c.currentCoordinates)||c.currentCoordinates<1||c.currentCoordinates>p.fullCurrentCount||
     !Number.isSafeInteger(p.currentModes)||p.currentModes<0||!validOrder||
     [c.planarMaxRelativeChange,c.mixedMaxRelativeChange,c.tetraSelfMaxRelativeChange,c.tetraPairAdaptiveTolerance].some(x=>typeof x!=='number'||!Number.isFinite(x)||x<0||x>(approximate?1:1e-8))||
     operator!==undefined||!approximate&&(report.quality.maxFullKvlUncertaintyRelative!==undefined||columns.some(x=>x.kvlUncertaintyRelative!==undefined||x.magneticCertificate!==undefined)))
   fail('invalid compact magnetic operator evidence');
  if(adaptive){
   if(c.electricOperator!=='layered-spatial-field-mesh'||!approximate&&(c.nearQuadratureEstimatedTolerance!==1e-8||
      !Number.isFinite(report.quality.maxCompactKvlUncertaintyRelative)||
      report.quality.maxCompactKvlUncertaintyRelative!==Math.max(...columns.map(x=>x.compactKvlUncertaintyRelative!))||
      columns.some(x=>typeof x.compactKvlUncertaintyRelative!=='number'||!Number.isFinite(x.compactKvlUncertaintyRelative)||x.compactKvlUncertaintyRelative<0||x.compactKvlUncertaintyRelative>x.kvlRelative||
       x.compactMagneticCertificate?.policy!=='compact-mixed-near-far'||x.compactMagneticCertificate.nearQuadratureErrorCertified!==false)))
    fail('invalid adaptive compact residual or near-quadrature evidence');
  }else if(report.quality.maxCompactKvlUncertaintyRelative!==undefined||columns.some(x=>x.compactKvlUncertaintyRelative!==undefined||x.compactMagneticCertificate!==undefined)){
   fail('unexpected adaptive compact evidence on a fixed-basis report');
  }
  return;
 }
 if(p.modelKind==='foil'){
  if(operator!==undefined||report.quality.maxFullKvlUncertaintyRelative!==undefined||
     columns.some(column=>column.kvlUncertaintyRelative!==undefined||column.magneticCertificate!==undefined))
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
  if(operator.method!=='unchanged direct tetrahedral pairs'||uncertainty!==0||
     columns.some(column=>column.magneticCertificate!==undefined))
   fail('exact magnetic operator unexpectedly reports truncation uncertainty');
  return;
 }
 const count=(value:unknown):value is number=>typeof value==='number'&&Number.isSafeInteger(value)&&value>=0;
 const levels=Math.ceil(Math.log2(p.nativeCopperTetrahedra))+1;
 const stackSlots=4*levels;
 const traversalBytes=16*stackSlots+6*levels*8+512;
 if(operator.policy!=='certified-adaptive-harmonic-v1'||operator.method!=='symmetric adaptive Newton expansion'||
    operator.expansionOrder!==12||operator.minimumExpansionOrder!==0||operator.separationRatioMaximum!==1||
    operator.admissibility!=='strict-disjoint-balls-certified-tail'||
    operator.representationVersion!==3||operator.harmonicBasis!=='laplace-quotient-z01'||
    operator.momentCoefficientCount!==169||operator.m2lCouplings!==5551||operator.shiftStencilTerms!==3619||
    operator.axisSerial!==true||operator.leafTetrahedra!==1||
    operator.interactionStorage!=='immutable-adaptive-dual-tree-v1'||
    operator.interactionLevels!==levels||operator.interactionStackSlots!==stackSlots||
    operator.interactionReplaysPerAction!==3||
    operator.traversalStorageBytes!==traversalBytes||traversalBytes>operator.plannedBytes||
    !count(operator.treeNodes)||operator.treeNodes<1||
    !count(operator.plannedBytes)||operator.plannedBytes<1||operator.plannedBytes>p.plannedBytes||
    !/^fnv1a64:[0-9a-f]{16}$/.test(operator.geometrySignature)||operator.treeSignature!==operator.geometrySignature)
  fail('unsupported or incomplete certified magnetic tree identity');
 const cover=operator.basisCover;
 validateMagneticCover(cover,cover?.rhsColumns,p.nativeCopperTetrahedra,0);
 if(!count(cover.modelRevision)||cover.modelRevision<1||!count(cover.visitedNodePairs)||cover.visitedNodePairs<1||
    !count(cover.nearTiles)||
    [cover.maximumMomentDegreeHistogram,cover.maximumLocalDegreeHistogram].some(hist=>
     !Array.isArray(hist)||hist.length!==13||!hist.every(count))||
    [cover.m2lScalarContractionsPerAction,cover.shiftStencilApplicationsPerAction].some(value=>value!==undefined&&!count(value)))
  fail('inconsistent immutable magnetic interaction cost evidence');
 for(const column of columns){
  if(column.magneticCertificate?.policy!==operator.policy)
   fail('fresh original-current adaptive magnetic certificate missing');
  validateMagneticCover(column.magneticCertificate,2,p.nativeCopperTetrahedra);
  if(column.magneticCertificate!.nearQuadraturePolicy!==cover.nearQuadraturePolicy||column.magneticCertificate!.nearQuadratureErrorCertified!==cover.nearQuadratureErrorCertified)
   fail('inconsistent basis and fresh-current near-quadrature provenance');
 }
}

function validatePartition(report:AnalysisBoardFoilCheckedReport,problem:BoardFoilProblem):void {
 const p=report.provenance,partition=p.automaticPartition,correction=report.conductiveCorrection;
 const policy=problem.request.partitionPolicy;
 if(policy===undefined){
  if(p.partitionPolicy!==undefined||p.hybridAssembly!==undefined||partition!==undefined||correction!==undefined)
   fail('unexpected conductive partition evidence');
  return;
 }
 if(policy==='automatic-hybrid-v1'){
  const h=p.hybridAssembly,c=p.compactOperator;
  if(p.partitionPolicy!==policy||partition!==undefined||correction!==undefined||!h||
     h.policy!==policy||h.method!=='global-compact-mixed'||h.physicalAccuracy!=='unverified'||
     !c||c.basis!=='adaptive-mixed-closure'||c.electricOperator!=='layered-spatial-field-mesh'||
     report.operator!=='compact-planar-layered-prototype'||h.potentialCoordinates!==c.potentialCoordinates||h.currentCoordinates!==c.currentCoordinates)
   fail('missing or inconsistent automatic hybrid assembly evidence');
  return;
 }
 if(p.hybridAssembly!==undefined)fail('unexpected hybrid assembly evidence');
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

function validateRegimeAudit(report:AnalysisBoardFoilCheckedReport):void {
 const audit=report.modelRegimeAudit,n=report.referenceOhm.length;
 if(audit===undefined){if(process.env.P7_HYBRID_REGIME_AUDIT==='1'&&report.provenance.compactOperator?.basis==='adaptive-mixed-closure')fail('requested model regime control is missing');return;}
 if(!audit)fail('malformed model regime control');
 const finiteVector=(v:unknown):v is number[]=>Array.isArray(v)&&v.length===n&&v.every(x=>typeof x==='number'&&Number.isFinite(x));
 if(audit.status!=='pass'||audit.scope!=='discrete-model-only'||audit.physicalCoverage!=='unverified'||
    !positive(audit.probeHz)||audit.probeHz>report.frequenciesHz.at(-1)!/20||
    !Array.isArray(audit.dcPortVoltagesV)||audit.dcPortVoltagesV.length!==n||!audit.dcPortVoltagesV.every(finiteVector)||
    !Array.isArray(audit.acPortVoltagesV)||audit.acPortVoltagesV.length!==n||audit.acPortVoltagesV.some(v=>!v||!finiteVector(v.real)||!finiteVector(v.imaginary))||
    !Array.isArray(audit.timeDiagnostics)||audit.timeDiagnostics.length!==n)
  fail('invalid model-only DC/AC/time control');
 for(const row of audit.timeDiagnostics){
  if(!row||!Array.isArray(row.relativePeakErrors)||row.relativePeakErrors.length!==2||row.relativePeakErrors.some(x=>!Number.isFinite(x)||x<0)||
     row.relativePeakErrors[1]>.02||row.relativePeakErrors[1]>row.relativePeakErrors[0]*1.25+1e-5||
     !Array.isArray(row.timeQuality)||row.timeQuality.length!==2)fail('model temporal overlap or refinement failed');
  for(const [i,q] of row.timeQuality.entries()){
   if(!q||!Number.isFinite(q.relativeResidual)||q.relativeResidual<0||q.relativeResidual>1e-8||
      !Number.isFinite(q.maximumLocalErrorRatio)||q.maximumLocalErrorRatio<0||q.maximumLocalErrorRatio>1||
      !Number.isFinite(q.energyBalanceJ)||!Number.isFinite(q.energyNormalizationJ)||q.energyNormalizationJ<0||
      Math.abs(q.energyBalanceJ)>1e-14+1e-7*q.energyNormalizationJ||
      Math.abs(q.energyBalanceJ)>(q.energyNormalizationJ===0?1e-20:1e-4*q.energyNormalizationJ)||
      !positive(q.maximumAcceptedStepS)||q.maximumAcceptedStepS>(1/audit.probeHz)/(i===0?128:256)*(1+1e-8))
    fail('model temporal numerical or energy evidence is invalid');
  }
 }
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
 if(!['skin-volume-v3','hybrid-planar-v3'].includes(request.operatorPolicy)||
    (request.operatorPolicy==='hybrid-planar-v3'&&request.partitionPolicy!=='automatic-hybrid-v1')||
    !positive(effectiveUpperHz)||effectiveUpperHz>(request.operatorPolicy==='hybrid-planar-v3'?1e10:1e5)||
    provenance.operatorPolicy!==request.operatorPolicy||
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
 const validSpacing=(skinDepthM:number,stepM:number,pieces:number,conductivity:unknown,divisor=3)=>{
  if(!positive(skinDepthM)||!positive(stepM)||!Number.isSafeInteger(pieces)||pieces<1||!positive(conductivity))return false;
  const depth=1/(Math.sqrt(Math.PI*(4e-7*Math.PI)*mesh.effectiveUpperHz)*Math.sqrt(conductivity));
  return positive(depth)&&Math.abs(skinDepthM/depth-1)<=1e-8&&stepM<=skinDepthM/divisor*(1+1e-8);
 };
 if(!layers.length||!Array.isArray(mesh.layers)||mesh.layers.length!==layers.length||
    mesh.layers.some((row,i)=>!row||row.layer!==layers[i].name||
     !validSpacing(row.skinDepthM,row.maximumStepM,row.slabs,layers[i].conductivitySM,request.operatorPolicy==='hybrid-planar-v3'?5:3))||
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
 // Native declares the approximation tier only for its non-certified large-board operator.
 const maxResidual=(report as {quality?:{residualPolicy?:{limit?:unknown}}}).quality?.residualPolicy?.limit===1e-3?1e-3:1e-8;
 const columns=report.samples.flatMap(sample=>sample.columns);
 if(columns.some(({kclRelative,kvlRelative,copperLossW,powerBalanceErrorW})=>
  [kclRelative,kvlRelative].some(x=>!Number.isFinite(x)||x<0||x>maxResidual)||
  [copperLossW,powerBalanceErrorW].some(x=>!Number.isFinite(x)||x<0)))
  fail('native foil residual or power evidence is outside its accepted limits');
 for(const [i,sample] of report.samples.entries())for(const column of sample.columns){
  validateReturnBoundary(column.returnBoundary, (problem.geometry as {returnBoundaryMm?:unknown})?.returnBoundaryMm);
  if(column.returnBoundary){
   const reflection=report.parameters[i][column.input][column.input];
   const portCurrent=0.5*Math.hypot(1-reflection.real,reflection.imaginary);
   if(Math.abs(portCurrent-column.returnBoundary.portCurrentA)>64*Number.EPSILON*Math.max(1,portCurrent))
    fail('return boundary normalization disagrees with physical port currents');
  }
  const spatial=column.spatialRecovery;
  if(spatial!==undefined){
   if(!spatial||typeof spatial!=='object'||spatial.status!=="recovered-discrete"||spatial.physicalAccuracy!=="unverified"||spatial.driveConvention!=="1 A RMS matched Norton"||!Array.isArray(spatial.supports)||spatial.supports.length===0||!Number.isFinite(spatial.copperLossW)||spatial.copperLossW<0)
    fail('invalid discrete spatial recovery');
   const vector=(v:unknown,n:number):v is number[]=>Array.isArray(v)&&v.length===n&&v.every(x=>typeof x==='number'&&Number.isFinite(x));
   let sum=0;
   for(const support of spatial.supports){
    if(!support||typeof support!=='object') fail('invalid spatial support');
    const geometry=support.kind==='foil' ? Array.isArray(support.xyM)&&support.xyM.length===3&&support.xyM.every(v=>vector(v,2))&&vector(support.zM,2)&&support.zM[1]>support.zM[0]&&typeof support.layer==='string'&&support.layer.length>0&&Number.isInteger(support.triangle)&&support.triangle>=0
      : support.kind==='barrel' ? Array.isArray(support.xyM)&&support.xyM.length===3&&support.xyM.every(v=>vector(v,2))&&vector(support.zM,2)&&support.zM[1]>support.zM[0]&&support.barrel!==undefined&&support.barrel!==null&&Number.isInteger(support.triangle)&&support.triangle>=0
      : support.kind==='tetrahedron' ? Array.isArray(support.verticesM)&&support.verticesM.length===4&&support.verticesM.every(v=>vector(v,3)) : false;
    if(!geometry||!Number.isFinite(support.volumeM3)||support.volumeM3<=0||!Number.isFinite(support.lossW)||support.lossW<0||!vector(support.currentDensityRealAPerM2,3)||!vector(support.currentDensityImaginaryAPerM2,3)) fail('invalid spatial support');
    sum+=support.lossW;
   }
   if(!Number.isFinite(sum)||Math.abs(sum-column.copperLossW)>1e-18+1e-8*(sum+column.copperLossW)||Math.abs(sum-spatial.copperLossW)>1e-18+1e-8*(sum+spatial.copperLossW)) fail('spatial losses disagree with copper loss');
  }
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
 const externalResources=request.partitionPolicy==='automatic-hybrid-v1';
 const seconds=evidence?.maxSeconds??NaN;
 const memory=evidence?.maxMemoryMiB??NaN;
 if(!evidence||!Array.isArray(evidence.sourceMapping)||!evidence.electrodeAreasM2||
    (!externalResources&&(!Number.isFinite(seconds)||seconds<=0||seconds>producerBudget.maxSeconds||
     !Number.isFinite(memory)||memory<1||memory>producerBudget.maxMemoryMiB||
     evidence.plannedBytes>memory*1048576)))
  fail('native producer limits or ownership evidence missing or inconsistent');
 validateSkinMesh(evidence,problem);
 validatePartition(report,problem);
 validateMagneticOperator(report);
 validateBoardFoilLossGrams(report,attachments);
 validateRegimeAudit(report);
 const expectedCopper=expected.sourceCopper.map(r=>JSON.stringify([r.regionId,r.sourcePath,r.layer])).sort();
 const actual=evidence.sourceMapping.map(([id,path,,layer])=>JSON.stringify([id,path,layer])).sort();
 const expectedBarrels=expected.sourceBarrels.map(barrel=>barrel.barrelId).sort();
 const validModel=evidence.modelKind==='foil'
  ? !expectedBarrels.length&&!attachments.length&&evidence.prisms>=1
  : evidence.modelKind==='mixed'&&Number.isSafeInteger(evidence.volumeTetrahedra)&&
    (evidence.volumeTetrahedra>0||evidence.volumeTetrahedra===0&&
     (attachments.length>0||request.operatorPolicy==='skin-volume-v3'||request.partitionPolicy==='automatic-conductive-schur-v1'||request.partitionPolicy==='automatic-hybrid-v1')&&!expectedBarrels.length&&evidence.prisms>0)&&
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
    !Number.isFinite(evidence.plannedBytes)||evidence.plannedBytes<=0||(!externalResources&&evidence.plannedBytes>producerBudget.maxMemoryMiB*1048576)||
    !Number.isFinite(evidence.maxCopperVolumeChange)||evidence.maxCopperVolumeChange<0||evidence.maxCopperVolumeChange>1e-8||
    evidence.spatialCoverage!=='unverified'||evidence.bandCoverage!=='unverified'||
    electrodeNames.size!==Object.keys(evidence.electrodeAreasM2).length||
    [...electrodeNames].some(name=>!Number.isFinite(evidence.electrodeAreasM2[name])||evidence.electrodeAreasM2[name]<=0))
  fail('native copper and barrel ownership, pad areas or scope do not match the closed candidate');
}
