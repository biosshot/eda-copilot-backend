import test from 'node:test';import assert from 'node:assert/strict';
import {compareThermalPhysicalFields} from './helpers/thermal-field-comparison.ts';
import {thermalBoundaryLocalizer} from './helpers/thermal-boundary-localization.ts';
import {readFileSync} from 'node:fs';
function mesh(fine:boolean):any{const points=fine?[[0,0],[1,0],[1,1],[0,1],[.5,.5]]:[[0,0],[1,0],[1,1],[0,1]],triangles=fine?[[0,1,4],[1,2,4],[2,3,4],[3,0,4]]:[[0,1,2],[0,2,3]],layers=['TOP','core','BOTTOM'];return{temperaturesC:Object.fromEntries(layers.flatMap(layer=>points.map((p,i)=>[`pcb:${layer}:${i}`,[25+10*p[0]+5*p[1]]]))),spatial:{layers:layers.map(name=>({name})),geometry:{booleanErrorBoundMm:1e-8,physicalDomain:[{outer:[[0,0],[1,0],[1,1],[0,1]],holes:[[[.4,.4],[.6,.4],[.6,.6],[.4,.6]]]}]},mesh:{maxEdgeMm:fine?.5:1,pointsMm:points,triangles,fieldNodes:layers.flatMap(layer=>points.map((pointMm,i)=>({id:`pcb:${layer}:${i}`,layer,pointMm})))}}};}
test('physical field sampling preserves affine P1 across both meshes, holes and physical boundary traces',()=>{const result=compareThermalPhysicalFields(mesh(false),mesh(true),25);assert.ok(result.relativeMaximumChange<1e-12);assert.ok(result.counts.classifiedVoidNodes>0);assert.ok(result.counts.physicalTraces>0);assert.equal(result.counts.missingPhysicalSamples,0);assert.equal(Object.keys(result.countsByLayer).length,3);assert.match(result.scope,/not a certified/);});
test('void vertices cannot hide an erroneous field on their surrounding physical support',()=>{const fine=mesh(true);fine.temperaturesC['pcb:TOP:4'][0]+=5;const result=compareThermalPhysicalFields(mesh(false),fine,25);assert.ok(result.maximum.differenceC>3);assert.ok(result.relativeMaximumChange>.02);});
test('missing physical interpolation remains explicit and different physical domains reject',()=>{const fine=mesh(true);delete fine.temperaturesC['pcb:TOP:0'];assert.throws(()=>compareThermalPhysicalFields(mesh(false),fine,25));const changed=mesh(true);changed.spatial.geometry.physicalDomain[0].holes=[];assert.throws(()=>compareThermalPhysicalFields(mesh(false),changed,25));const missing=mesh(true);delete missing.spatial.geometry.physicalDomain;assert.throws(()=>compareThermalPhysicalFields(mesh(false),missing,25));});

test('physical comparison rise uses explicit ambient and is invariant under absolute-temperature shift',()=>{const coarse=mesh(false),fine=mesh(true);fine.temperaturesC['pcb:TOP:4'][0]+=5;const first=compareThermalPhysicalFields(coarse,fine,25);for(const s of [coarse,fine])for(const values of Object.values(s.temperaturesC)as number[][])values[0]+=100;const shifted=compareThermalPhysicalFields(coarse,fine,125);assert.ok(Math.abs(first.relativeMaximumChange-shifted.relativeMaximumChange)<1e-12);assert.equal(shifted.referenceAmbientC,125);assert.throws(()=>compareThermalPhysicalFields(coarse,fine,NaN));});

test('independent requested physical points are sampled before nodal deduplication and void requests reject',()=>{const recorded:any[]=[];const result=compareThermalPhysicalFields(mesh(false),mesh(true),25,s=>{if(s.kind==='requestedPhysicalSamples')recorded.push(s);},[[.25,.25],[.25,.25],[0,0]]);assert.equal(result.counts.requestedPhysicalSamples,2);assert.equal(recorded.length,2);assert.ok(recorded.every(s=>s.coarseValues.every((v:number,i:number)=>Math.abs(v-s.fineValues[i])<1e-12)));assert.throws(()=>compareThermalPhysicalFields(mesh(false),mesh(true),25,undefined,[[.5,.5]]));});

