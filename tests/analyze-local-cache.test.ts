import test from 'node:test';import assert from 'node:assert/strict';
import {mkdtempSync,readFileSync,writeFileSync,readdirSync,rmSync,mkdirSync,statSync} from 'node:fs';import {join} from 'node:path';import {tmpdir} from 'node:os';import {spawnSync} from 'node:child_process';
import {LocalModelCache,canonical,digest,localExtractionKey} from '../src/analyze/local-model-cache.ts';
const fixture={status:'complete',model:{R:1},validity:{status:'conditional',reason:'finite band'},recovery:[1,2]};
const validate=(v:any)=>{assert.equal(v.status,'complete');assert.equal(v.model.R,1);assert.equal(v.validity.status,'conditional');assert.deepEqual(v.recovery,[1,2]);};
const dir=()=>mkdtempSync(join(tmpdir(),'analyze-model-cache-'));
test('cold, isolated memory hit, cleared/fresh-process disk restore and no assembly/training on hit',()=>{
 const d=dir();try{let calls=0;const cache=new LocalModelCache(d),key=cache.key({board:'B',port:'p',band:[1,2],core:'v'});
 const run=()=>{calls++;return structuredClone(fixture);};const cold=cache.obtain(key,run,validate);assert.equal(cold.evidence.status,'cold');assert.equal(calls,1);
 cold.value.model.R=9;const warm=cache.obtain(key,run,validate);assert.equal(warm.evidence.status,'memory');assert.equal(warm.value.model.R,1);assert.equal(warm.evidence.assembly,0);assert.equal(warm.evidence.training,0);assert.equal(calls,1);
 const code=`import {LocalModelCache} from './src/analyze/local-model-cache.ts';const c=new LocalModelCache(${JSON.stringify(d)});let calls=0;const r=c.obtain(${JSON.stringify(key)},()=>{calls++;throw Error('unexpected training')},v=>{if(v.model.R!==1)throw Error('bad')});console.log(JSON.stringify({e:r.evidence,calls,value:r.value}));`;
 const fresh=spawnSync(process.execPath,['--import','tsx','--input-type=module','-e',code],{encoding:'utf8'});assert.equal(fresh.status,0,fresh.stderr);const restored=JSON.parse(fresh.stdout);assert.equal(restored.e.status,'disk');assert.equal(restored.calls,0);assert.deepEqual(restored.value,fixture);
 }finally{rmSync(d,{recursive:true,force:true});}
});
test('canonical identity invalidates board/port/stack/material/band/temperature/core and schema, excludes solve/plot/check',()=>{
 const c=new LocalModelCache(dir());try{const base={board:'b',ports:['a','r'],stack:[1,2],material:58e6,band:[1000,1e5],temperature:25,core:'v1'};const key=c.key(base);
 for(const k of Object.keys(base))assert.notEqual(c.key({...base,[k]:'changed'}),key,k);assert.equal(c.key({b:2,a:1}),c.key({a:1,b:2}));assert.throws(()=>canonical({bad:NaN}));
 }finally{rmSync(c.dir,{recursive:true,force:true});}
});
test('corruption, truncation, schema, checksum and valid-envelope invalid coefficients become safe misses',()=>{
 const d=dir();try{const c=new LocalModelCache(d);const key=c.key({x:1});let calls=0;const run=()=>{calls++;return structuredClone(fixture);};c.obtain(key,run,validate);const path=join(d,key+'.json'),original=readFileSync(path,'utf8');
 for(const bad of ['{',original.slice(0,-5),original.replace('"schema":1','"schema":2'),original.replace('checksum','broken')]){c.clear();writeFileSync(path,bad);assert.equal(c.obtain(key,run,validate).evidence.status,'cold');}
 assert.equal(calls,5);c.clear();const poisoned=JSON.stringify({...fixture,model:{R:-1}});writeFileSync(path,JSON.stringify({schema:1,key,checksum:digest(poisoned),payload:poisoned}));assert.equal(c.obtain(key,run,validate).evidence.status,'cold');assert.equal(calls,6);c.clear();writeFileSync(path,'x'.repeat(200));const bounded=new LocalModelCache(d,100,100,100);assert.equal(bounded.obtain(key,run,validate).evidence.status,'cold');
 }finally{rmSync(d,{recursive:true,force:true});}
});
test('failed prepare/validation/write, bounded memory/disk eviction, concurrent atomic writers',async()=>{
 const d=dir();try{const c=new LocalModelCache(d,700,700,700,1),key=c.key({x:1});assert.throws(()=>c.obtain(key,()=>{throw Error('failed')},validate));assert.deepEqual(readdirSync(d),[]);assert.throws(()=>c.obtain(key,()=>({...fixture,status:'partial'}),validate));assert.deepEqual(readdirSync(d),[]);
 const blocker=join(d,'file');writeFileSync(blocker,'x');const failed=new LocalModelCache(blocker);assert.equal(failed.obtain(key,()=>fixture,validate).evidence.writeFailed,true);
 for(let i=0;i<8;i++)c.obtain(c.key({i}),()=>fixture,validate);assert.ok(readdirSync(d).filter(x=>x.endsWith('.json')).length<=2);assert.ok(readdirSync(d).filter(x=>x.endsWith('.json')).reduce((n,x)=>n+statSync(join(d,x)).size,0)<=700);assert.equal(c.obtain(c.key({i:0}),()=>fixture,validate).evidence.status,'cold');
 const fresh=new LocalModelCache(join(d,'concurrent'));const k=fresh.key({a:1});const code=`import{LocalModelCache}from'./src/analyze/local-model-cache.ts';const c=new LocalModelCache(${JSON.stringify(fresh.dir)});c.obtain(${JSON.stringify(k)},()=>(${JSON.stringify(fixture)}),()=>{});`;
 await Promise.all([1,2,3].map(()=>new Promise<void>((resolve,reject)=>{const p=import('node:child_process').then(({spawn})=>{const child=spawn(process.execPath,['--import','tsx','--input-type=module','-e',code]);child.on('exit',n=>n===0?resolve():reject(Error(String(n))));});})));
 assert.equal(fresh.obtain(k,()=>{throw Error('miss')},validate).evidence.status,'disk');assert.ok(readdirSync(fresh.dir).every(x=>x.endsWith('.json')));
 }finally{rmSync(d,{recursive:true,force:true});}
});

