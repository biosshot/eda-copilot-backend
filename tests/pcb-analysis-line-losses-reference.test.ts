import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {createRequire} from 'node:module';
const references=JSON.parse(readFileSync('tests/fixtures/analysis-line-losses/references.json','utf8'));
const independent=JSON.parse(readFileSync('tests/fixtures/analysis-line-losses/boundary-panel-references.json','utf8'));
const addon=createRequire(import.meta.url)('../native/node-addon/index.cjs');

// Reviewer decision 2026-10-23: physical acceptance for the unchanged fixed
// cases uses the independent numerical reference and unchanged 5% target.
// Qucs remains a visible approximate engineering comparison, with its original
// 10% comparison goal and the historical failures retained in documentation.
for(const [index,r]of references.singleEngineering.entries())test(`P7b R1: fixed conductor w=${r.widthMm} mm matches independent numerical reference within 5%`,()=>{
 const session=new addon.AnalysisElectricalSession(),copper={conductivitySM:r.conductivitySM,state:'Explicit smooth reference at fixed temperature'};
 const result=session.solveCrossSection({line:{kind:r.kind,widthMm:r.widthMm,copperThicknessMm:r.copperThicknessMm,dielectricHeightMm:r.dielectricHeightMm,material:{relativePermittivity:r.relativePermittivity,lossTangent:0,state:'Explicit air reference; no dispersion'},conductors:{signal:copper,referencePlanes:{bottom:{thicknessMm:r.referenceThicknessMm,material:copper}}}},frequencies:[r.frequencyHz],mesh:{maxNodes:300000,maxTriangles:600000,maxEdgeMm:r.widthMm<1?.00375:.00625},budget:{maxMemoryMiB:1024}});
 try{
  assert.equal(result.status,'complete',JSON.stringify(result.diagnostics));
  const actual=result.parameters.frequencyDependent.samples[0].modes[0].conductorAlphaFirstOrderNpPerM,reference=independent.fixedCases[index];
  for(const expected of [reference.finest.alphaNpPerM,reference.cornerRichardsonEstimate.alphaNpPerM])assert.ok(Math.abs(actual/expected-1)<=.05,`${actual} vs independent ${expected}`);
  for(const [surface,key]of [[0,'signalGramPerM'],[1,'groundGramPerM']]as const)assert.ok(Math.abs(result.lossIntegrals.surfaces[surface].gramPerM[0][0]/reference.finest[key]-1)<=.05);
  console.log(JSON.stringify({widthMm:r.widthMm,actual,independentFinest:reference.finest.alphaNpPerM,independentEstimatedLimit:reference.cornerRichardsonEstimate.alphaNpPerM,independentEstimatedUncertainty:reference.estimatedRelativeUncertainty,engineeringQucs:r.conductorAlphaNpPerM,engineeringRelativeError:actual/r.conductorAlphaNpPerM-1,originalEngineeringGoal:r.tolerance,reviewDecision:'R1 numerical 5% acceptance approved; engineering discrepancy retained'}));
 }finally{session.clear();}
});
