import {readFile,writeFile,readdir,mkdir} from 'node:fs/promises';
import {basename,join,resolve} from 'node:path';
import {fileURLToPath} from 'node:url';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {runAnalysisDsl} from '../../src/analyze/run-analysis-dsl.ts';
import {auditThermal,thermalResults} from './pcb-analysis-thermal-audit.ts';
import {thermalSvg,thermalHtml} from './pcb-analysis-thermal-render.ts';
export async function runSavedThermal(input:{script:URL;expectations:URL;timeoutMs?:number}){
 const timeoutMs=input.timeoutMs??600000;if(!Number.isSafeInteger(timeoutMs)||timeoutMs<1000||timeoutMs>1200000)throw Error('Thermal test timeoutMs must be an integer in 1000..1200000');
 const fixture=process.env.PCB_ANALYZE_FIXTURE_DIR,output=process.env.PCB_ANALYZE_RUN_DIR;if(!fixture||!output)throw Error('Use test:pcb-analyze -- <fixture> --name thermal');
 const name=basename(fixture),boardBytes=await readFile(join(fixture,`${name}.json`),'utf8'),board=JSON.parse(boardBytes),code=await readFile(input.script,'utf8'),expectedBytes=await readFile(input.expectations,'utf8'),expected=JSON.parse(expectedBytes);const hash=(v:string|Buffer)=>createHash('sha256').update(v).digest('hex');
 const implementation:Record<string,string>={};async function capture(dir:string){for(const e of await readdir(dir,{withFileTypes:true})){const p=join(dir,e.name);if(e.isDirectory())await capture(p);else if(/\.(rs|ts)$/.test(p))implementation[p.replaceAll('\\','/')]=hash((await readFile(p,'utf8')).replaceAll('\r\n','\n'));}}
 for(const dir of ['src/analyze','native/analyze/src','native/node-addon/src'])await capture(dir);
 for(const f of ['native/analyze/Cargo.toml','native/analyze/Cargo.lock','native/node-addon/Cargo.toml','native/node-addon/Cargo.lock','tests/helpers/pcb-analysis-thermal.ts','tests/helpers/pcb-analysis-thermal-audit.ts','tests/helpers/pcb-analysis-thermal-render.ts','tests/helpers/thermal-field-comparison.ts','tests/helpers/thermal-boundary-localization.ts','tests/helpers/thermal-nodal-balance.ts'])implementation[f]=hash((await readFile(f,'utf8')).replaceAll('\r\n','\n'));
 const {nativeFilename}=createRequire(import.meta.url)('../../native/node-addon/platform.cjs');const start=performance.now();const result=await runAnalysisDsl({board,code},{filename:fileURLToPath(input.script),timeoutMs,maxOutputBytes:128*1024*1024,maxEntries:1000});
 const resultJson=JSON.stringify(result);
 const metadata={fixture:name,scenarioTimeoutMs:timeoutMs,scenarioOutputCapBytes:128*1024*1024,resultBytes:Buffer.byteLength(resultJson),resultSha256:hash(resultJson),componentCadId:board.components.find((c:any)=>c.designator===expected.component)?.id??expected.component,boardSha256:hash(boardBytes),scriptSha256:hash(code),expectationsSha256:hash(expectedBytes),nativeSha256:hash(await readFile(join('native/node-addon',nativeFilename()))),implementationSha256:hash(JSON.stringify(implementation)),wallMs:performance.now()-start,peakWorkerRssBytes:Math.max(0,...thermalResults(result).map(r=>r.solution.resources?.peakRssBytes??0)),visualQa:'Static SVG/PNG directly from saved numerical arrays; HTML delivered separately; no browser screenshot'};
 let audit:unknown;try{audit=auditThermal(result,board,expected);}catch(error){audit={status:'fail',reason:String(error)};process.exitCode=1;}
 const svg=thermalSvg(result,expected,{...metadata,audit});const files={'scenario.js':code,'expectations.json':expectedBytes,'implementation.json':JSON.stringify(implementation,null,2),'thermal.json':`{"metadata":${JSON.stringify(metadata)},"result":${resultJson}}`,'audit.json':JSON.stringify(audit,null,2),'preview.svg':svg,'index.html':thermalHtml(svg,{...metadata,audit})};for(const[file,value]of Object.entries(files))await writeFile(join(output,file),value);
 const {createCanvas,loadImage}=await import('canvas');const png=createCanvas(1420,1160);png.getContext('2d').drawImage(await loadImage(Buffer.from(svg)),0,0);await writeFile(join(output,'preview.png'),png.toBuffer('image/png'));
 const overview=resolve('debugging/pcb-analyze/thermal');await mkdir(overview,{recursive:true});await writeFile(join(overview,`${name}.json`),JSON.stringify({output:resolve(output),metadata,audit}));console.log(JSON.stringify({...metadata,audit}));
}