function transported(fine:boolean,delta=1e-8){const s=mesh(fine);s.spatial.boardIdentity='transport-contract';s.spatial.geometry.outline={outer:[[0,0],[1,0],[1,1],[0,1]],holes:[]};s.spatial.geometry.physicalDomain[0].outer=s.spatial.geometry.outline.outer.map(([x,y]:number[])=>[x-delta,y]);return s;}
test('verified Boolean-transported outline uses the same source edge fraction with recorded bounded displacement',()=>{
 const a=transported(false),b=transported(true),result=compareThermalPhysicalFields(a,b,25);
 assert.equal(result.counts.missingBoundarySamples,0);assert.ok(result.boundaryLocalization.count>0);
 assert.ok(result.boundaryLocalization.records.every(r=>r.displacementMm<=r.limitMm&&r.limitMm<=r.representationLimitMm+1e-13));
 for(const r of result.boundaryLocalization.records){const [a,b]=r.sourceEdge;assert.ok(Math.hypot(r.localizedPointMm[0]-a[0]-r.fraction*(b[0]-a[0]),r.localizedPointMm[1]-a[1]-r.fraction*(b[1]-a[1]))<1e-14);}
 const localize=thermalBoundaryLocalizer(a);assert.equal(localize([.4,.5]),undefined,'A cutout boundary is never localized onto the outer outline');assert.equal(localize([.2,-1e-8]),undefined,'Nearby point off the transported boundary is not admissible');
});
test('boundary localization cannot replace a physical gap, another boundary, excessive transport error or interior interpolation',()=>{
 const gap=transported(true);gap.spatial.mesh.triangles=[[0,1,4],[1,2,4]];assert.ok(compareThermalPhysicalFields(transported(false),gap,25).counts.missingBoundarySamples>0);
 const wrong=transported(true);wrong.spatial.geometry.outline.outer[0]=[.001,0];assert.ok(compareThermalPhysicalFields(transported(false),wrong,25).counts.missingBoundarySamples>0);
 const identity=transported(true);identity.spatial.boardIdentity='another-board';assert.ok(compareThermalPhysicalFields(transported(false),identity,25).counts.missingBoundarySamples>0);
 assert.ok(compareThermalPhysicalFields(transported(false,1e-5),transported(true,1e-5),25).counts.missingBoundarySamples>0);
 const interior=compareThermalPhysicalFields(transported(false),transported(true),25,undefined,[[-5e-9,.5]]);assert.ok(interior.counts.missingPhysicalSamples>0,'Interior P1 lookup must not acquire boundary localization');
});

test('TS and native classify the same frozen real transported boundary triangle patches',()=>{
 const f=JSON.parse(readFileSync(new URL('./fixtures/thermal-boundary-correspondence.json',import.meta.url),'utf8'));
 const layers=['TOP','core','BOTTOM'];const make=(source:boolean)=>({temperaturesC:Object.fromEntries(layers.flatMap(layer=>f.points.map((_:unknown,i:number)=>[`pcb:${layer}:${i}`,[26]]))),spatial:{boardIdentity:f.identity,layers:layers.map(name=>({name})),geometry:{physicalDomain:[{outer:f.domainOuter,holes:[]}],booleanErrorBoundMm:f.booleanErrorBoundMm,...(source?{outline:{outer:f.sourceOuter,holes:[]}}:{})},mesh:{maxEdgeMm:1,pointsMm:f.points,triangles:f.triangles,fieldNodes:layers.flatMap(layer=>f.points.map((pointMm:number[],i:number)=>({id:`pcb:${layer}:${i}`,layer,pointMm})))}}});
 const strict=new Set<string>(),localized=new Set<string>();const a=make(false),b=make(true);
 compareThermalPhysicalFields(a,a,25,s=>{if(s.kind==='requestedPhysicalSamples')strict.add(JSON.stringify(s.pointMm));},f.queries);
 compareThermalPhysicalFields(b,b,25,s=>{if(s.kind==='requestedPhysicalSamples')localized.add(JSON.stringify(s.pointMm));},f.queries);
 const classification=f.queries.map((pointMm:number[])=>({pointMm,strictAvailable:strict.has(JSON.stringify(pointMm)),localizedAvailable:localized.has(JSON.stringify(pointMm))}));
 assert.equal(classification.length,24);assert.ok(classification.every((c:any)=>c.strictAvailable===f.expectedStrictAvailable&&c.localizedAvailable===f.expectedLocalizedAvailable));
 console.log('THERMAL_FROZEN_BOUNDARY '+JSON.stringify({classification}));
});
