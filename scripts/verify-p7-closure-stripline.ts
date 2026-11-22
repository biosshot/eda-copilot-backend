import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';import{createHash}from'node:crypto';
import{compareValue,validateControl,validateDslCalculation}from'../tests/helpers/p7-closure-reference.ts';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
const line={kind:'coupled-stripline',widthMm:.3,gapMm:.3,copperThicknessMm:.035,topDielectricHeightMm:.3,bottomDielectricHeightMm:.3,lengthMm:20,topMaterial:{relativePermittivity:4,lossTangent:0,state:'Explicit homogeneous analytic TEM benchmark'},bottomMaterial:{relativePermittivity:4,lossTangent:0,state:'Explicit homogeneous analytic TEM benchmark'}};
const frequencies=[1e6,1e7,1e8];const start=performance.now();
const packetFolder=process.env.P7_OUTPUT_DIR??'debugging/p7-closure';mkdirSync(packetFolder,{recursive:true});
const reused=process.argv.includes('--reuse');
const sourcePath=process.env.P7_RESULT_FILE??'debugging/p7-closure/stripline-native.json';
const dslPath='tests/fixtures/analysis-p7-closure/coupled-stripline.js';
const hash=(p:string)=>createHash('sha256').update(readFileSync(p)).digest('hex');
const provenance=reused?{sourcePath,sourceSha256:hash(sourcePath),sourceAddonSha256:null,coldSolve:false}:{addonSha256:hash('native/node-addon/eda-copilot-node.win32-x64-msvc.node'),dslSha256:hash(dslPath),workerSha256:hash('dist/run-analysis-dsl.worker.js'),coldSolve:!process.env.P7_VALIDATION_REPORT,validationOnly:!!process.env.P7_VALIDATION_REPORT};
let result:any;
try {
 if(process.env.P7_VALIDATION_REPORT){const calculation=JSON.parse(readFileSync(process.env.P7_VALIDATION_REPORT,'utf8'));validateDslCalculation(calculation);result=calculation.solution;}
 else if(reused) result=JSON.parse(readFileSync(sourcePath,'utf8')).result;
 else {
  const dsl=await runAnalysisDsl({code:readFileSync(dslPath,'utf8')},{filename:dslPath,timeoutMs:120000,maxOutputBytes:128*1024*1024});
  writeFileSync(packetFolder+'/stripline-dsl-report.json',JSON.stringify(dsl));
  if(dsl.status!=='complete') throw Error(JSON.stringify(dsl.error));
  const calculation:any=dsl.reports.flatMap(r=>r.calculations).map((c:any)=>c.result).find((c:any)=>c?.solution?.parameters);
  if(!calculation) throw Error('MISSING_EVIDENCE calculation.solution');
  validateDslCalculation(calculation);
  result=calculation.solution;
 }
validateControl(result,frequencies,['positiveInput','negativeInput','positiveOutput','negativeOutput']);
if(!reused)writeFileSync(packetFolder+'/stripline-native-replay.json',JSON.stringify({wallSeconds:(performance.now()-start)/1000,provenance,line,frequencies,result}));
const analyticDelay=2/299792458,comparisons=result.parameters.modes.map((m:any)=>({mode:m.name,metric:'homogeneous TEM delay s/m',...compareValue(m.delaySPerM,analyticDelay,1e-13)}));
// Both even/odd waves have the same physical velocity in a homogeneous dielectric.
// Recover their transmission phases from the full numerical physical-port S matrix,
// then compare the ABCD A term against cos(omega * independent TEM flight time).
for(let f=0;f<frequencies.length;f++)for(const sign of [1,-1]){
 const m=result.network.parameters[f],add=(a:any,b:any,k:number)=>({real:a.real+k*b.real,imaginary:a.imaginary+k*b.imaginary}),mul=(a:any,b:any)=>({real:a.real*b.real-a.imaginary*b.imaginary,imaginary:a.real*b.imaginary+a.imaginary*b.real}),div=(a:any,b:any)=>{const d=b.real*b.real+b.imaginary*b.imaginary;return {real:(a.real*b.real+a.imaginary*b.imaginary)/d,imaginary:(a.imaginary*b.real-a.real*b.imaginary)/d};};
 const r=add(m[0][0],m[0][1],sign),t=add(m[2][0],m[2][1],sign);
 const one={real:1,imaginary:0},a=div(add(add(one,mul(r,r),-1),mul(t,t),1),{real:2*t.real,imaginary:2*t.imaginary});
 comparisons.push({mode:sign===1?'even':'odd',metric:'ABCD A real',frequencyHz:frequencies[f],...compareValue(a.real,Math.cos(2*Math.PI*frequencies[f]*.02*analyticDelay),1e-4)});
}
const folder=process.env.P7_OUTPUT_DIR??'debugging/p7-closure';mkdirSync(folder,{recursive:true});const evidence={status:comparisons.every((x:any)=>x.status==='pass')?'pass':'fail',scope:'Genuine coupled C/L FEM and propagated four-port S; independent homogeneous Maxwell TEM velocity/ABCD comparison only. Absolute coupled C/L/Z/crosstalk accuracy remains unverified; TEM velocity is structurally imposed by C/C0 extraction.',line,frequencies,comparisons,wallSeconds:(performance.now()-start)/1000,reused:process.argv.includes('--reuse'),provenance,numericalResidual:result.field.quality.actual.normalizedResidual,capacitanceFPerM:result.parameters.capacitanceFPerM,inductanceHPerM:result.parameters.inductanceHPerM,network:result.network,nativeElapsedMs:result.elapsedMs??null,nativeFieldPreparationElapsedMs:result.fieldReuse?.fieldPreparationElapsedMs??null,resources:reused?'comparison-only replay; original provenance retained, no cold timing':'Direct DSL includes worker/preparation/validation/serialization; process-tree metrics in supervision.json'};
writeFileSync(folder+'/stripline-comparison.json',JSON.stringify(evidence,null,2));console.log(JSON.stringify({status:evidence.status,comparisons,maxError:Math.max(...comparisons.map((x:any)=>x.error)),provenance:evidence.provenance}));

process.exitCode=evidence.status==='pass'?0:1;
} catch(error) {
 const reason=String(error);const status=reason.includes('TIMEOUT')?'timeout':reason.includes('BUDGET')||reason.includes('MEMORY')?'resource-failure':reason.includes('UNSUPPORTED')?'unsupported':reason.includes('MISSING_EVIDENCE')?'missing-evidence':'validation-failure';
 writeFileSync(packetFolder+'/stripline-comparison.json',JSON.stringify({status,reason,provenance}));console.error(reason);process.exitCode=1;
}
