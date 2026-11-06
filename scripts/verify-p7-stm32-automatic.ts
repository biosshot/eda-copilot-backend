/** Run from the backend root. Default captures geometry and the first actual
 * automatic producer request. --solve uses the same saved DSL under supervision. */
import assert from 'node:assert/strict';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {resolve,join} from 'node:path';
import {pathToFileURL} from 'node:url';
const moduleAt=(path:string)=>import(pathToFileURL(resolve(path)).href);
const {executeScenario}=await moduleAt('src/analyze/analysis-dsl/scenario.ts');
const {localRequest}=await moduleAt('src/analyze/analysis-dsl/local-request.ts');
const {createBoardChannelExecutor}=await moduleAt('src/analyze/board-channel-executor.ts');
const {runAutoFoil}=await moduleAt('src/analyze/auto-foil-response.ts');
const {capturedBoardGeometry}=await moduleAt('src/analyze/geometry-snapshot.ts');
const {localCoreIdentity,LocalModelCache}=await moduleAt('src/analyze/local-model-cache.ts');
const {cachedBoardFoilReport}=await moduleAt('src/analyze/board-foil-cache.ts');
const out=process.env.P7_OUTPUT_DIR;
assert.ok(out,'Set P7_OUTPUT_DIR to a dedicated evidence folder');
mkdirSync(out,{recursive:true});
const save=(name:string,value:unknown)=>writeFileSync(join(out,name),JSON.stringify(value,null,2)+'\n');
const sha=(text:string)=>createHash('sha256').update(text).digest('hex');
const manifest=JSON.parse(readFileSync('tests/fixtures/analysis-p7-closure/manifest.json','utf8'));
const entry=manifest.cases.find((c:any)=>c.id==='STM32G0_CAN_RS485');
assert.ok(entry);
const sourcePath='tests/pcb-analyze/STM32G0_CAN_RS485/local-extraction-complete-channel.js';
const boardText=readFileSync(entry.board,'utf8'),original=readFileSync(sourcePath,'utf8');
assert.equal(sha(boardText),entry.boardSha256);
const budget={maxSeconds:Number(process.env.P7_VERIFY_SECONDS??1800),maxMemoryMiB:Number(process.env.P7_VERIFY_MEMORY_MIB??3072)};
assert.ok(Number.isFinite(budget.maxSeconds)&&budget.maxSeconds>0&&budget.maxSeconds<=1800,'P7_VERIFY_SECONDS must be in (0,1800]');
assert.ok(Number.isFinite(budget.maxMemoryMiB)&&budget.maxMemoryMiB>=256&&budget.maxMemoryMiB<=65536,'P7_VERIFY_MEMORY_MIB must be in [256,65536]');
const treePrivateCapMiB=Number(process.env.P7_TREE_CAP_MIB??4096);
assert.ok(Number.isFinite(treePrivateCapMiB)&&treePrivateCapMiB>=budget.maxMemoryMiB&&treePrivateCapMiB<=65536,'P7_TREE_CAP_MIB must cover native memory and be <=65536; enforce this cap with the supervisor');
const maxIterations=process.env.P7_VERIFY_MAX_ITERATIONS===undefined?undefined:Number(process.env.P7_VERIFY_MAX_ITERATIONS);
assert.ok(maxIterations===undefined||(Number.isInteger(maxIterations)&&maxIterations>=1&&maxIterations<=16),'P7_VERIFY_MAX_ITERATIONS must be in 1..16');
process.env.P7_VERIFY_SECONDS=String(budget.maxSeconds);
process.env.P7_VERIFY_MEMORY_MIB=String(budget.maxMemoryMiB);
const marker='maxSeconds:300,maxMemoryMiB:2048';
assert.equal(original.split(marker).length,2);
const code=original.replace(marker,`maxSeconds:${budget.maxSeconds},maxMemoryMiB:${budget.maxMemoryMiB}`);
writeFileSync(join(out,'source.js'),code);
writeFileSync(join(out,'board.json'),boardText);
const core=localCoreIdentity();
// Record effective experiment knobs alongside input hashes; absent values use native defaults.
const numericalEnvironment=Object.fromEntries([
 'P7_MAGNETIC_AXIAL_TETS','P7_MAGNETIC_NEAR_THREADS','P7_MAGNETIC_BATCH_COLUMNS',
 'P7_MAGNETIC_NEAR_BOUND','P7_MAGNETIC_PAIR_CACHE_MIB','P7_MAGNETIC_COVER_CACHE_MIB',
 'P7_MAGNETIC_TRACE','P7_MAGNETIC_PROFILE','P7_MAGNETIC_SLOW_PAIR_PATH','RAYON_NUM_THREADS','PCB_ANALYSIS_TRAINING_JOBS',
].map(key=>[key,process.env[key]??null]));
const identity={board:entry.board,dsl:sourcePath,boardSha256:sha(boardText),originalDslSha256:sha(original),
 executedDslSha256:sha(code),coreIdentity:core,budget,treePrivateCapMiB,maxIterations:maxIterations??16,numericalEnvironment,
 assumptions:entry.missingFields,physicalCoverage:'unverified'};
