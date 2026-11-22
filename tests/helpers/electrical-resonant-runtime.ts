import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdirSync,existsSync} from 'node:fs';
import {join,resolve,dirname} from 'node:path';
import {pathToFileURL} from 'node:url';
import {createRequire} from 'node:module';
import {spawnSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import {resonantResource,resonantPhysicalTime,resonantPhysicalAc} from './electrical-resonant-model.ts';
import {auditResonantRuntime} from './electrical-resonant-audit.ts';
const mode=process.argv[2]??'direct',output=resolve(process.argv[3]??`debugging/p6-resonant-runtime/${mode}`);mkdirSync(output,{recursive:true});
const saved=process.argv.includes('--saved');
const scenario=`const authored=${JSON.stringify(resonantResource())};const coarseAuthored=${JSON.stringify(resonantResource('mutual',257))};\n`+readFileSync('tests/fixtures/analysis-resonant-runtime/scenario.js','utf8');
const hash=(b:string|Buffer)=>createHash('sha256').update(b).digest('hex');
const {nativeFilename}=createRequire(import.meta.url)('../../native/node-addon/platform.cjs');
const nativeHash=hash(readFileSync(join('native/node-addon',nativeFilename())));
assert.equal(createRequire(import.meta.url)('../../native/node-addon/index.cjs').numericContract(),'f32-rte-ftz-v1');
let result:any;const start=performance.now();
if(saved){
 const old=JSON.parse(readFileSync(join(output,'result.json'),'utf8'));assert.equal(old.metadata.nativeSha256,nativeHash);result=old.result;
}else if(mode==='direct'){
 const {executeScenario}=await import('../../src/analyze/analysis-dsl/scenario.ts');
 result=await executeScenario({code:scenario},{filename:'resonant-direct.js',maxEntries:3000,maxOutputBytes:32*1024*1024});
}else if(mode==='compiled'){
 const {runAnalysisDsl,analysisDslDocumentation}=await import(pathToFileURL(resolve('dist/index.js')).href);
 assert.match(analysisDslDocumentation(),/passive-resonant-y/);
 result=await runAnalysisDsl({code:scenario},{filename:'resonant-compiled.js',timeoutMs:60000,maxEntries:3000,maxOutputBytes:32*1024*1024});
}else if(mode==='installed'){
 const npmCli=process.env.npm_execpath??join(dirname(process.execPath),'node_modules/npm/bin/npm-cli.js');assert.ok(existsSync(npmCli),'Set npm_execpath');
 const cleanEnv={...process.env};for(const key of Object.keys(cleanEnv))if(/^(?:PCB_.*|NODE_PATH|NODE_OPTIONS|OPENAI_API_KEY|ANTHROPIC_API_KEY|EASYEDA_COPILOT_SERVER_URL)$/i.test(key))delete cleanEnv[key];
 const run=(args:string[],cwd:string)=>{const p=spawnSync(process.execPath,args,{cwd,env:cleanEnv,encoding:'utf8',windowsHide:true,timeout:60000});if(p.error)throw p.error;assert.equal(p.status,0,p.stdout+'\n'+p.stderr);return p.stdout;};
 const pack=JSON.parse(run([npmCli,'pack','--ignore-scripts','--json','--pack-destination',output],process.cwd()))[0];
 assert.equal(pack.files.filter((p:any)=>/native\/node-addon\/.*\.node$/.test(p.path)).length,1);
 const consumer=join(output,'consumer');mkdirSync(consumer,{recursive:true});writeFileSync(join(consumer,'package.json'),JSON.stringify({name:'resonant-installed-consumer',private:true,type:'module'}));
 const archive=join(output,pack.filename);run([npmCli,'install','--ignore-scripts','--no-audit','--no-fund',archive],consumer);
 writeFileSync(join(consumer,'scenario.js'),scenario);
 writeFileSync(join(consumer,'smoke.mjs'),`import assert from 'node:assert/strict';import{readFileSync,writeFileSync}from'node:fs';import{createRequire}from'node:module';import{runAnalysisDsl,analysisDslDocumentation}from'eda-copilot-backend';const require=createRequire(import.meta.url);const native=require('./node_modules/eda-copilot-backend/native/node-addon/index.cjs');assert.equal(native.numericContract(),'f32-rte-ftz-v1');assert.match(analysisDslDocumentation(),/passive-resonant-y/);const result=await runAnalysisDsl({code:readFileSync('scenario.js','utf8')},{timeoutMs:60000,maxEntries:3000,maxOutputBytes:32*1024*1024});writeFileSync('result.json',JSON.stringify(result));assert.equal(result.status,'complete',JSON.stringify(result.error));`);
 run(['smoke.mjs'],consumer);result=JSON.parse(readFileSync(join(consumer,'result.json'),'utf8'));
 const installedHash=hash(readFileSync(join(consumer,'node_modules/eda-copilot-backend/native/node-addon',nativeFilename())));assert.equal(installedHash,nativeHash);
 writeFileSync(join(output,'package.json'),JSON.stringify({archive,archiveSha256:hash(readFileSync(archive)),nativeSha256:installedHash,packageBytes:pack.size},null,2));
}else throw Error('direct|compiled|installed');
if(!saved){writeFileSync(join(output,'scenario.js'),scenario);
writeFileSync(join(output,'result.json'),JSON.stringify({metadata:{mode,nativeSha256:nativeHash,scenarioSha256:hash(scenario),wallMs:performance.now()-start},result}));}
const audit=auditResonantRuntime(result);writeFileSync(join(output,'audit.json'),JSON.stringify(audit,null,2));console.log(JSON.stringify(audit));
const completed=result.reports.flatMap((r:any)=>r.calculations).filter((c:any)=>c.phase==='completed');
const fine=completed[1].result.solution,ac=completed[2].result.solution;
writeFileSync(join(output,'reference.json'),JSON.stringify({timeS:fine.axis,time:fine.axis.map(resonantPhysicalTime),frequencyHz:ac.frequenciesHz,admittanceS:ac.frequenciesHz.map(resonantPhysicalAc)}));
