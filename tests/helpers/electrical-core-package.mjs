import assert from 'node:assert/strict';
import {spawnSync} from 'node:child_process';
import {mkdirSync,readFileSync,writeFileSync,copyFileSync,existsSync} from 'node:fs';
import {join,resolve,dirname} from 'node:path';
import {createHash} from 'node:crypto';
const output=resolve(process.argv[2]??'debugging/p6-verification/installed');mkdirSync(output,{recursive:true});
const npmCli=process.env.npm_execpath??join(dirname(process.execPath),'node_modules/npm/bin/npm-cli.js');assert.ok(existsSync(npmCli),'Set npm_execpath to the actual npm CLI');
const cleanEnv={...process.env};for(const key of Object.keys(cleanEnv))if(/^(?:PCB_.*|NODE_PATH|NODE_OPTIONS|OPENAI_API_KEY|ANTHROPIC_API_KEY|EASYEDA_COPILOT_SERVER_URL|KICAD_COPILOT_SERVER_URL)$/i.test(key))delete cleanEnv[key];
function run(args,cwd,timeout=180000){const r=spawnSync(process.execPath,args,{cwd,encoding:'utf8',env:cleanEnv,windowsHide:true,timeout});writeFileSync(join(output,'last-command.log'),String(r.stdout)+String(r.stderr));if(r.error)throw r.error;assert.equal(r.status,0,r.stdout+'\n'+r.stderr);return r.stdout;}
const pack=JSON.parse(run([npmCli,'pack','--ignore-scripts','--json','--pack-destination',output],process.cwd()))[0];
const archive=join(output,pack.filename),consumer=join(output,'consumer');mkdirSync(consumer,{recursive:true});writeFileSync(join(consumer,'package.json'),JSON.stringify({name:'p6-installed-consumer',private:true,type:'module'}));
assert.equal(pack.files.filter(f=>/native\/node-addon\/.*\.node$/.test(f.path)).length,1);
run([npmCli,'install','--ignore-scripts','--no-audit','--no-fund',archive],consumer);
copyFileSync('tests/fixtures/analysis-electrical-core/scenario.js',join(consumer,'scenario.js'));
writeFileSync(join(consumer,'smoke.mjs'),`
import assert from 'node:assert/strict';import{readFileSync,writeFileSync}from'node:fs';import{createRequire}from'node:module';import{join}from'node:path';
import{runAnalysisDsl,analysisDslDocumentation}from'eda-copilot-backend';
const require=createRequire(import.meta.url),root=join(process.cwd(),'node_modules/eda-copilot-backend');
const native=require(join(root,'native/node-addon/index.cjs'));assert.equal(native.numericContract(),'f32-rte-ftz-v1');assert.equal(typeof(new native.AnalysisElectricalSession()).solveTransient,'function');
assert.match(analysisDslDocumentation(),/passive-relaxation/);assert.match(analysisDslDocumentation(),/delayed-scattering/);
const code=readFileSync('scenario.js','utf8');const result=await runAnalysisDsl({code},{timeoutMs:120000,maxOutputBytes:32*1024*1024});writeFileSync('result.json',JSON.stringify(result));assert.equal(result.status,'complete',JSON.stringify(result.error));
assert.deepEqual(result.reports.flatMap(r=>r.checks).map(c=>c.status),['fail','pass']);
const timeout=await runAnalysisDsl({code:'const a=analysis();while(true){}'},{timeoutMs:25});assert.equal(timeout.error.code,'TIMEOUT');
const controller=new AbortController();controller.abort();const aborted=await runAnalysisDsl({code},{signal:controller.signal});assert.equal(aborted.error.code,'ABORTED');
const recovered=await runAnalysisDsl({code:'const a=analysis();a.check({name:"post-cancel recovery",actual:1,target:1,tolerance:0,unit:"1"});'});assert.equal(recovered.status,'complete');assert.equal(recovered.reports[0].checks[0].status,'pass');
console.log(JSON.stringify({status:'pass',nativeNumericContract:native.numericContract(),completed:result.reports.flatMap(r=>r.calculations).filter(c=>c.phase==='completed').length,timeout:'TIMEOUT',cancel:'ABORTED',recovery:'pass'}));
`);
const summary=run(['smoke.mjs'],consumer);
const hash=createHash('sha256').update(readFileSync(archive)).digest('hex');writeFileSync(join(output,'manifest.json'),JSON.stringify({archive,sha256:hash,consumer,packageBytes:pack.size,unpackedBytes:pack.unpackedSize,summary:JSON.parse(summary.trim().split('\n').at(-1))},null,2));console.log(summary.trim());
