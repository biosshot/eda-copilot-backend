import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import {verifyHash} from '../tests/helpers/p7-closure-reference.ts';
import {localCoreIdentity} from '../src/analyze/local-model-cache.ts';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';

const id=process.argv[2], dry=process.argv.includes('--dry-run');
const queue=JSON.parse(readFileSync('docs/analyze/reference/p7-extraction/p7-hybrid-final-queue.json','utf8'));
const c=queue.cases.find((item:any)=>item.id===id);
if(!c) throw Error('Select a board ID from the final queue');
verifyHash(c.board.path,c.board.sha256); verifyHash(c.dsl.path,c.dsl.sha256);
const code=readFileSync(c.dsl.path,'utf8');
const budgets=[...code.matchAll(/maxSeconds\s*:\s*(\d+)/g)].map(m=>Number(m[1]));
if(!budgets.length || budgets.some(n=>n<1 || n>300)) throw Error('DSL budget must be explicitly bounded at 300 seconds');
if(dry){console.log(JSON.stringify({id,board:c.board.path,dsl:c.dsl.path,budgets,physicalAccuracy:'unverified'}));}
else {
 const folder=process.env.P7_OUTPUT_DIR;
 if(!folder || !process.env.PCB_ANALYSIS_CACHE_DIR || process.env.P7_SUPERVISED_BOARD!=='1') throw Error('Run through verify-p7-closure-supervised.py --board-id with a supervised cache');
 mkdirSync(folder,{recursive:true});
 const identity={id,board:c.board,dsl:c.dsl,core:localCoreIdentity(),budgets,cacheDirectory:process.env.PCB_ANALYSIS_CACHE_DIR,physicalAccuracy:'unverified'};
 writeFileSync(folder+'/identity.json',JSON.stringify(identity,null,2));
 const result=await runAnalysisDsl({board:JSON.parse(readFileSync(c.board.path,'utf8')),code},{filename:c.dsl.path,timeoutMs:310000,maxEntries:1000,maxOutputBytes:128*1024*1024});
 writeFileSync(folder+'/dsl-result.json',JSON.stringify(result,null,2));
 writeFileSync(folder+'/board-result.json',JSON.stringify({status:result.status,physicalAccuracy:'unverified',error:result.error,resources:'Native diagnostics are retained in dsl-result.json; see supervision.json for process-tree measurements'},null,2));
 console.log(JSON.stringify({id,status:result.status,physicalAccuracy:'unverified'}));
 if(result.status!=='complete')process.exitCode=1;
}
