/** Developer-only timing of one unadmitted real-board discretization, not channel acceptance. */
import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import {runAnalysisProblem,terminateAnalysisPool} from '../src/analyze/run-analysis.ts';
import {localCoreIdentity} from '../src/analyze/local-model-cache.ts';
const jobs=Number(process.argv[2]??1);if(![1,2,4].includes(jobs))throw Error('jobs1/2/4 required');
const directory='debugging/p7-channel-plan/training-jobs-'+jobs+'-'+Date.now();mkdirSync(directory,{recursive:true});process.env.PCB_ANALYSIS_TRAINING_JOBS=String(jobs);process.env.PCB_ANALYSIS_CACHE_DIR=directory+'/cache';
const boardJson=JSON.stringify(JSON.parse(readFileSync('tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json','utf8')));
const geometry=JSON.parse(readFileSync('debugging/p7-channel-plan/automatic-request.json','utf8'));
const request={boardJson,geometry,magneticPatchLevel:1 as const,bandHz:[1000,100000] as [number,number],frequenciesHz:[1000,10000,Math.sqrt(1e9),100000],observablesOnly:true,remainingSeconds:300};
const runs:any[]=[];writeFileSync(directory+'/input.json',JSON.stringify({coreIdentity:localCoreIdentity(),jobs,request,scope:'One fixed real-board direct-FEM candidate; high-level automatic planning and spatial/environment admission are not included. No certified channel result or 5% accuracy claim.'}));
try{for(const mode of ['cold','warm','restart']){if(mode==='restart')await terminateAnalysisPool();const started=performance.now();const report=await runAnalysisProblem({kind:'local-extraction',request},{timeoutMs:300000});writeFileSync(directory+'/'+mode+'.json',JSON.stringify(report));runs.push({mode,wallSeconds:(performance.now()-started)/1000,cache:report.cache,resources:report.resources,stageSeconds:report.prepared.stageSeconds,samples:report.samples});console.log(JSON.stringify({directory,...runs.at(-1)}));}assert.deepEqual(runs.map(r=>r.cache.status),['cold','memory','disk']);assert.deepEqual(runs[0].samples,runs[1].samples);assert.deepEqual(runs[0].samples,runs[2].samples);writeFileSync(directory+'/evidence.json',JSON.stringify({coreIdentity:localCoreIdentity(),jobs,runs},null,2));}finally{await terminateAnalysisPool();}
