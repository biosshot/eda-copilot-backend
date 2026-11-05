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
 return {schema:'p7-hierarchical-magnetic-v1',policy:'certified-cartesian-tree-v1',coverage:'unverified',
  originalCellCount:3,expansionOrder:4,exactPairs:3,farClusterPairs:1,cases};
}
const verify=p=>verifyHierarchicalMagnetic('P7_HIERARCHICAL_MAGNETIC '+JSON.stringify(p));

test('saved magnetic proof requires independent action intervals and original row uncertainty',()=>{
 assert.equal(verify(packet()).basisChecks,9);
 const corruptions=[
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
