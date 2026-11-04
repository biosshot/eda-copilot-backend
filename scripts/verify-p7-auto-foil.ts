import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdirSync,writeFileSync} from 'node:fs';
import {join} from 'node:path';
import type {AnalysisBoardFoilCheckedReport} from '../native/analyze/index.js';
import {createAnalysisExecutor,type ExecutionResult} from '../src/analyze/analysis-dsl/executor.ts';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import type {PhysicalPortSampleData} from '../src/analyze/analysis-dsl/spec-doc.js';
import {localCoreIdentity} from '../src/analyze/local-model-cache.ts';
import {finitePlaneBend,finitePlaneBendDsl} from '../tests/helpers/pcb-analysis-finite-plane-bend.ts';
import {finiteFoilWitnessControl} from '../tests/helpers/pcb-analysis-finite-foil-witness.ts';

const folder=process.env.P7_OUTPUT_DIR;
if(!folder)throw Error('Set P7_OUTPUT_DIR by running this smoke under the process-tree supervisor');
mkdirSync(folder,{recursive:true});
const file=(name:string)=>join(folder,name);
const write=(name:string,data:unknown)=>writeFileSync(file(name),JSON.stringify(data,null,2)+'\n');
const sha=(data:string)=>createHash('sha256').update(data).digest('hex');
const shaPattern=/^[a-f0-9]{64}$/;
const budget={maxSeconds:1800,maxMemoryMiB:3072};
process.env.P7_VERIFY_SECONDS=String(budget.maxSeconds);
process.env.P7_VERIFY_MEMORY_MIB=String(budget.maxMemoryMiB);
const coupon=process.env.P7_AUTO_FOIL_COUPON??'finite-plane-bend';
assert.ok(coupon==='finite-plane-bend'||coupon==='finite-foil-witness','Unknown foil smoke coupon '+coupon);
const witness=coupon==='finite-foil-witness';
const board=witness?finiteFoilWitnessControl():finitePlaneBend();
const source=finitePlaneBendDsl.replace('maxSeconds:300,maxMemoryMiB:2048',
 'maxSeconds:'+budget.maxSeconds+',maxMemoryMiB:'+budget.maxMemoryMiB)
 +"\nawait a.networkParameters().network(result.network).representation('S').referenceOhm(50).frequencyHz(1000).run();\n";
assert.notEqual(source.indexOf('maxSeconds:1800,maxMemoryMiB:3072'),-1);
const boardJson=JSON.stringify(board);
const start=performance.now();
const config={coupon,budget,treePrivateCapMiB:4096,frequenciesHz:[1000,3000],replayFrequencyHz:1000,
 referenceOhm:[50,50],boardSha256:sha(boardJson),sourceSha256:sha(source),
 node:process.version,magneticProfile:process.env.P7_MAGNETIC_PROFILE??'baseline',
 magneticPrecision:process.env.P7_MAGNETIC_PRECISION??'default',cache:process.env.PCB_ANALYSIS_CACHE_DIR};
