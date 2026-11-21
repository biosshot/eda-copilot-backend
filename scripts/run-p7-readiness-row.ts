// Runs one row of tests/pcb-analyze/p7-matrix.json and compares status/code with the recorded expectation.
// Heavy (up to an hour, ~4 GiB): run rows one at a time with RAYON_NUM_THREADS=4 P7_HMATRIX_THREADS=4.
// Usage: npx tsx scripts/run-p7-readiness-row.ts <row> [outDir]
import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import {fileURLToPath} from 'node:url';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import type {AnalysisTask} from '../src/analyze/analysis-dsl/executor.ts';
import {createPiBoardExecutor} from '../src/analyze/analysis-dsl/pi-board-executor.ts';
import {terminateAnalysisPool} from '../src/analyze/run-analysis.ts';

const fixtures=fileURLToPath(new URL('../tests/pcb-analyze/',import.meta.url));
const [id,outArg]=process.argv.slice(2);
const manifest=JSON.parse(readFileSync(join(fixtures,'p7-matrix.json'),'utf8'));
const row=manifest.rows.find((r:any)=>r.id===id);
if(!row)throw Error(`Unknown row ${id}; known: ${manifest.rows.map((r:any)=>r.id).join(', ')}`);
if(!row.scenario)throw Error(`${id} has no scenario: ${row.observed.note}`);
for(const [k,v] of Object.entries(row.env as Record<string,string>))process.env[k]=v;
const out=outArg??join('debugging','p7-matrix',id);
mkdirSync(out,{recursive:true});
const board=JSON.parse(readFileSync(join(fixtures,row.board,`${row.board}.json`),'utf8'));
const code=readFileSync(join(fixtures,row.scenario),'utf8').replace(/\r\n/g,'\n');
const start=performance.now();
let result:any;
try{
 result=row.pi
  ?await executeScenario({board,code},{filename:row.scenario,maxEntries:1000,maxOutputBytes:64*1024*1024},()=>{},async t=>createPiBoardExecutor()(t as AnalysisTask<'pi'>))
  :await executeScenario({board,code},{filename:row.scenario,maxEntries:1000,maxOutputBytes:128*1024*1024});
}catch(error){result={status:'thrown',error:{code:'THROWN',message:String(error)}};}
await terminateAnalysisPool();
const summary={row:id,wallSeconds:(performance.now()-start)/1000,status:result.status,code:result.error?.code,expected:row.expected};
const match=summary.status===row.expected.status&&(row.expected.code===undefined||summary.code===row.expected.code);
writeFileSync(join(out,'result.json'),JSON.stringify(result,null,2));
writeFileSync(join(out,'summary.json'),JSON.stringify({...summary,match},null,2));
console.log(JSON.stringify({...summary,match},null,2));
process.exitCode=match?0:1;
