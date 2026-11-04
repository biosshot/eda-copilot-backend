import test from 'node:test';
import assert from 'node:assert/strict';
import type {AnalysisCrossSectionReport,AnalysisCrossSectionRequest} from '../native/analyze/index.js';
import {executeSegmentedSection} from '../src/analyze/segmented-channel.ts';

const polygon={
 kind:'polygon-section',
 lengthMm:0.3,
 geometry:{maxNodes:50000,maxMemoryMib:1024,maxTriangleAreaM2:5e-9,domain:{outerM:[[0,0],[1,0],[1,1],[0,1]]}},
};
const request:AnalysisCrossSectionRequest={line:polygon,frequencies:[1000,10000],budget:{maxSeconds:75,maxMemoryMiB:3072}};
const complete:AnalysisCrossSectionReport={status:'complete',diagnostics:[],elapsedMs:1};
const meshRefusal:AnalysisCrossSectionReport={
 status:'rejected',elapsedMs:1,
 diagnostics:[{code:'SECTION_MESH_LIMIT',message:'SECTION_MESH_LIMIT: polygon refinement cap',severity:'error'}],
};

test('successful generated section keeps its original request and cache identity',()=>{
 const result=executeSegmentedSection(request,r=>{assert.equal(r,request);return complete;},()=>assert.fail('no retry budget'),true);
 assert.deepEqual(result,{result:complete});
});

test('generated polygon cap retries once with the same geometry, memory and a fresh deadline',()=>{
 const calls:AnalysisCrossSectionRequest[]=[];
 const original=structuredClone(request);
 const outcome=executeSegmentedSection(request,r=>{
  calls.push(r);
  return calls.length===1?meshRefusal:complete;
 },()=>68,true);
 assert.deepEqual(outcome,{result:complete,meshRetry:{
  reason:'SECTION_MESH_LIMIT: polygon refinement cap',maxNodesBefore:50000,maxNodesAfter:200000,effectiveMemoryMiB:1024,
 }});
 assert.equal(calls.length,2);
 assert.equal(calls[0],request);
 assert.deepEqual(calls[1],{
  ...request,line:{...polygon,geometry:{...polygon.geometry,maxNodes:200000}},budget:{...request.budget,maxSeconds:68},
 });
 assert.deepEqual(request,original);
});

test('generated section retry respects byte, explicit node and triangle bounds',()=>{
 const options:AnalysisCrossSectionRequest[]=[
  {...request,budget:{maxSeconds:75,maxMemoryMiB:160}},
  {...request,mesh:{maxNodes:57000,maxRefinements:3}},
  {...request,mesh:{maxNodes:59000,maxTriangles:110001}},
 ];
 const expected=[Math.floor((160*1024**2-65536)/3072),57000,55000];
 for(const [index,input] of options.entries()){
  const calls:AnalysisCrossSectionRequest[]=[];
  const outcome=executeSegmentedSection(input,r=>{calls.push(r);return meshRefusal;},()=>74,true);
  assert.equal(outcome.result,meshRefusal);
  assert.equal(outcome.meshRetry?.maxNodesAfter,expected[index]);
  assert.equal(calls.length,2);
  assert.equal(calls[1].mesh,input.mesh);
  assert.equal(calls[1].budget?.maxMemoryMiB,input.budget?.maxMemoryMiB);
 }
});

test('authored, non-polygon and unrelated section failures do not retry',()=>{
 const rejected=(code:string,message:string):AnalysisCrossSectionReport=>({...meshRefusal,diagnostics:[{code,message,severity:'error'}]});
 const cases:[AnalysisCrossSectionRequest,AnalysisCrossSectionReport,boolean][]=[
  [request,meshRefusal,false],
  [{...request,line:{kind:'microstrip'}},meshRefusal,true],
  [request,rejected('SECTION_MESH_LIMIT','SECTION_MESH_LIMIT: corner vertex budget'),true],
  [request,rejected('SECTION_CONVERGENCE','SECTION_CONVERGENCE: polygon matrix mesh convergence failed within shared caps'),true],
  [request,rejected('SECTION_TIMEOUT','SECTION_TIMEOUT: deadline'),true],
  [request,{...meshRefusal,diagnostics:[...meshRefusal.diagnostics,...rejected('SECTION_MATRIX','factor refused').diagnostics]},true],
  [{...request,mesh:{maxTriangles:100000}},meshRefusal,true],
  [{...request,budget:{maxSeconds:75,maxMemoryMiB:128}},meshRefusal,true],
 ];
 for(const [input,refusal,enabled] of cases){
  let count=0;
  const outcome=executeSegmentedSection(input,r=>{count++;assert.equal(r,input);return refusal;},()=>assert.fail('no retry budget'),enabled);
  assert.equal(count,1);
  assert.deepEqual(outcome,{result:refusal});
 }
});
