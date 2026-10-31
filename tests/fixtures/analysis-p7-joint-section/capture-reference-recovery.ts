import {createHash} from 'node:crypto';
import {mkdirSync,readFileSync,writeFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {resolve} from 'node:path';

const source=resolve(process.argv[2]??'');
const output=resolve(process.argv[3]??'');
if(!process.argv[2]||!process.argv[3])throw Error('usage: capture-reference-recovery.ts <joint-report.json> <output.json>');
const prepared=JSON.parse(readFileSync(source,'utf8')).report.prepared;
const addonPath=resolve('native/node-addon/eda-copilot-node.win32-x64-msvc.node');
const addon=createRequire(import.meta.url)('../../../native/node-addon/index.cjs');
const session=new addon.AnalysisElectricalSession();
const response=session.solveLocal({prepared,frequenciesHz:[0,1000],drive:[1,0,-1,0],loadsOhm:[50,50,50,50]},30,false);
mkdirSync(resolve(output,'..'),{recursive:true});
writeFileSync(output,JSON.stringify({source,sourceSha256:createHash('sha256').update(readFileSync(source)).digest('hex'),addonSha256:createHash('sha256').update(readFileSync(addonPath)).digest('hex'),response},null,2));
console.log(JSON.stringify({output,source,addonSha256:createHash('sha256').update(readFileSync(addonPath)).digest('hex'),samples:response.samples.map((s:any)=>({frequencyHz:s.frequencyHz,portRealV:s.portRealV,portImaginaryV:s.portImaginaryV,totalLossW:s.totalLossW}))}));
