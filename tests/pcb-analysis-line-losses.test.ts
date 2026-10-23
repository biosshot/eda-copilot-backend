import test from 'node:test';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync} from 'node:fs';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
import {sectionResults} from './helpers/pcb-analysis-cross-section-audit.ts';
const addon=createRequire(import.meta.url)('../native/node-addon/index.cjs');
const copper={conductivitySM:5.8e7,state:'smooth fixed-temperature copper hypothesis'};
const line={kind:'microstrip',widthMm:.3,copperThicknessMm:.035,dielectricHeightMm:.18,lengthMm:20,material:{relativePermittivity:4.2,lossTangent:.015,state:'constant dielectric hypothesis within band'},conductors:{signal:copper,referencePlanes:{bottom:{thicknessMm:.035,material:copper}}}};
const options={maxOutputBytes:128*1024*1024,timeoutMs:180000};
const frequency={startHz:5e8,stopHz:1e9,points:3,scale:'linear'};
const close=(a:number,b:number)=>assert.ok(Math.abs(a-b)<1e-10+Math.abs(b)*1e-8,`${a} != ${b}`);

test('lossy direct DSL -> complex impedance/phase delay -> P6b renormalization/connect and immutable old network',async()=>{
 const code=`const a=analysis();const line=a.line(${JSON.stringify(line)});const builder=a.impedance().target(line).method('cross-section').frequency(${JSON.stringify(frequency)});const preparation=await builder.prepare();a.log({level:'info',message:'prepare',data:preparation});const z=await builder.run();a.plot({title:'Z real',data:z.characteristicImpedance.real()});a.plot({title:'Z imaginary',data:z.characteristicImpedance.imaginary()});const old=z.network;const r=await a.networkParameters().network(old).frequency(${JSON.stringify(frequency)}).representation('S').referenceOhm(50).run();const second=(await builder.run()).network;const cascade=a.connect({networks:[old,second],connections:[{from:old.port('output'),to:second.port('input')}],ports:{input:old.port('input'),output:second.port('output')}});await a.networkParameters().network(cascade).frequencyHz(1e9).representation('S').referenceOhm(50).run();const renormalized=await a.networkParameters().network(old).frequencyHz(1e9).representation('S').referenceOhm(75).run();const again=await a.networkParameters().network(old).frequency(${JSON.stringify(frequency)}).representation('S').referenceOhm(50).run();a.check({name:'expected fail',actual:r.parameter({out:'output',in:'input'}).magnitude().min(),target:0,tolerance:.01,unit:'1'});a.check({name:'continued',actual:again.parameter({out:'output',in:'input'}).magnitude().min(),min:.1,max:1,unit:'1'});a.plot({title:'continued',data:again.parameter({out:'output',in:'input'}).phaseDeg()});`;
 const result=await runAnalysisDsl({code},options);assert.equal(result.status,'complete',JSON.stringify(result.error));
 const r=sectionResults(result),z=r[0];assert.equal(z.characteristicImpedance.kind,'series');assert.equal(z.characteristicImpedance.unit,'Ohm');assert.equal(z.characteristicImpedance.values.length,3);assert.equal(z.delay.kind,'series');assert.equal(z.delay.unit,'s');assert.equal(z.rlgc.data.samples.length,3);assert.deepEqual(z.rlgc.data.frequenciesHz,[5e8,7.5e8,1e9]);assert.deepEqual(r[1].solution.parameters,r.at(-1).solution.parameters);assert.deepEqual(result.reports[0].checks.map(c=>c.status),['fail','pass']);
 const preparation=(result.reports[0].logs[0]as any).data;assert.equal(preparation.physicalReady,false);assert.equal(preparation.structurallyValid,true);assert.ok(preparation.diagnostics.some((d:any)=>d.code==='SECTION_LOSS_APPLICABILITY_UNVERIFIED'));
 for(const sample of z.rlgc.data.samples){assert.equal(sample.surfaceContributions.length,2);assert.equal(sample.regionContributions.length,2);assert.ok(sample.surfaceContributions[1].resistanceOhmPerM[0][0]>0);assert.ok(sample.modes[0].alphaNpPerM>0);}
});