test('actual extraction producer identity binds captured board, contacts, stack, materials, band, temperature and binary, while solve/report changes reuse',()=>{const d=dir();try{const c=new LocalModelCache(d);const geometry=JSON.parse(readFileSync('tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json','utf8'));const request={boardJson:'{"captured":"board"}',geometry,bandHz:[1000,100000],temperatureC:25};const key=localExtractionKey(c,'binary-v1',request);const changes=[{...request,boardJson:'{"captured":"edited"}'},{...request,geometry:{...geometry,ports:[['signal-TOP','signal-BOTTOM']]}},{...request,geometry:{...geometry,stack:geometry.stack.map((l:any,i:number)=>i===1?{...l,thicknessM:.0016}:l)}},{...request,geometry:{...geometry,barrelConductivitySM:5e7}},{...request,bandHz:[1000,200000]},{...request,temperatureC:40},{...request,magneticPatchLevel:1 as const},{...request,magneticPatchLevel:2 as const}];for(const r of changes)assert.notEqual(localExtractionKey(c,'binary-v1',r),key);assert.notEqual(localExtractionKey(c,'binary-v2',request),key);const changedSolve={...request,drive:[1,-1],loadsOhm:[100,100],plot:{color:'red'},check:{max:1}};assert.equal(localExtractionKey(c,'binary-v1',changedSolve),key);}finally{rmSync(d,{recursive:true,force:true});}});

test('explicit magnetic profile separates binary cache identities and rejects unknown choices',async()=>{
 const {localCoreIdentity}=await import('../src/analyze/local-model-cache.ts');
 const before=process.env.P7_MAGNETIC_PROFILE;
 try{delete process.env.P7_MAGNETIC_PROFILE;const baseline=localCoreIdentity();process.env.P7_MAGNETIC_PROFILE='baseline';assert.equal(localCoreIdentity(),baseline);process.env.P7_MAGNETIC_PROFILE='compact-v1';assert.notEqual(localCoreIdentity(),baseline);process.env.P7_MAGNETIC_PROFILE='auto';assert.throws(()=>localCoreIdentity(),/unsupported P7_MAGNETIC_PROFILE/);}
 finally{if(before===undefined)delete process.env.P7_MAGNETIC_PROFILE;else process.env.P7_MAGNETIC_PROFILE=before;}
});