save('identity.json',identity);
let request:any,segmented:any,extraction:any,baseGeometry:any,taskBoardJson:string|undefined;
const captured=new Error('P7_CAPTURE_BEFORE_NUMERICS');
const solve=createBoardChannelExecutor(undefined,(plan:any)=>{
 segmented=plan;throw captured;
},(plan:any,axis:number[],limits:any,interval:any)=>runAutoFoil(plan,axis,limits,interval,(problem:any)=>{
 request=problem;throw captured;
}));
const started=performance.now();
const scenario=await executeScenario({board:JSON.parse(boardText),code},
 {filename:sourcePath,maxEntries:1000,maxOutputBytes:64*1024*1024},()=>{},async(task:any)=>{
  assert.equal(task.kind,'impedance');
  taskBoardJson=JSON.stringify(task.board);
  baseGeometry=localRequest(task,(plan:any)=>{extraction=plan;});
  assert.equal(task.config.frequencyHz,10000);
  const axis=[task.config.frequencyHz];
  try{
   solve(task.board,baseGeometry,extraction.terminalRequest??extraction.contacts,axis,budget,[axis[0],axis[0]]);
  }catch(error){if(error!==captured)throw error;}
  return {id:'preflight',status:'partial',method:'Actual geometry planning and producer capture',
   diagnostics:[],assumptions:[],metrics:{},solution:{preflight:true}};
 });
save('planning-scenario.json',scenario);
save('extraction-plan.json',extraction??null);
if(taskBoardJson)writeFileSync(join(out,'solver-board.json'),taskBoardJson);
if(segmented)save('segmented-plan.json',segmented);
if(request)save('automatic-request.json',request);
const geometry=taskBoardJson?capturedBoardGeometry(taskBoardJson):undefined;
if(geometry)save('geometry.json',geometry);
const selected=new Set(request?.expected.sourceCopper.map((r:any)=>r.regionId));
const ownership=geometry&&{sourceCopper:request?.expected.sourceCopper,sourceBarrels:request?.expected.sourceBarrels,
 islands:geometry.islands.filter((island:any)=>island.region_ids.some((id:string)=>selected.has(id))),
 contacts:extraction?.terminalRequest??extraction?.contacts,
 diagnosticDecisions:extraction?.diagnosticDecisions,
 excluded:geometry.excluded,findings:geometry.findings,fills:geometry.fills};
if(request&&geometry)for(const island of ownership.islands){
 assert.ok(island.region_ids.every((id:string)=>selected.has(id)),'Physical island was clipped');
}
save('ownership.json',ownership??null);
const preflight:any={status:scenario.status==='complete'&&request?'pass':'unsupported',
 phase:'geometry planning only',numericalSolves:0,route:request?'automatic joint':segmented?'segmented':'refused',
 error:scenario.error,elapsedSeconds:(performance.now()-started)/1000,
 resources:{budget,treePrivateCapMiB,meshAndFactorAdmission:'unverified',
  reason:'Actual constrained mesh counts and common field/factor plans are checked inside the bounded native producer.',
  baselineRssBytes:process.memoryUsage().rss},
 roiMm:request?.geometry.roiMm,sourceCopperCount:request?.expected.sourceCopper.length,
 capturedCopperCount:geometry?.regions.length,capturedIslandCount:geometry?.islands.length,
 retainedIslandCount:ownership?.islands.length,
 sourceBarrelCount:request?.expected.sourceBarrels.length,partitionPolicy:request?.request.partitionPolicy,
 physicalCoverage:'unverified',missingPhysicalInputs:entry.missingFields};
