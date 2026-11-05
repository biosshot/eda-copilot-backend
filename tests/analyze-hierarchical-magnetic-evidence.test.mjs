import test from 'node:test';
import assert from 'node:assert/strict';
import {verifyHierarchicalMagnetic} from '../scripts/verify-p7-hierarchical-magnetic.mjs';

function packet(){
 const n=9;
 const make=current=>({current,exactAction:Array(n).fill(1),approximateAction:Array(n).fill(1),
  absoluteErrorBound:Array(n).fill(1e-15),referenceErrorBound:Array(n).fill(1e-16)});
 const cases=Array.from({length:n},(_,j)=>make(Array.from({length:n},(_,i)=>i===j?1:0)));
 const q=[1,-2,3,-4,5,-6,7,-8,9];
 cases.push(make(q),make(q.map(v=>-v)));
 Object.assign(cases.at(-1),{frequencyHz:1000,fullKvl:{
  scope:'manufactured-original-current-rows',exactResidual:Array(n).fill(0),
  approximateResidual:Array(n).fill(0),uncertaintyBound:Array(n).fill(1e-11),
  referenceUncertaintyBound:Array(n).fill(1e-12),scale:1,upperRelative:1e-11}});
 return {schema:'p7-hierarchical-magnetic-v1',policy:'certified-adaptive-harmonic-v1',coverage:'unverified',
  originalCellCount:3,expansionOrder:12,minimumExpansionOrder:0,separationRatioMaximum:1,
  admissibility:'strict-disjoint-balls-certified-tail',
  representationVersion:3,harmonicBasis:'laplace-quotient-z01',momentCoefficientCount:169,
  m2lCouplings:5551,shiftStencilTerms:3619,axisSerial:true,leafTetrahedra:1,
  geometrySignature:'fnv1a64:0123456789abcdef',treeSignature:'fnv1a64:0123456789abcdef',
  interactionStorage:'immutable-adaptive-dual-tree-v1',interactionLevels:3,interactionStackSlots:12,
  interactionReplaysPerAction:3,traversalStorageBytes:848,treeNodes:5,cases,
  basisCover:{modelRevision:1,rhsColumns:cases.length,coverSignature:'fnv1a64:1111111111111111',
   envelopeSignature:'fnv1a64:2222222222222222',absoluteActionGoals:Array(cases.length).fill(1e-12),
   maximumAbsoluteErrorBounds:Array(cases.length).fill(1e-15),chosenOrderHistogram:[1,...Array(12).fill(0)],
   visitedNodePairs:5,farClusterPairs:1,nearTiles:3,exactPairs:3,zeroBlocks:0,activeEnvelopeTerminals:3,
   maximumMomentDegreeHistogram:Array(13).fill(0),maximumLocalDegreeHistogram:Array(13).fill(0),
   m2lScalarContractionsPerAction:3,shiftStencilApplicationsPerAction:0,actionWallSeconds:0.01}};
}
const verify=p=>verifyHierarchicalMagnetic('P7_HIERARCHICAL_MAGNETIC '+JSON.stringify(p));

test('saved magnetic proof requires independent action intervals and original row uncertainty',()=>{
 assert.equal(verify(packet()).basisChecks,9);
 const corruptions=[
  p=>{delete p.representationVersion;},
  p=>{p.representationVersion=1;},
  p=>{p.momentCoefficientCount=286;},
  p=>{delete p.interactionStorage;},
  p=>{p.traversalStorageBytes=0;},
  p=>{p.basisCover.m2lScalarContractionsPerAction=-1;},
  p=>{p.basisCover.absoluteActionGoals[0]=0;},
  p=>{p.basisCover.maximumAbsoluteErrorBounds[0]=1;},
  p=>{p.cases[0].exactAction[0]+=1e-6;},
  p=>{p.cases.at(-1).fullKvl.exactResidual[0]=1e-6;},
  p=>{p.cases.at(-1).fullKvl.uncertaintyBound[0]=0;},
  p=>{p.cases.at(-1).fullKvl.referenceUncertaintyBound[0]=0;},
  p=>{p.cases.at(-1).fullKvl.upperRelative=1e-12;},
  p=>{p.cases.at(-1).fullKvl.upperRelative=2e-8;},
  p=>{p.cases.at(-1).current=[...p.cases.at(-2).current];},
  p=>{p.cases[0].current[0]=0;},
 ];
 for(const corrupt of corruptions){
  const value=packet();corrupt(value);
  assert.throws(()=>verify(value));
 }
});
