import {readFileSync,writeFileSync,mkdirSync,existsSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {localRequest} from '../src/analyze/analysis-dsl/local-request.ts';
import {boardChannelPlan} from '../src/analyze/board-channel-plan.ts';
import {solveSegmentedChannel,validateChannelOwnership} from '../src/analyze/segmented-channel.ts';
import {createProblemExecutor} from '../src/analyze/execute-problem.ts';
import {localCoreIdentity,canonical} from '../src/analyze/local-model-cache.ts';
import {runAnalysisProblem,terminateAnalysisPool} from '../src/analyze/run-analysis.ts';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
process.env.P7_D1_PHASE??='joint';if(!['joint','joint-fine','segmented','shifted','warm-shifted','continuous-segmented','dsl'].includes(process.env.P7_D1_PHASE))throw Error('D1_PHASE: choose one bounded workflow');
process.env.P7_VERIFY_MEMORY_MIB??='1024';process.env.P7_VERIFY_SECONDS??='300';
const out=process.env.P7_OUTPUT_DIR??'debugging/p7-d1/'+Date.now();mkdirSync(out,{recursive:true});process.env.PCB_ANALYSIS_CACHE_DIR=process.env.P7_D1_CACHE_DIR??out+'/cache';
const frequencies=[1000,1700,Math.sqrt(1e7),6300,10000],variant=process.env.P7_D1_VARIANT??'slot',boardJson=readFileSync('tests/fixtures/analysis-p7-joint-section/'+variant+'.json','utf8'),board=JSON.parse(boardJson),code=readFileSync('tests/fixtures/analysis-p7-joint-section/channel.js','utf8').replace('maxSeconds:300,maxMemoryMiB:2048','maxSeconds:300,maxMemoryMiB:1024');let base:any,contacts:any;
const captured=await executeScenario({board,code:code.split('a.plot(')[0]},{filename:'d1-capture.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{base=localRequest(task as any,p=>contacts=p.contacts);return{id:'capture',status:'complete',method:'physical input capture only',diagnostics:[],assumptions:[],metrics:{}};});if(captured.status!=='complete')throw Error(JSON.stringify(captured.error));
const geometry={...base,roiMm:[[-.3,-.6],[2.3,.6]],maxTriangleAreaM2:5e-8,airMarginM:.002,electrodes:[...base.electrodes,{name:'victim-left',sourcePath:'pads[4]',layer:'TOP',upper:false},{name:'victim-right',sourcePath:'pads[5]',layer:'TOP',upper:false}],ports:[base.ports[0],['victim-left',base.ports[0][1]],base.ports[1],['victim-right',base.ports[1][1]]]};
const identity={phase:process.env.P7_D1_PHASE,nativeEnvironmentMemoryMiB:Number(process.env.P7_VERIFY_MEMORY_MIB),nativeEnvironmentSeconds:Number(process.env.P7_VERIFY_SECONDS),code,codeSha256:createHash('sha256').update(code).digest('hex'),variant,boardSha256:createHash('sha256').update(boardJson).digest('hex'),core:localCoreIdentity(),geometry,frequencies};writeFileSync(out+'/identity.json',JSON.stringify(identity,null,2));const execute=createProblemExecutor(true);
const network=(model:any,n=4)=>execute({kind:'frequency-network',request:{instances:[{id:'joint',model:{id:'joint',kind:'network',ports:Array.from({length:n},(_,i)=>'p'+i),data:model},ports:Array.from({length:n},(_,i)=>'p'+i)}],connections:[],ports:Array.from({length:n},(_,i)=>({name:['input','victimInput','output','victimOutput'][i],index:i})),frequencies,representation:'S',referenceOhm:Array(n).fill(50),budget:{maxSeconds:60,maxMemoryMiB:512}}});
try{
 if(!['segmented','shifted','warm-shifted','continuous-segmented','dsl'].includes(process.env.P7_D1_PHASE??''))for(const [label,level,air]of (process.env.P7_D1_PHASE==='joint-fine'?[['fine',2,.002]]:[['coarse',1,.002],['fine',2,.002],['domain',2,.004]]) as [string,1|2,number][]){
  const started=performance.now(),r=await runAnalysisProblem({kind:'local-extraction',request:{boardJson,geometry:{...geometry,airMarginM:air},magneticPatchLevel:level,remainingSeconds:180,frequenciesHz:frequencies,bandHz:[1000,100000],observablesOnly:true,drive:[1,0,-1,0],loadsOhm:[50,50,50,50]}},{timeoutMs:190000});
  const s=network(r.model);writeFileSync(out+'/joint-'+label+'.json',JSON.stringify({report:r,network:s,wallSeconds:(performance.now()-started)/1000}));if(s.status!=='complete')throw Error(JSON.stringify(s.diagnostics));
 }
 if(!['joint','joint-fine','dsl'].includes(process.env.P7_D1_PHASE??''))for(const boundaryFraction of (['shifted','warm-shifted'].includes(process.env.P7_D1_PHASE??'')?[1/6]:[1/8])){
  const plan=boardChannelPlan(board,{...base,maxTriangleAreaM2:5e-8,airMarginM:.002},contacts,{magneticPatchLevel:2,boundaryFraction});
  for(let end=0;end<2;end++){
   const original=plan.ports.find(p=>p.name===(end===0?'input':'output'))!;const b=plan.blocks.find(b=>b.kind==='local'&&b.cuts.some(c=>c.positive===original.cut.positive))!;if(b.kind!=='local')throw Error('Endpoint ownership');
   const g:any=b.geometry,index=b.cuts.findIndex(c=>c.positive===original.cut.positive),negative=g.ports[index][1],name='external-victim-'+end;
   g.electrodes.push({name,sourcePath:'pads['+(4+end)+']',layer:'TOP',upper:false});g.ports.push([name,negative]);const cut={positive:name,negative:original.cut.negative};b.cuts.push(cut);plan.ports.push({name: end===0?'victimInput':'victimOutput',cut});
  }
  plan.ports.sort((a,b)=>['input','victimInput','output','victimOutput'].indexOf(a.name)-['input','victimInput','output','victimOutput'].indexOf(b.name));validateChannelOwnership(plan);writeFileSync(out+'/plan-'+boundaryFraction+'.json',JSON.stringify(plan));
  if(process.env.P7_D1_PHASE==='warm-shifted'){
   const dir=process.env.PCB_ANALYSIS_CACHE_DIR!,source=dir+'/..',coldIdentity=JSON.parse(readFileSync(source+'/identity.json','utf8')),coldPlan=JSON.parse(readFileSync(source+'/plan-'+boundaryFraction+'.json','utf8')),cold=JSON.parse(readFileSync(source+'/segmented-'+boundaryFraction+'.json','utf8'));
   if(coldIdentity.core!==identity.core||canonical(coldPlan)!==canonical(plan))throw Error('WARM_CACHE_PHYSICAL_IDENTITY');
   for(const e of cold.evidence){const file=dir+'/'+e.cache.key+'.json';if(!existsSync(file))throw Error('WARM_CACHE_MISS: '+e.id);const envelope=JSON.parse(readFileSync(file,'utf8'));if(createHash('sha256').update(envelope.payload).digest('hex')!==envelope.checksum)throw Error('WARM_CACHE_CHECKSUM');}
  }
  const r=solveSegmentedChannel(plan,frequencies,{maxSeconds:process.env.P7_D1_PHASE==='warm-shifted'?30:180,maxMemoryMiB:1024});writeFileSync(out+'/segmented-'+boundaryFraction+'.json',JSON.stringify(r));
 }
 if(process.env.P7_D1_PHASE==='joint'){writeFileSync(out+'/comparison.json',JSON.stringify({status:'unsupported',reason:'joint diagnostics require magnetic/full-reference admission before physical acceptance'}));}else if(process.env.P7_D1_PHASE==='joint-fine'){const warm=await runAnalysisProblem({kind:'local-extraction',request:{boardJson,geometry,magneticPatchLevel:2,remainingSeconds:180,frequenciesHz:frequencies,bandHz:[1000,100000],observablesOnly:true,drive:[0,1,0,-1],loadsOhm:[50,50,50,50]}},{timeoutMs:190000});writeFileSync(out+'/joint-warm.json',JSON.stringify(warm));writeFileSync(out+'/comparison.json',JSON.stringify({status:'unsupported',reason:'joint diagnostic only; magnetic reference not admitted',cache:warm.cache}));}else if(['shifted','segmented','warm-shifted','continuous-segmented'].includes(process.env.P7_D1_PHASE??'')){writeFileSync(out+'/comparison.json',JSON.stringify({status:'unsupported',reason:'assembled diagnostic only; independent joint magnetic reference remains unadmitted'}));}else{const dsl=await runAnalysisDsl({board,code},{filename:'d1-real-channel.js',timeoutMs:310000,maxEntries:1000,maxOutputBytes:64*1024*1024});writeFileSync(out+'/dsl.json',JSON.stringify(dsl));
 writeFileSync(out+'/comparison.json',JSON.stringify({status:'pending-comparison',dslStatus:dsl.status}));}
}catch(e:any){writeFileSync(out+'/failure.json',JSON.stringify({message:e.message,cause:e.cause?.message}));throw e;}finally{await terminateAnalysisPool();}
