import test from 'node:test';
import assert from 'node:assert/strict';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
import {mkdtempSync, readFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {spawnSync} from 'node:child_process';
import {createRequire} from 'node:module';
const addon=createRequire(import.meta.url)('../native/node-addon/index.cjs');
const line={kind:'microstrip',widthMm:.3,copperThicknessMm:.035,dielectricHeightMm:.18,lengthMm:20,material:{relativePermittivity:4.2,lossTangent:0,state:'Explicit PEC/lossless test hypothesis'}};
const start=`const a=analysis();const l=a.line(${JSON.stringify(line)});`;
const solve=`a.impedance().target(l).method('cross-section').frequency({startHz:1e6,stopHz:1e9,points:7,scale:'linear'}).run()`;
const options={maxOutputBytes:128*1024*1024,timeoutMs:120000};
const results=(r:any)=>r.reports.flatMap((r:any)=>r.calculations).filter((c:any)=>c.phase==='completed').map((c:any)=>c.result);
test('production FEM -> immutable NetworkResource -> P6b cascade; expected fail continues',async()=>{
 const r=await runAnalysisDsl({code:`${start}const z=await ${solve};a.check({name:'expected fail',actual:z.characteristicImpedance,target:0,tolerance:1,unit:'Ohm'});a.check({name:'continued',actual:z.characteristicImpedance,min:1,max:500,unit:'Ohm'});a.plot({title:'Z0',data:z.characteristicImpedance});const first=await a.networkParameters().network(z.network).frequency({startHz:1e6,stopHz:1e9,points:7,scale:'linear'}).representation('S').referenceOhm(50).run();a.plot({title:'phase',data:first.parameter({out:'output',in:'input'}).phaseDeg()});const pad=a.network({model:a.model({id:'attenuatorPad',kind:'network',ports:['p1','p2'],data:{format:'network-parameters',representation:'S',referenceTopology:'common',nature:'passive',referenceOhm:50,frequenciesHz:[1e6,1e9],parameters:[[[{real:0,imaginary:0},{real:.5,imaginary:0}],[{real:.5,imaginary:0},{real:0,imaginary:0}]],[[{real:0,imaginary:0},{real:.5,imaginary:0}],[{real:.5,imaginary:0},{real:0,imaginary:0}]]]}}),ports:{input:'p1',output:'p2'}});const old=z.network;const connected=a.connect({networks:[old,pad],connections:[{from:old.port('output'),to:pad.port('input')}],ports:{input:old.port('input'),output:pad.port('output')}});const cascade=await a.networkParameters().network(connected).frequencyHz(1e6).representation('S').referenceOhm(75).run();a.plot({title:'cascade',data:cascade.parameter({out:'output',in:'input'}).magnitude()});const again=await a.networkParameters().network(old).frequencyHz(1e6).representation('S').referenceOhm(50).run();a.plot({title:'unchanged',data:again.parameter({out:'output',in:'input'}).phaseDeg()});`},options);
 assert.equal(r.status,'complete',JSON.stringify(r.error));assert.deepEqual(r.reports[0].checks.map(c=>c.status),['fail','pass']);const [z,f,c,again]=results(r);assert.equal(z.solution.field.quality.actual.rhs,1);assert.equal(z.solution.field.quality.actual.factorizations,1);assert.ok(z.solution.field.quality.actual.normalizedResidual<=1e-8);assert.deepEqual(z.solution.network.ports,['input','output']);assert.deepEqual(f.solution.parameters,z.solution.network.parameters);assert.deepEqual(f.solution.parameters[0],again.solution.parameters[0]);assert.equal(c.solution.provenance.length,2);assert.equal(z.rlgc.unit,'1');assert.match(z.rlgc.data.resistanceOhmPerM.state,/assumed/);
});
test('no length returns per-metre values, delay/network unavailable; paired modes/order/units',async()=>{
 const {lengthMm:_,...noLength}=line;const r=await runAnalysisDsl({code:`const a=analysis();const z=await a.impedance().target(a.line(${JSON.stringify(noLength)})).method('cross-section').frequencyHz(1e8).run();a.check({name:'no delay',actual:z.delay,unit:'s',min:0});try{z.network}catch(e){a.log({level:'info',message:e.code});}a.check({name:'after missing length',actual:z.characteristicImpedance,unit:'Ohm',min:1});`},options);
 assert.equal(r.status,'complete',JSON.stringify(r.error));assert.deepEqual(r.reports[0].checks.map(c=>c.status),['unknown','pass']);assert.equal(results(r)[0].network,undefined);
 const pair={...line,kind:'coupled-stripline',topDielectricHeightMm:.3,bottomDielectricHeightMm:.3,topMaterial:line.material,bottomMaterial:line.material,gapMm:.3}as any;delete pair.dielectricHeightMm;delete pair.material;
 const p=await runAnalysisDsl({code:`const a=analysis();const r=await a.impedance().target(a.line(${JSON.stringify(pair)})).method('cross-section').frequencyHz(1e8).run();a.plot({title:'modes',data:r.characteristicImpedance});const n=await a.networkParameters().network(r.network).frequencyHz(1e8).representation('S').referenceOhm(50).run();a.plot({title:'cross',data:n.parameter({out:'negativeOutput',in:'positiveInput'}).imaginary()});`},options);
 assert.equal(p.status,'complete',JSON.stringify(p.error));const z=results(p)[0];assert.deepEqual(z.solution.network.ports,['positiveInput','negativeInput','positiveOutput','negativeOutput']);assert.deepEqual(z.characteristicImpedance.data.rows.map((r:any)=>r.name),['even','odd','differential','common']);assert.equal(z.characteristicImpedance.unit,'Ohm');assert.equal(z.delay.unit,'s');assert.ok(z.solution.parameters.capacitanceFPerM[0][1]<0);assert.ok(z.solution.parameters.inductanceHPerM[0][1]>0);
});
test('unsupported/excess-loss/missing materials, impossible mesh, caps and next success are explicit',async(t)=>{
 const previousCache = process.env.PCB_ANALYSIS_CACHE_DIR;
 const coldCache = mkdtempSync(join(tmpdir(), 'eda-section-budget-'));
 process.env.PCB_ANALYSIS_CACHE_DIR = coldCache;
 t.after(() => {
  if (previousCache === undefined) delete process.env.PCB_ANALYSIS_CACHE_DIR;
  else process.env.PCB_ANALYSIS_CACHE_DIR = previousCache;
  rmSync(coldCache, {recursive:true, force:true});
 });
 for(const input of [{...line,material:{...line.material,lossTangent:.051}},{...line,material:{state:'missing'}},{...line,material:{...line.material,conductivitySPerM:0}},{...line,solderMask:{}}]){const r=await runAnalysisDsl({code:`const a=analysis();await a.impedance().target(a.line(${JSON.stringify(input)})).method('cross-section').frequencyHz(1e8).run();`},options);assert.equal(r.status,'failed');assert.match(r.error!.code,/SECTION_|INVALID_CONFIGURATION/);}
 const unsupported=await runAnalysisDsl({code:`const a=analysis();await a.impedance().target({id:'future-resource',kind:'unimplemented',analyses:['impedance']}).method('cross-section').frequencyHz(1e8).run();`},options);assert.equal(unsupported.error?.code,'SECTION_UNSUPPORTED');
 for(const modifiers of [`.budget({maxMemoryMiB:1})`,`.method('layered')`,`.frequencyHz(1e15)`,`.mesh({maxNodes:16})`,`.mesh({maxRefinements:1})`,`.budget({maxSeconds:1e-9})`,`.frequency({startHz:1e6,stopHz:1e9,points:1e9,scale:'linear'})`]){const r=await runAnalysisDsl({code:`${start}try{await a.impedance().target(l).method('cross-section')${modifiers.startsWith('.frequency(')?'':'.frequencyHz(1e8)'}${modifiers}.run();}catch(e){a.log({level:'info',message:e.code});}const z=await a.impedance().target(l).method('cross-section').frequencyHz(1e8).run();a.check({name:'recovery',actual:z.characteristicImpedance,unit:'Ohm',min:1});`},options);assert.equal(r.status,'complete',JSON.stringify(r.error));assert.equal(r.reports[0].logs.length,1);assert.match(r.reports[0].logs[0].message,/SECTION_/);assert.equal(r.reports[0].checks[0].status,'pass');}
});
test('Node/Rust transport parity and native error recovery; no frequency fan-out',()=>{
 const s=new addon.AnalysisElectricalSession();const a=s.solveCrossSection({line,frequencies:[1e6]});const b=s.solveCrossSection({line,frequencies:[1e6,1e8,1e9]});assert.equal(a.status,'complete',JSON.stringify(a));assert.deepEqual(a.parameters,b.parameters);assert.deepEqual(a.field,b.field);assert.deepEqual(a.convergence,b.convergence);const original=JSON.stringify(a.network),short=s.solveCrossSection({line:{...line,lengthMm:10},frequencies:[1e6]});assert.deepEqual(short.parameters,a.parameters);assert.notDeepEqual(short.network.parameters,a.network.parameters);const wider=s.solveCrossSection({line:{...line,widthMm:.6},frequencies:[1e6]});assert.notEqual(wider.parameters.capacitanceFPerM[0][0],a.parameters.capacitanceFPerM[0][0]);assert.equal(JSON.stringify(a.network),original);assert.throws(()=>s.solveCrossSection({line,frequencies:[1e6],unknown:1}));const bad=s.solveCrossSection({line:{...line,lengthMm:0},frequencies:[1e6]});assert.equal(bad.status,'rejected');assert.equal(s.solveCrossSection({line,frequencies:[1e6]}).status,'complete');s.clear();
});
test('timeout/cancel/report limit terminate existing process then successful solve',async()=>{
 const stopped=await runAnalysisDsl({code:`${start}await ${solve};`},{...options,timeoutMs:1});assert.equal(stopped.error?.code,'TIMEOUT');const abort=new AbortController();abort.abort();assert.equal((await runAnalysisDsl({code:start},{signal:abort.signal})).error?.code,'ABORTED');const limited=await runAnalysisDsl({code:`${start}await ${solve};`},{maxOutputBytes:4096});assert.equal(limited.status,'failed');const good=await runAnalysisDsl({code:`${start}await ${solve};`},options);assert.equal(good.status,'complete',JSON.stringify(good.error));
});

test('Node and standalone Rust import the same cross section, counts and diagnostics',()=>{
 const request={line,frequencies:[1e6]},a=new addon.AnalysisElectricalSession().solveCrossSection(request);
 const build=spawnSync('cargo',['build','--release','--locked','--manifest-path','native/analyze/Cargo.toml','--example','cross-section','--message-format=json'],{encoding:'utf8',windowsHide:true,timeout:900000,maxBuffer:8*1024*1024});
 assert.ifError(build.error);assert.equal(build.status,0,build.stderr);
 const executable=build.stdout.trim().split('\n').map(line=>JSON.parse(line)).find(entry=>entry.reason==='compiler-artifact'&&entry.target.name==='cross-section'&&entry.executable)?.executable;
 assert.ok(executable,'Cargo did not return the cross-section example executable');
 const r=spawnSync(executable,[],{input:JSON.stringify(request),encoding:'utf8',windowsHide:true,timeout:30000,maxBuffer:8*1024*1024});
 assert.ifError(r.error);assert.equal(r.status,0,r.stderr);
 const b=JSON.parse(r.stdout);assert.deepEqual(b,{status:a.status,diagnostics:a.diagnostics,parameters:a.parameters,quality:a.field.quality,nodes:a.field.nodes.length,triangles:a.field.triangles.length});
});

test('direct task example runs through installed worker vocabulary without casts',async()=>{const code=readFileSync('tests/fixtures/analysis-cross-section/example.js','utf8');const r=await runAnalysisDsl({code});assert.equal(r.status,'complete',JSON.stringify(r.error));assert.equal(results(r).length,2);assert.equal(r.reports[0].plots.length,4);});
