import{readFile,writeFile,copyFile,readdir,mkdir}from'node:fs/promises';
import{basename,join,resolve,relative}from'node:path';
import{fileURLToPath}from'node:url';
import{createHash}from'node:crypto';
import{createRequire}from'node:module';
import{execFileSync}from'node:child_process';
import{runAnalysisDsl}from'../../src/analyze/run-analysis-dsl.ts';
import{auditRlcReport,electricalCalculations,type RlcExpectations}from'./pcb-analysis-rlc-audit.ts';
import{auditDcReport,type DcExpectations}from'./pcb-analysis-dc-audit.ts';
import{renderRlcReport}from'./pcb-analysis-rlc-render.ts';
/** Exact authored source and current, explicitly identified board DC evidence.
 * Never select a previous successful run to repair missing report data. */
export async function runSavedAnalysisRlc(input:{script:URL;expectations:URL}){
 const fixture=process.env.PCB_ANALYZE_FIXTURE_DIR,output=process.env.PCB_ANALYZE_RUN_DIR;if(!fixture||!output)throw Error('Use test:pcb-analyze -- <fixture> --name rlc');
 const name=basename(fixture),boardBytes=await readFile(join(fixture,`${name}.json`),'utf8'),board=JSON.parse(boardBytes),code=await readFile(input.script,'utf8'),expected=JSON.parse(await readFile(input.expectations,'utf8'))as RlcExpectations;
 const hash=(v:string|Buffer)=>createHash('sha256').update(v).digest('hex');
 const {nativeFilename}=createRequire(import.meta.url)('../../native/node-addon/platform.cjs');const nativeSha256=hash(await readFile(join('native/node-addon',nativeFilename())));
 const dcEntry=JSON.parse(await readFile(join('debugging/pcb-analyze/dc',`${name}.json`),'utf8'));
 if(dcEntry.audit.status!=='pass')throw Error('Current DC evidence must pass its audit; run named dc first');
 const dcBytes=await readFile(join(dcEntry.output,'dc.json'),'utf8'),dc=JSON.parse(dcBytes);
 if(dc.metadata.sourceSha256!==hash(boardBytes.replaceAll('\r\n','\n')))throw Error('DC evidence board hash differs; run current dc fixture first');
 const dcImplementation=JSON.parse(await readFile(join(dcEntry.output,'implementation.json'),'utf8'))as Record<string,string>;
 // The RLC module and explicit-circuit adapter have no calls from board FEM DC. Rebuilding them
 // does not require cyclic unchanged FEM replays. Verify every shared/DC source
 // against the actually executed snapshot; retain both binary hashes explicitly.
 for(const[path,digest]of Object.entries(dcImplementation)){if(path.startsWith('native/analyze/src/electrical/')||path==='src/analyze/analysis-dsl/electrical-executor.ts')continue;if(hash((await readFile(path,'utf8')).replaceAll('\r\n','\n'))!==digest)throw Error(`DC shared/FEM implementation changed: ${path}; run current dc fixture first`);}
 auditDcReport(dc.result,JSON.parse(await readFile(join(fixture,'dc.expectations.json'),'utf8'))as DcExpectations);
 await copyFile(join(dcEntry.output,'dc.json'),join(output,'board-dc.json'));await copyFile(join(dcEntry.output,'index.html'),join(output,'board-dc.html'));
 // The DC HTML embeds its complete result; only its optional artifact links point
 // to the copied JSON / source below. No missing fields loaded from another run.
 const dcHtml=(await readFile(join(output,'board-dc.html'),'utf8')).replace('href="dc.json"','href="board-dc.json"').replace('href="scenario.js"','href="board-dc-scenario.js"');await writeFile(join(output,'board-dc.html'),dcHtml);await copyFile(join(dcEntry.output,'scenario.js'),join(output,'board-dc-scenario.js'));
 const implementation:Record<string,string>={};async function capture(dir:string){for(const ent of await readdir(dir,{withFileTypes:true})){const p=join(dir,ent.name);if(ent.isDirectory())await capture(p);else if(/\.(rs|ts)$/.test(p))implementation[p.replaceAll('\\','/')]=hash((await readFile(p,'utf8')).replaceAll('\r\n','\n'));}}
 for(const dir of ['src/analyze','native/analyze/src','native/node-addon/src'])await capture(dir);
 for(const p of ['native/analyze/Cargo.toml','native/analyze/Cargo.lock','native/node-addon/Cargo.toml','native/node-addon/Cargo.lock','tests/helpers/pcb-analysis-rlc.ts','tests/helpers/pcb-analysis-rlc-audit.ts','tests/helpers/pcb-analysis-rlc-render.ts'])implementation[p]=hash((await readFile(p,'utf8')).replaceAll('\r\n','\n'));
 await writeFile(join(output,'implementation.json'),JSON.stringify(implementation,null,2));await writeFile(join(output,'scenario.js'),code);
 const start=performance.now();const result=await runAnalysisDsl({board,code},{filename:fileURLToPath(input.script),timeoutMs:120000,maxOutputBytes:32*1024*1024,maxEntries:3000});
 const dcCases=dc.result.reports.flatMap((r:any)=>r.logs).filter((l:any)=>l.message==='dc.case').map((l:any)=>l.data);
 const metadata={fixture:name,source:'Explicit test hypotheses; no PCB AC extraction',head:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8',windowsHide:true}).trim(),scriptSha256:hash(code),boardSha256:hash(boardBytes.replaceAll('\r\n','\n')),nativeSha256,implementationSha256:hash(JSON.stringify(implementation)),expectations:expected,boardDc:{path:dcEntry.output,sha256:hash(dcBytes),nativeSha256:dc.metadata.version.nativeSha256,sharedAndFemSourceParityVerified:true,excludedIndependentModules:['native/analyze/src/electrical/','src/analyze/analysis-dsl/electrical-executor.ts'],audit:'pass',cases:dcCases},wallMs:performance.now()-start,peakWorkerRssBytes:Math.max(0,...electricalCalculations(result).map(r=>r.solution.resources?.peakRssBytes??0)),memoryMetric:'OS process high-water RSS through native call; excludes final report serialization after the last call',resolvedInputs:electricalCalculations(result).map(r=>({resultId:r.id,request:r.solution.request}))};
 await writeFile(join(output,'rlc.json'),JSON.stringify({metadata,result}));let audit:unknown;
 try{audit={status:'pass',...auditRlcReport(result,expected)};}catch(error){audit={status:'fail',reason:String(error)};process.exitCode=1;}
 await writeFile(join(output,'audit.json'),JSON.stringify(audit,null,2));await writeFile(join(output,'index.html'),renderRlcReport(name,{...metadata,audit},result));console.log(JSON.stringify({fixture:name,status:result.status,error:result.error,audit,wallMs:metadata.wallMs,peakRssBytes:metadata.peakWorkerRssBytes}));
 const overview=resolve('debugging/pcb-analyze/rlc');await mkdir(overview,{recursive:true});await writeFile(join(overview,`${name}.json`),JSON.stringify({fixture:name,output:resolve(output),metadata,audit}));
 const entries=await Promise.all((await readdir(overview)).filter(f=>f.endsWith('.json')).map(async f=>JSON.parse(await readFile(join(overview,f),'utf8'))));
 await writeFile(join(overview,'index.html'),`<!doctype html><html lang="ru"><meta charset="utf-8"><title>RLC P6a reports</title><style>body{font:18px system-ui;background:#132032;color:#e4edf7;margin:40px}a{color:#80d8ff}li{margin:20px}</style><h1>Заданная RLC-модель участка; частотное извлечение PCB не выполнялось</h1><p>Три платы: исходный PCB DC, явно заданная сеть, Z(f), transient, уточнение и continuation.</p><ul>${entries.map(e=>`<li><a href="${relative(overview,join(e.output,'index.html')).replaceAll('\\','/')}">${e.fixture}</a> — audit ${e.audit.status}; ${(e.metadata.wallMs/1000).toFixed(2)} s; peak worker RSS ${(e.metadata.peakWorkerRssBytes/1048576).toFixed(1)} MiB</li>`).join('')}</ul></html>`);
}
