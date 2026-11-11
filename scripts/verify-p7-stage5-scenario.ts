/** P7 v1 stage 5 scenario runner: PDN multiport, coupled pair, band steps (manifest tests/fixtures/analysis-p7-stage5).
 * `--prepare-only` plans the physical ports and blocks without any solve (cheap). A full run goes through the
 * production DSL executor; the process is expected to run under scripts/run-p7-matrix.py (supervisor, cache dir).
 * `--repeat N` repeats the DSL in the same process: pass 1 cold (or disk cache after a restart), pass 2+ in-process warm.
 * Physical accuracy stays `unverified`. */
import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {pathToFileURL} from 'node:url';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import type {AnalysisTask} from '../src/analyze/analysis-dsl/executor.ts';
import {localRequest,localChannelGroupRequest,type LocalExtractionPlan} from '../src/analyze/analysis-dsl/local-request.ts';
import {boardChannelPlan} from '../src/analyze/board-channel-plan.ts';
import {terminateAnalysisPool} from '../src/analyze/run-analysis.ts';

export type Stage5Scenario={id:string;dsl:string;route:string;bandsHz:number[];bandToken:string;expectedPorts:number};
const manifestPath=process.env.P7_STAGE5_MANIFEST??'tests/fixtures/analysis-p7-stage5/manifest.json';
export function stage5Manifest(){return JSON.parse(readFileSync(manifestPath,'utf8')) as {board:{path:string;sha256:string};scenarios:Stage5Scenario[]};}
/** Hash with CRLF folded, so a Windows autocrlf checkout equals the recorded LF hash. */
export const lfSha256=(path:string)=>createHash('sha256').update(readFileSync(path,'utf8').replace(/\r\n/g,'\n')).digest('hex');
export const scenarioSource=(scenario:Stage5Scenario,bandHz:number)=>{
 const text=readFileSync(scenario.dsl,'utf8').replace(/\r\n/g,'\n');
 if(!scenario.bandToken)return text;
 const pattern=new RegExp('/\\*'+scenario.bandToken+'\\*/[0-9.e+-]+');
 if(!pattern.test(text))throw Error('BAND_TOKEN_MISSING '+scenario.dsl);
 return text.replace(pattern,'/*'+scenario.bandToken+'*/'+bandHz);
};

function cacheStatuses(value:unknown,found:string[]=[],depth=0){
 if(!value||typeof value!=='object'||depth>8)return found;
 const v=value as Record<string,any>;
 if(v.cache&&typeof v.cache==='object'&&typeof v.cache.status==='string')found.push(v.cache.status);
 for(const child of Object.values(v))cacheStatuses(child,found,depth+1);
 return found;
}
function diagnosticCodes(value:unknown,codes=new Set<string>(),depth=0){
 if(!value||typeof value!=='object'||depth>10)return codes;
 const v=value as Record<string,any>;
 if(Array.isArray(v.diagnostics))for(const d of v.diagnostics)if(d&&typeof d.code==='string')codes.add(d.code);
 for(const child of Object.values(v))diagnosticCodes(child,codes,depth+1);
 return codes;
}

/** Plans the physical ports and blocks with the production hybrid planner options; no solve. */
export async function planScenario(board:any,scenario:Stage5Scenario,bandHz:number){
 let planned:any;
 const output=await executeScenario({board,code:scenarioSource(scenario,bandHz)},{filename:scenario.dsl,maxEntries:1000,maxOutputBytes:128*1024*1024},()=>{},async task=>{
  let prepared:LocalExtractionPlan|undefined;
  const geometry=task.kind==='si'?localChannelGroupRequest(task as AnalysisTask<'si'>,p=>{prepared=p;}):localRequest(task as AnalysisTask<'impedance'>,p=>{prepared=p;});
  const request=prepared?.terminalRequest??prepared?.contacts;
  // Same options as solveHybridBoardChannel (production hybrid entry), without any solve.
  const foil:any=boardChannelPlan(board,geometry,request as any,{closedJointDeadlineMs:Infinity,guardedReturn:true});
  planned={ports:foil.ports.map((p:any)=>p.name),terminals:foil.terminals?.length,roiMm:foil.roiMm,attachments:foil.attachments?.length??0,
   blocks:foil.blocks?.map((b:any)=>b.kind),signalIslands:foil.audit?.signalIslands?.length,returnSearch:Boolean(foil.returnSearch)};
  return {id:'plan',status:'complete',method:'Stage 5 geometry only',diagnostics:[],assumptions:[],metrics:{}};
 });
 return {planned,output};
}