writeFileSync(file('board.json'),boardJson);
writeFileSync(file('source.js'),source);
write('config.json',config);
const payloads:ExecutionResult[]=[];
let executorError:unknown;
let solverBoardSha256:string|undefined;
try{
 write('config.json',{...config,coreIdentity:localCoreIdentity()});
 const execute=createAnalysisExecutor();
 const scenario=await executeScenario({board,code:source},{filename:'automatic-foil-smoke.js',maxEntries:1000,maxOutputBytes:32*1024*1024},()=>{},
  async task=>{
   if(task.kind==='impedance'&&task.board){
    const exact=JSON.stringify(task.board);
    solverBoardSha256=sha(exact);
    writeFileSync(file('solver-board.json'),exact);
   }
   try{
    const result=await execute(task);
    payloads.push(result);
    write('artifact.json',payloads);
    return result;
   }catch(error){
    executorError=error instanceof Error?{name:error.name,message:error.message,stack:error.stack}:String(error);
    throw error;
   }
  });
 write('report.json',scenario);
 assert.equal(scenario.status,'complete',JSON.stringify(scenario.error));
 assert.equal(payloads.length,2);
 const [producer,replay]=payloads;
 assert.equal(producer.status,'partial');
 assert.ok(producer.diagnostics.some(d=>d.code==='CHANNEL_FOIL_PHYSICAL_UNVERIFIED'));
 assert.equal(replay.status,'complete');
 assert.deepEqual(producer.network?.ports,['input','output']);
 assert.deepEqual(replay.network?.ports,producer.network?.ports);
 const extracted=producer.network?.data as PhysicalPortSampleData;
 const replayed=replay.network?.data as PhysicalPortSampleData;
 const expectedPairs=[
  {positive:{sourcePath:'pads[0]',layer:'TOP'},negative:{sourcePath:'pads[2]',layer:'BOTTOM'}},
  {positive:{sourcePath:'pads[1]',layer:'TOP'},negative:{sourcePath:'pads[3]',layer:'BOTTOM'}},
 ];
 for(const resource of [extracted,replayed]){
  assert.equal(resource.format,'physical-port-samples');
  assert.equal(resource.samplePolicy,'exact');
  assert.equal(resource.representation,'S');
  assert.equal(resource.referenceTopology,'physical-pairs');
  assert.deepEqual(resource.physicalPorts,expectedPairs);
  assert.deepEqual(resource.referenceOhm,config.referenceOhm);
 }
 assert.deepEqual(extracted.frequenciesHz,config.frequenciesHz);
 assert.deepEqual(replayed.frequenciesHz,[config.replayFrequencyHz]);
 const sample=extracted.parameters[0];
 assert.equal(replayed.parameters[0].length,2);
 for(let out=0;out<2;out++)for(let input=0;input<2;input++){
  const expected=sample[out][input],actual=replayed.parameters[0][out][input];
  assert.ok(Math.hypot(actual.real-expected.real,actual.imaginary-expected.imaginary)<=1e-12,'Exact-grid P6 replay changed S');
 }
 const solution=producer.solution as unknown as AnalysisBoardFoilCheckedReport & {
  foilPlan:{producer:{boardSha256:string;geometrySha256:string};physicalPorts:unknown};
 };
 const producerDomain=producer.network?.validity?.domain as Record<string,unknown>;
 const replayDomain=replay.network?.validity?.domain as Record<string,unknown>;
 assert.equal(producerDomain.bandHz,undefined);
 assert.equal(producerDomain.spatialCoverage,'unverified');
 assert.equal(producerDomain.bandCoverage,'unverified');
 assert.equal(producerDomain.domainCoverage,'unverified');
 assert.equal(producerDomain.energyVerification,'unverified');
 const identity=solution.foilPlan.producer;
 assert.equal(identity.boardSha256,solverBoardSha256);
 assert.match(identity.geometrySha256,shaPattern);
 assert.deepEqual(producerDomain.producer,identity);
 assert.deepEqual(replayDomain.producer,identity);
 assert.equal(replayDomain.sourceValiditySha256,sha(JSON.stringify(producer.network?.validity)));
 assert.equal(replayDomain.sourceValidity,undefined);
 const replaySolution=replay.solution as {topology:unknown;sourceHashes:{resourceId:string;sha256:string}[];validityLimitations?:unknown};
 assert.deepEqual(replaySolution.topology,{referenceTopology:'physical-pairs',connections:[]});
 assert.equal(replaySolution.validityLimitations,undefined);
 assert.equal(replaySolution.sourceHashes.length,1);
 assert.equal(replaySolution.sourceHashes[0].resourceId,producer.network?.id);
 assert.match(replaySolution.sourceHashes[0].sha256,shaPattern);
 assert.equal(solution.coverage,'unverified');
 const returnSource=witness?'zones[1]':'zones[0]';
 assert.ok(solution.provenance.sourceMapping.some(([,path,,layer])=>layer==='BOTTOM'&&path.includes(returnSource)));
 assert.deepEqual(solution.foilPlan.physicalPorts,expectedPairs);
 assert.equal(solution.samples.length,2);
 for(const [i,sample] of solution.samples.entries()){
  assert.equal(sample.frequencyHz,config.frequenciesHz[i]);
  assert.equal(sample.columns.length,2);
  for(const [j,column] of sample.columns.entries()){
   assert.equal(column.input,j);
   assert.ok(column.kclRelative>=0&&column.kclRelative<=1e-8);
   assert.ok(column.kvlRelative>=0&&column.kvlRelative<=1e-8);
   const outgoing=extracted.parameters[i].reduce((sum,row)=>sum+row[j].real**2+row[j].imaginary**2,0);
   const net=config.referenceOhm[j]/4*(1-outgoing);
   const limit=1e-12+1e-8*(Math.abs(net)+column.copperLossW);
   assert.ok(column.copperLossW>=0&&column.powerBalanceErrorW>=0&&column.powerBalanceErrorW<=limit);
   assert.ok(Math.abs(net-column.copperLossW)<=limit);
  }
 }
 write('error.json',null);
 write('comparison.json',{status:'pass',coupon,elapsedSeconds:(performance.now()-start)/1000,
  extractionStatus:producer.status,replayStatus:replay.status,producer:identity,quality:solution.quality,
  plannedBytes:solution.provenance.plannedBytes,prisms:solution.provenance.prisms,physicalCoverage:'unverified'});
}catch(error){
 const detail=error instanceof Error?{name:error.name,message:error.message,stack:error.stack}:String(error);
 write('error.json',{error:detail,executorError});
 write('comparison.json',{status:'validation-failure',coupon,elapsedSeconds:(performance.now()-start)/1000,error:detail});
 console.error(error);
 process.exitCode=1;
}