test('four modes, regional air participation, conductivity/tangent scaling and no frequency field fanout',()=>{
 const session=new addon.AnalysisElectricalSession();const solve=(input:any,frequencies=[5e8,1e9])=>{const r=session.solveCrossSection({line:input,frequencies});assert.equal(r.status,'complete',JSON.stringify(r.diagnostics));return r;};
 const {conductors:_,...pec}=line,ideal={...pec,material:{...pec.material,lossTangent:0}},conductorOnly={...line,material:ideal.material};
 const baseline=solve(ideal),dielectric=solve(pec),conductor=solve(conductorOnly),combined=solve(line),scaled=solve({...line,conductors:{signal:{...copper,conductivitySM:4*copper.conductivitySM},referencePlanes:{bottom:{thicknessMm:.035,material:{...copper,conductivitySM:4*copper.conductivitySM}}}}}),single=solve(line,[1e9]);
 assert.equal(baseline.parameters.frequencyDependent,undefined);assert.equal(dielectric.parameters.frequencyDependent.conductorModel,'explicit PEC');assert.deepEqual(combined.field,single.field);assert.deepEqual(combined.lossIntegrals,single.lossIntegrals);assert.deepEqual(combined.convergence,single.convergence);
 for(let k=0;k<2;k++){const b=combined.parameters.frequencyDependent.samples[k],d=dielectric.parameters.frequencyDependent.samples[k],c=conductor.parameters.frequencyDependent.samples[k],s=scaled.parameters.frequencyDependent.samples[k];close(b.resistanceOhmPerM[0][0],c.resistanceOhmPerM[0][0]);close(d.resistanceOhmPerM[0][0],0);close(c.conductanceSPerM[0][0],0);close(b.conductanceSPerM[0][0],d.conductanceSPerM[0][0]);close(s.resistanceOhmPerM[0][0],b.resistanceOhmPerM[0][0]/2);close(s.conductanceSPerM[0][0],b.conductanceSPerM[0][0]);assert.equal(b.regionContributions[1].id,'air');assert.equal(b.regionContributions[1].conductanceSPerM[0][0],0);assert.ok(b.conductanceSPerM[0][0]<2*Math.PI*b.frequencyHz*b.capacitanceFPerM[0][0]*.015);}
 session.clear();
});

test('native lossy applicability/material/budget refusals retain exact frequency and permit next success',()=>{
 const session=new addon.AnalysisElectricalSession();
 for(const [label,request,code]of [
  ['DC',{line,frequencies:[0,1e9]},'SECTION_LOSS_FREQUENCY'],['skin band',{line,frequencies:[1e6,1e9]},'SECTION_SKIN_DEPTH'],
  ['thin reference',{line:{...line,conductors:{...line.conductors,referencePlanes:{bottom:{thicknessMm:.001,material:copper}}}},frequencies:[1e9]},'SECTION_SKIN_DEPTH'],
  ['sigma zero',{line:{...line,conductors:{...line.conductors,signal:{...copper,conductivitySM:0}}},frequencies:[1e9]},'SECTION_CONDUCTOR_MATERIAL'],
  ['excess tangent',{line:{...line,material:{...line.material,lossTangent:.051}},frequencies:[1e9]},'SECTION_MATERIAL'],
  ['missing plane',{line:{...line,conductors:{signal:copper,referencePlanes:{}}},frequencies:[1e9]},'SECTION_CONDUCTOR_MATERIAL'],
  ['roughness',{line:{...line,conductors:{...line.conductors,signal:{...copper,roughnessUm:1}}},frequencies:[1e9]},'SECTION_CONDUCTOR_MATERIAL'],
  ['memory',{line,frequencies:[1e9],budget:{maxMemoryMiB:1}},'SECTION_MEMORY_LIMIT'],['deadline',{line,frequencies:[1e9],budget:{maxSeconds:1e-12}},'SECTION_TIMEOUT'],
 ]as const){const r=session.solveCrossSection(request);assert.equal(r.status,'rejected',label);assert.equal(r.diagnostics[0].code,code,label);if(label==='skin band')assert.match(r.diagnostics[0].message,/1000000 Hz/);}
 assert.equal(session.solveCrossSection({line,frequencies:[1e9]}).status,'complete');session.clear();
});

