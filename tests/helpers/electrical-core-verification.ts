import {readFile,writeFile,mkdir,readdir} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {createHash} from 'node:crypto';
import {createRequire} from 'node:module';
import {execFileSync} from 'node:child_process';
import {runAnalysisDsl} from '../../src/analyze/run-analysis-dsl.ts';
import {auditElectricalCore} from './electrical-core-audit.ts';
const output=resolve(process.argv[2]??'debugging/p6-verification/direct-dsl');
const fixtures=process.argv.slice(3);if(!fixtures.length)fixtures.push('standalone','STM32G0_CAN_RS485','ThunderF722');
const script=await readFile('tests/fixtures/analysis-electrical-core/scenario.js','utf8');
const hash=(v:string|Buffer)=>createHash('sha256').update(v).digest('hex');
const {nativeFilename}=createRequire(import.meta.url)('../../native/node-addon/platform.cjs');
const implementation:Record<string,string>={};
async function capture(dir:string){for(const e of await readdir(dir,{withFileTypes:true})){const path=join(dir,e.name);if(e.isDirectory())await capture(path);else if(/\.(rs|ts)$/.test(e.name))implementation[path.replaceAll('\\','/')]=hash((await readFile(path,'utf8')).replaceAll('\r\n','\n'));}}
for(const dir of ['src/analyze','native/analyze/src','native/node-addon/src'])await capture(dir);
const nativeSha256=hash(await readFile(join('native/node-addon',nativeFilename())));
for(const name of fixtures){
 const dir=join(output,name);await mkdir(dir,{recursive:true});
 const bytes=name==='standalone'?undefined:await readFile(`tests/pcb-analyze/${name}/${name}.json`,'utf8');const board=bytes?JSON.parse(bytes):undefined;
 if(board&&(!board.components.some((c:any)=>c.designator==='R1')||board.pads.filter((p:any)=>p.component==='R1').length!==2||!board.pads.some((p:any)=>p.component==='U2')))throw Error('Frozen board binding R1/U2 is missing');
 const start=performance.now();const result=await runAnalysisDsl({code:script,...(board?{board}:{})},{filename:'electrical-core-'+name+'.js',timeoutMs:120000,maxOutputBytes:32*1024*1024,maxEntries:3000});
 const metadata={fixture:name,scriptSha256:hash(script),boardSha256:bytes?hash(bytes):null,nativeSha256,implementationSha256:hash(JSON.stringify(implementation)),head:execFileSync('git',['rev-parse','HEAD'],{encoding:'utf8',windowsHide:true}).trim(),wallMs:performance.now()-start,interactiveControlsVerified:false};
 let audit:unknown;try{audit=auditElectricalCore(result);}catch(error){audit={status:'fail',reason:String(error)};process.exitCode=1;}
 await writeFile(join(dir,'scenario.js'),script);await writeFile(join(dir,'implementation.json'),JSON.stringify(implementation,null,2));await writeFile(join(dir,'result.json'),JSON.stringify({metadata,result}));await writeFile(join(dir,'audit.json'),JSON.stringify(audit,null,2));
 console.log(JSON.stringify({metadata,audit}));
}