test('one executor refuses magnetic profile changes before accessing a differently configured native model',async()=>{
 const {createProblemExecutor}=await import('../src/analyze/execute-problem.ts');const run=createProblemExecutor();const old=process.env.P7_MAGNETIC_PROFILE;
 const request:any={kind:'local-extraction',request:{boardJson:'{}',geometry:{},bandHz:[1000,100000],frequenciesHz:[1000],magneticPatchLevel:1}};
 try{process.env.P7_MAGNETIC_PROFILE='baseline';assert.throws(()=>run(request));process.env.P7_MAGNETIC_PROFILE='compact-v1';assert.throws(()=>run(request),/magnetic profile changed during executor lifetime/);}
 finally{if(old===undefined)delete process.env.P7_MAGNETIC_PROFILE;else process.env.P7_MAGNETIC_PROFILE=old;}
});


test('rejected experimental precision cannot silently restore an F64 model',async()=>{
 const {localCoreIdentity}=await import('../src/analyze/local-model-cache.ts');const {createProblemExecutor}=await import('../src/analyze/execute-problem.ts');
 const old=process.env.P7_MAGNETIC_PRECISION;try{
 delete process.env.P7_MAGNETIC_PRECISION;const baseline=localCoreIdentity();process.env.P7_MAGNETIC_PRECISION='f64';assert.equal(localCoreIdentity(),baseline);
 const run=createProblemExecutor();const request:any={kind:'local-extraction',request:{boardJson:'{}',geometry:{},bandHz:[1000,100000],frequenciesHz:[1000],magneticPatchLevel:1}};assert.throws(()=>run(request));
 for(const unsupported of ['f32','auto']){process.env.P7_MAGNETIC_PRECISION=unsupported;assert.throws(()=>localCoreIdentity(),/unsupported P7_MAGNETIC_PRECISION/);assert.throws(()=>run(request),/unsupported P7_MAGNETIC_PRECISION/);}
 }finally{if(old===undefined)delete process.env.P7_MAGNETIC_PRECISION;else process.env.P7_MAGNETIC_PRECISION=old;}
});

test('channel magnetic cache survives executor restart and excludes changing resource budgets from identity',()=>{
 const d=dir();try{
  const request={blocks:[{id:'a',dofs:1,cells:[{verticesM:[[0,0,0],[.001,0,0],[0,.001,0],[0,0,.001]],currentsPerAmpM2:[[0,0,1e6]]}]},{id:'b',dofs:1,cells:[{verticesM:[[.01,0,0],[.011,0,0],[.01,.001,0],[.01,0,.001]],currentsPerAmpM2:[[0,0,1e6]]}]}],order:8,budget:{maxSeconds:30,maxMemoryMiB:64}};
  const launch=(code:string)=>{const p=spawnSync(process.execPath,['--import','tsx','--input-type=module','-e',code],{cwd:process.cwd(),env:{...process.env,PCB_ANALYSIS_CACHE_DIR:d},encoding:'utf8'});assert.equal(p.status,0,p.stderr);return JSON.parse(p.stdout);};
  const initial=launch(`import{createProblemExecutor}from'./src/analyze/execute-problem.ts';const request=${JSON.stringify(request)};const run=()=>createProblemExecutor()({kind:'channel-magnetic',request});const cold=run(),memory=run();request.budget.maxSeconds=20;const budget=run();console.log(JSON.stringify({cold,memory,budget}));`);
  assert.equal(initial.cold.cache.status,'cold');assert.equal(initial.cold.cache.assembly,1);assert.equal(initial.memory.cache.status,'memory');assert.equal(initial.memory.cache.assembly,0);assert.equal(initial.budget.cache.status,'memory');assert.deepEqual(initial.memory.couplings,initial.cold.couplings);
  const restored=launch(`import{createProblemExecutor}from'./src/analyze/execute-problem.ts';const r=createProblemExecutor()({kind:'channel-magnetic',request:${JSON.stringify(request)}});console.log(JSON.stringify(r));`);
  assert.equal(restored.cache.status,'disk');assert.equal(restored.cache.assembly,0);assert.deepEqual(restored.couplings,initial.cold.couplings);
 }finally{rmSync(d,{recursive:true,force:true});}
});
