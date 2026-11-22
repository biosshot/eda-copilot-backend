import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';

const marker='P7_HIERARCHICAL_MAGNETIC ';
const gate=1e-8;
function vector(value,n){
 assert.ok(Array.isArray(value)&&value.length===n&&value.every(Number.isFinite));
 return value;
}

/** Independent checks of a saved tiny original-coordinate native control.
 * The existing direct pair quadrature is a numerical reference. Its own near,
 * self and physical discretization accuracy remain outside this certificate. */
export function verifyHierarchicalMagnetic(log){
 const lines=log.split(/\r?\n/).filter(line=>line.startsWith(marker));
 assert.equal(lines.length,1,'Expected one hierarchical magnetic control packet');
 const p=JSON.parse(lines[0].slice(marker.length));
 assert.equal(p.schema,'p7-hierarchical-magnetic-v1');
 assert.equal(p.policy,'certified-adaptive-harmonic-v1');
 assert.equal(p.representationVersion,3);
 assert.equal(p.harmonicBasis,'laplace-quotient-z01');
 assert.equal(p.momentCoefficientCount,169);
 assert.equal(p.m2lCouplings,5551);
 assert.equal(p.shiftStencilTerms,3619);
 assert.equal(p.minimumExpansionOrder,0);
 assert.equal(p.separationRatioMaximum,1);
 assert.equal(p.admissibility,'strict-disjoint-balls-certified-tail');
 assert.match(p.geometrySignature,/^fnv1a64:[0-9a-f]{16}$/);
 assert.equal(p.treeSignature,p.geometrySignature);
 assert.equal(p.axisSerial,true);
 assert.equal(p.leafTetrahedra,1);
 assert.equal(p.coverage,'unverified');
 assert.ok(Number.isSafeInteger(p.originalCellCount)&&p.originalCellCount>0&&p.originalCellCount<=32);
 const n=3*p.originalCellCount;
 const levels=Math.ceil(Math.log2(p.originalCellCount))+1;
 assert.equal(p.interactionStorage,'immutable-adaptive-dual-tree-v1');
 assert.equal(p.interactionLevels,levels);
 assert.equal(p.interactionStackSlots,4*levels);
 assert.equal(p.interactionReplaysPerAction,3);
 assert.equal(p.traversalStorageBytes,4*levels*16+6*levels*8+512);
 assert.ok(Number.isSafeInteger(p.treeNodes)&&p.treeNodes>0);
 assert.equal(p.expansionOrder,12);
 assert.ok(Array.isArray(p.cases)&&p.cases.length>=n+2);
 const cover=p.basisCover;
 const count=value=>Number.isSafeInteger(value)&&value>=0;
 assert.ok(cover);
 assert.match(cover.coverSignature,/^fnv1a64:[0-9a-f]{16}$/);
 assert.match(cover.envelopeSignature,/^fnv1a64:[0-9a-f]{16}$/);
 assert.ok(count(cover.modelRevision)&&cover.modelRevision>=1);
 assert.equal(cover.rhsColumns,p.cases.length);
 vector(cover.absoluteActionGoals,cover.rhsColumns);
 vector(cover.maximumAbsoluteErrorBounds,cover.rhsColumns);
 assert.ok(cover.absoluteActionGoals.every(value=>value>0));
 assert.ok(cover.maximumAbsoluteErrorBounds.every((value,i)=>value>=0&&value<=cover.absoluteActionGoals[i]));
 assert.ok(count(cover.visitedNodePairs)&&cover.visitedNodePairs>0);
 assert.ok(count(cover.farClusterPairs)&&cover.farClusterPairs>0);
 assert.ok([cover.exactPairs,cover.nearTiles,cover.zeroBlocks,cover.activeEnvelopeTerminals].every(count));
 assert.ok(cover.activeEnvelopeTerminals<=p.originalCellCount);
 for(const hist of [cover.chosenOrderHistogram,cover.maximumMomentDegreeHistogram,cover.maximumLocalDegreeHistogram])
  assert.ok(Array.isArray(hist)&&hist.length===13&&hist.every(count));
 assert.equal(cover.chosenOrderHistogram.reduce((sum,value)=>sum+value,0),cover.farClusterPairs);
 for(const value of [cover.m2lScalarContractionsPerAction,cover.shiftStencilApplicationsPerAction])
  if(value!==undefined)assert.ok(count(value));
 assert.ok(Number.isFinite(cover.actionWallSeconds)&&cover.actionWallSeconds>=0);
 const basis=new Set();
 let maxActionError=0,maxActionBound=0,maxFullKvlUpper=0,fullChecks=0;
 for(const [index,c] of p.cases.entries()){
  vector(c.current,n);
  vector(c.exactAction,n);
  vector(c.approximateAction,n);
  vector(c.absoluteErrorBound,n);
  vector(c.referenceErrorBound,n);
  assert.ok(Math.max(...c.absoluteErrorBound)<=cover.maximumAbsoluteErrorBounds[index],
   'Common batch certificate must include each original-current action bound');
  const active=c.current.flatMap((v,i)=>v===0?[]:[i]);
  if(active.length===1&&c.current[active[0]]===1)basis.add(active[0]);
  for(let i=0;i<n;i++){
   const bound=c.absoluteErrorBound[i],error=Math.abs(c.exactAction[i]-c.approximateAction[i]);
   const referenceBound=c.referenceErrorBound[i];
   assert.ok(bound>=0&&referenceBound>=0&&error<=bound+referenceBound,
    'Direct action and approximation intervals are disjoint');
   maxActionError=Math.max(maxActionError,error);
   maxActionBound=Math.max(maxActionBound,bound);
  }
  if(c.fullKvl!==undefined){
   const k=c.fullKvl;
   assert.equal(k.scope,'manufactured-original-current-rows');
   assert.ok(Number.isFinite(c.frequencyHz)&&c.frequencyHz>0);
   assert.ok(Array.isArray(k.exactResidual)&&k.exactResidual.length===n);
   const rows=k.exactResidual.length;
   vector(k.exactResidual,rows);
   vector(k.approximateResidual,rows);
   vector(k.uncertaintyBound,rows);
   vector(k.referenceUncertaintyBound,rows);
   assert.ok(Number.isFinite(k.scale)&&k.scale>0);
   assert.ok(Number.isFinite(k.upperRelative)&&k.upperRelative>=0&&k.upperRelative<=gate);
   let upper=0,exact=0;
   for(let i=0;i<rows;i++){
    const uncertainty=k.uncertaintyBound[i];
    const referenceUncertainty=k.referenceUncertaintyBound[i];
    const omega=2*Math.PI*c.frequencyHz;
    assert.ok(uncertainty>=omega*c.absoluteErrorBound[i]&&
     referenceUncertainty>=omega*c.referenceErrorBound[i],
     'Full KVL uncertainty must include the original magnetic action uncertainty');
    assert.ok(uncertainty>=0&&referenceUncertainty>=0&&
     Math.abs(k.exactResidual[i]-k.approximateResidual[i])<=uncertainty+referenceUncertainty,
     'Direct full KVL reference and production intervals are disjoint');
    upper=Math.max(upper,Math.abs(k.approximateResidual[i])+uncertainty);
    exact=Math.max(exact,Math.abs(k.exactResidual[i])+referenceUncertainty);
   }
   // Permit a few ulps only in the comparison of the reported division.
   assert.ok(upper/k.scale<=k.upperRelative*(1+8*Number.EPSILON));
   assert.ok(exact/k.scale<=gate,'Direct original-coordinate full KVL gate failed');
   maxFullKvlUpper=Math.max(maxFullKvlUpper,k.upperRelative);
   fullChecks++;
  }
 }
 assert.equal(basis.size,n,'Every original x/y/z current basis must be compared');
 const signed=p.cases.filter(c=>[0,1,2].every(axis=>
  c.current.some((v,i)=>i%3===axis&&v>0)&&c.current.some((v,i)=>i%3===axis&&v<0)));
 assert.ok(signed.some(c=>signed.some(d=>c!==d&&c.current.every((v,i)=>v===-d.current[i]))),
  'Expected opposite signed dense excitations in all three axes');
 assert.ok(fullChecks>0,'Original full KVL direct-reference certificate is required');
 return {policy:p.policy,originalCellCount:p.originalCellCount,expansionOrder:p.expansionOrder,
  exactPairs:cover.exactPairs,farClusterPairs:cover.farClusterPairs,basisChecks:basis.size,
  fullChecks,fullKvlScope:'manufactured original-current row guard control',
  maxActionError,maxActionBound,maxFullKvlUpper,coverage:p.coverage};
}

if(process.argv[1]&&import.meta.url===pathToFileURL(resolve(process.argv[1])).href){
 assert.ok(process.argv[2],'Usage: node scripts/verify-p7-hierarchical-magnetic.mjs <native-control.log>');
 console.log(JSON.stringify(verifyHierarchicalMagnetic(readFileSync(process.argv[2],'utf8')),null,2));
}