test('full lossy pair: P6b cascade of two lengths equals total length and preserves conductor ordering',()=>{
 const session=new addon.AnalysisElectricalSession();
 const pair={kind:'coupled-stripline',widthMm:.3,copperThicknessMm:.035,gapMm:.2,lengthMm:20,bottomDielectricHeightMm:.18,topDielectricHeightMm:.25,bottomMaterial:line.material,topMaterial:line.material,conductors:{...line.conductors,referencePlanes:{...line.conductors.referencePlanes,top:{thicknessMm:.035,material:copper}}}};
 const short=session.solveCrossSection({line:pair,frequencies:[1e9],mesh:{maxNodes:300000,maxTriangles:600000},budget:{maxMemoryMiB:1024}}),long=session.solveCrossSection({line:{...pair,lengthMm:40},frequencies:[1e9],mesh:{maxNodes:300000,maxTriangles:600000},budget:{maxMemoryMiB:1024}});
 assert.equal(short.status,'complete',JSON.stringify(short.diagnostics));assert.equal(long.status,'complete',JSON.stringify(long.diagnostics));const {geometry:shortGeometry,...shortField}=short.field,{geometry:longGeometry,...longField}=long.field;assert.deepEqual(shortField,longField);const {length:shortLength,...shortSection}=shortGeometry,{length:longLength,...longSection}=longGeometry;assert.deepEqual(shortSection,longSection);assert.equal(shortLength,.02);assert.equal(longLength,.04);
 const model={id:'paired-loss',kind:'network',ports:short.network.ports,validity:{status:'conditional',reason:'surface impedance perturbation'},data:{format:'network-parameters',representation:'S',referenceTopology:'common',nature:'passive',referenceOhm:50,frequenciesHz:[1e9],parameters:short.network.parameters}};
 const combined=session.solveFrequency({instances:[{id:'first',model,ports:short.network.ports},{id:'second',model,ports:short.network.ports}],connections:[[2,4],[3,5]],ports:[{name:'positiveInput',index:0},{name:'negativeInput',index:1},{name:'positiveOutput',index:6},{name:'negativeOutput',index:7}],frequencies:[1e9],representation:'S',referenceOhm:[50,50,50,50]});
 assert.equal(combined.status,'complete',JSON.stringify(combined.diagnostics));
 for(let i=0;i<4;i++)for(let j=0;j<4;j++)for(const part of ['real','imaginary'])close(combined.parameters[0][i][j][part],long.network.parameters[0][i][j][part]);
 const permutation=[1,0,3,2];for(let i=0;i<4;i++)for(let j=0;j<4;j++)for(const part of ['real','imaginary'])close(short.network.parameters[0][i][j][part],short.network.parameters[0][permutation[i]][permutation[j]][part]);
 session.clear();
});

test('lossy missing length stays unavailable, structural unknowns/temperature/axis caps and process recovery',async()=>{
 const {lengthMm:_,...short}=line;
 const r=await runAnalysisDsl({code:`const a=analysis();const z=await a.impedance().target(a.line(${JSON.stringify(short)})).method('cross-section').frequencyHz(1e9).run();a.check({name:'no length',actual:z.delay,min:0,unit:'s'});try{z.network}catch(e){a.log({level:'info',message:e.code})}a.plot({title:'per metre',data:z.rlgc});`},options);assert.equal(r.status,'complete',JSON.stringify(r.error));assert.equal(r.reports[0].checks[0].status,'unknown');
 for(const input of [{...line,conductors:{...line.conductors,signal:{...copper,referenceC:20}}},{...line,conductors:{...line.conductors,referencePlanes:{...line.conductors.referencePlanes,top:{thicknessMm:.035,material:copper}}}}]){const bad=await runAnalysisDsl({code:`const a=analysis();a.line(${JSON.stringify(input)});`});assert.equal(bad.status,'failed');}
 for(const modifiers of [`.temperatureC(25)`,`.frequency({startHz:5e8,stopHz:1e9,points:1e9,scale:'linear'})`]){const bad=await runAnalysisDsl({code:`const a=analysis();await a.impedance().target(a.line(${JSON.stringify(line)})).method('cross-section').frequencyHz(1e9)${modifiers}.run();`},options);assert.equal(bad.status,'failed');assert.match(bad.error!.code,/SECTION_|INVALID_CONFIGURATION/);}
 const abort=new AbortController();abort.abort();assert.equal((await runAnalysisDsl({code:'const a=analysis();'},{signal:abort.signal})).error?.code,'ABORTED');
 assert.equal((await runAnalysisDsl({code:readFileSync('tests/fixtures/analysis-line-losses/example.js','utf8')},options)).status,'complete');
});