if(request){
 assert.equal(request.request.partitionPolicy,'automatic-conductive-schur-v1');
 assert.equal(request.geometry.allowInsulatedCopperCuts,false);
 assert.deepEqual(request.geometry.cutPlanes,{});
 assert.equal(request.expected.electrodePairs.length,2);
 const electrodes=new Map(request.geometry.electrodes.map((e:any)=>[e.name,e]));
 preflight.physicalPorts=request.expected.electrodePairs.map((pair:string[])=>pair.map(id=>electrodes.get(id)));
 preflight.solverBoardSha256=sha(request.boardJson);
 try{
  const cached=cachedBoardFoilReport(new LocalModelCache(),core,request,()=>{throw captured;});
  preflight.compatibleCache=cached.cache;
 }catch(error){
  preflight.compatibleCache=error===captured?{status:'miss',nativeSolves:0}:
   {status:'refused',message:String(error)};
 }
}
save('preflight.json',preflight);
save('comparison.json',preflight);
console.log(JSON.stringify(preflight));
if(process.argv.includes('--solve')){
 try{
 assert.equal(preflight.status,'pass','Automatic joint request must pass planning first');
 const {createAnalysisExecutor}=await moduleAt('src/analyze/analysis-dsl/executor.ts');
 let execute=createAnalysisExecutor();
 if(maxIterations!==undefined){
  const {createLocalExecutor}=await moduleAt('src/analyze/analysis-dsl/local-executor.ts');
  const {createProblemExecutor}=await moduleAt('src/analyze/execute-problem.ts');
  const native=createProblemExecutor(true);
  execute=createLocalExecutor((problem:any)=>{
   if(problem.kind!=='board-foil-checked')return native(problem);
   const bounded={...problem,request:{...problem.request,maxIterations}};
   save('executed-native-request.json',bounded);
   try{return native(bounded);}catch(error){
    save('native-error.json',{message:String(error),stack:error instanceof Error?error.stack:undefined});
    throw error;
   }
  });
 }
 const artifacts:any[]=[];
 const result=await executeScenario({board:JSON.parse(boardText),code},
  {filename:sourcePath,maxEntries:1000,maxOutputBytes:64*1024*1024},()=>{},async(task:any)=>{
   const artifact=await execute(task);
   artifacts.push(artifact);
   save('artifacts.json',artifacts);
   return artifact;
  });
 save('result.json',result);
 if(result.status==='complete'){
  assert.equal(artifacts.length,1);
  const report=artifacts[0].solution;
  assert.equal(report.provenance.partitionPolicy,request.request.partitionPolicy);
  assert.equal(report.provenance.automaticPartition.status,'selected');
  assert.ok(report.conductiveCorrection.maxOriginalFreeResidualRelative<=1e-8);
  assert.equal(report.coverage,'unverified');
  assert.deepEqual(report.frequenciesHz,request.request.frequenciesHz);
  assert.equal(report.foilPlan.producer.boardSha256,sha(request.boardJson));
  save('resources.json',{cache:report.cache,provenance:report.provenance,
   automaticPartition:report.provenance.automaticPartition,conductiveCorrection:report.conductiveCorrection});
 }
 const comparison={status:result.status==='complete'?'pass':'validation-failure',phase:'automatic extraction',
  error:result.error,physicalCoverage:'unverified',identity};
 save('comparison.json',comparison);
 console.log(JSON.stringify(comparison));
 process.exitCode=result.status==='complete'?0:1;
 }catch(error){
  const detail={message:String(error),stack:error instanceof Error?error.stack:undefined};
  save('error.json',detail);
  save('comparison.json',{status:'validation-failure',phase:'automatic extraction',error:detail,identity});
  process.exitCode=1;
 }
}else process.exitCode=preflight.status==='pass'?0:1;
