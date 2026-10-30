import test from 'node:test';import assert from 'node:assert/strict';
import {sectionFieldIdentity,canonical} from '../src/analyze/local-model-cache.ts';
test('section field identity excludes length/output axis, retains geometry/material/band/mesh, and preserves inputs',()=>{
 const request={line:{kind:'microstrip',lengthMm:20,widthMm:.3,material:{relativePermittivity:4.2,bandHz:[1e6,1e9]}},frequencies:[1e8],mesh:{maxNodes:10000}};
 const original=structuredClone(request),key=(r:typeof request)=>canonical(sectionFieldIdentity(r));
 assert.equal(key(request),key({...request,line:{...request.line,lengthMm:40},frequencies:[2e8]}));
 assert.notEqual(key(request),key({...request,line:{...request.line,widthMm:.4}}));assert.notEqual(key(request),key({...request,mesh:{maxNodes:20000}}));
 assert.notEqual(key(request),key({...request,line:{...request.line,material:{relativePermittivity:4.3,bandHz:[1e6,1e9]}}}));assert.notEqual(key(request),key({...request,line:{...request.line,material:{relativePermittivity:4.2,bandHz:[1e7,1e9]}}}));assert.deepEqual(request,original);
 const polygon={line:{kind:'polygon-section',lengthMm:10,geometry:{loss:{frequenciesHz:[1e8],bandHz:[1e7,1e9],conductivitySM:[58e6]}}}};const second=structuredClone(polygon);second.line.geometry.loss.frequenciesHz=[2e8];assert.equal(canonical(sectionFieldIdentity(polygon)),canonical(sectionFieldIdentity(second)));second.line.geometry.loss.bandHz=[1e8,1e9];assert.notEqual(canonical(sectionFieldIdentity(polygon)),canonical(sectionFieldIdentity(second)));
});

test('native serialized absent mesh controls retain the same field identity',()=>{assert.equal(canonical(sectionFieldIdentity({line:{kind:'microstrip'},mesh:{maxNodes:10000}})),canonical(sectionFieldIdentity({line:{kind:'microstrip'},mesh:{maxNodes:10000,maxTriangles:null,maxEdgeMm:null}})));});