async function main(){
 const [id,...rest]=process.argv.slice(2);
 const flag=(name:string)=>{const i=rest.indexOf(name);return i<0?undefined:rest[i+1];};
 const manifest=stage5Manifest();
 const scenario=manifest.scenarios.find(s=>s.id===id);
 if(!scenario)throw Error('Select one of: '+manifest.scenarios.map(s=>s.id).join(', '));
 const bandHz=Number(flag('--band-hz')??scenario.bandsHz[0]);
 if(!Number.isFinite(bandHz)||bandHz<=0)throw Error('--band-hz must be positive');
 const repeat=Number(flag('--repeat')??1);
 if(lfSha256(manifest.board.path)!==manifest.board.sha256)throw Error('HASH_MISMATCH '+manifest.board.path);
 const board=JSON.parse(readFileSync(manifest.board.path,'utf8'));
 const code=scenarioSource(scenario,bandHz);
 const options={filename:scenario.dsl,maxEntries:1000,maxOutputBytes:128*1024*1024};
 const folder=process.env.P7_OUTPUT_DIR;
 if(folder)mkdirSync(folder,{recursive:true});
 const emit=(name:string,body:unknown)=>{if(folder)writeFileSync(folder+'/'+name,JSON.stringify(body,null,2));console.log(JSON.stringify(body));};
 if(process.argv.includes('--prepare-only')){
  const {planned,output}=await planScenario(board,scenario,bandHz);
  emit('prepare.json',{id,bandHz,status:planned?'planned':'refused',expectedPorts:scenario.expectedPorts,planned,
   error:output.error&&{code:output.error.code,message:output.error.message,diagnostics:output.error.diagnostics.map(d=>d.code)}});
  process.exitCode=planned?.ports?.length===scenario.expectedPorts?0:1; // the plot of the stub result is invalid by design; only the plan counts
  return;
 }
 if(!folder||!process.env.PCB_ANALYSIS_CACHE_DIR)throw Error('Run through scripts/run-p7-matrix.py (supervised cache directory)');
 const passes:any[]=[];
 let last:any;
 for(let pass=1;pass<=repeat;pass++){
  const start=performance.now();
  const result=await executeScenario({board,code},options);
  const calculations=result.reports.flatMap(report=>report.calculations).filter(c=>c.phase==='completed') as any[];
  const solutions=calculations.map(c=>c.result?.solution).filter(Boolean);
  passes.push({pass,wallSeconds:(performance.now()-start)/1000,dslStatus:result.status,cache:cacheStatuses(solutions),
   solutionStatuses:solutions.map(s=>s.status??(s.network?'network':undefined)),diagnosticCodes:[...diagnosticCodes(calculations)],
   error:result.error&&{code:result.error.code,message:result.error.message,diagnostics:result.error.diagnostics.map(d=>({code:d.code,message:d.message}))}});
  last=result;
  if(result.status!=='complete')break;
 }
 await terminateAnalysisPool();
 writeFileSync(folder+'/dsl-result.json',JSON.stringify(last,null,2));
 const calculations=(last.reports.flatMap((r:any)=>r.calculations) as any[]).filter(c=>c.phase==='completed');
 const solved=calculations.map(c=>c.result?.solution).filter(Boolean);
 const network=solved.map(s=>s.network??s.evidence?.network).find(Boolean);
 const model=last.status==='complete'&&solved.length>0;
 emit('scenario-result.json',{id,bandHz,status:model?'model':'refused',physicalAccuracy:'unverified',
  errorCode:last.error?.code,errorMessage:last.error?.message,ports:network?.ports??solved[0]?.network?.ports,
  frequenciesHz:solved[0]?.frequenciesHz??solved[0]?.evidence?.frequenciesHz,passes});
 if(!model)process.exitCode=1;
}
if(import.meta.url===pathToFileURL(process.argv[1]).href)main().catch(error=>{console.error(error);process.exitCode=1;});
