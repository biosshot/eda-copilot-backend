import test from 'node:test';import assert from 'node:assert/strict';
import{existsSync,mkdtempSync,readFileSync,writeFileSync,rmSync}from'node:fs';import{tmpdir}from'node:os';import{join}from'node:path';import{spawnSync}from'node:child_process';
const root=process.cwd();
const units={capacitanceFPerM:'F/m',vacuumCapacitanceFPerM:'F/m',inductanceHPerM:'H/m',resistanceOhmPerM:'Ohm/m',conductanceSPerM:'S/m'};
const ports=['positiveInput','negativeInput','positiveOutput','negativeOutput'];const axis=[1e6,1e7,1e8];
// Synthetic matched reciprocal line: exercises comparison execution, not EM accuracy.
function calculation(){
 const delay=2/299792458;
 const parameters=axis.map(f=>Array.from({length:4},(_,i)=>Array.from({length:4},(_,j)=>{
  const phase=2*Math.PI*f*.02*delay;
  return Math.abs(i-j)===2?{real:Math.cos(phase),imaginary:-Math.sin(phase)}:{real:0,imaginary:0};
 })));
 return {status:'complete',rlgc:{unit:'1',data:{units}},network:{kind:'network',id:'test'},solution:{
  status:'complete',frequenciesHz:[...axis],field:{quality:{actual:{normalizedResidual:0}}},
  parameters:{capacitanceFPerM:[[2e-10,-1e-11],[-1e-11,2e-10]],inductanceHPerM:[[2e-7,1e-8],[1e-8,2e-7]],
   modes:['even','odd'].map(name=>({name,delaySPerM:delay,impedanceOhm:50,capacitanceFPerM:delay/50,inductanceHPerM:delay*50}))},
  network:{ports:[...ports],referenceOhm:50,parameters}
 }};
}
function compareInSubprocess(c:any){
 const folder=mkdtempSync(join(tmpdir(),'p7-validation-'));
 try{
  const path=join(folder,'input.json');writeFileSync(path,JSON.stringify(c));
  const p=spawnSync(process.execPath,['--import','tsx','scripts/verify-p7-closure-stripline.ts'],{cwd:root,env:{...process.env,P7_OUTPUT_DIR:folder,P7_VALIDATION_REPORT:path},encoding:'utf8',timeout:15000});
  assert.equal(p.error,undefined);assert.equal(p.signal,null);
  const evidencePath=join(folder,'stripline-comparison.json');
 assert.ok(existsSync(evidencePath),p.stdout+p.stderr);
 return {exit:p.status,evidence:JSON.parse(readFileSync(evidencePath,'utf8')),output:p.stdout+p.stderr};
 }finally{rmSync(folder,{recursive:true,force:true});}
}
test('real comparator subprocess passes valid control and fails exactly one perturbed delay',()=>{
 const control=calculation();const positive=compareInSubprocess(control);
 assert.equal(positive.exit,0,positive.output);assert.equal(positive.evidence.status,'pass');
 assert.equal(positive.evidence.reason,undefined);assert.equal(positive.evidence.comparisons.length,8);
 assert.ok(positive.evidence.comparisons.every((c:any)=>c.status==='pass'&&Number.isFinite(c.error)));
 const changed=structuredClone(control);changed.solution.parameters.modes[0].delaySPerM*=1.09;
 // This one modal delay is the only changed input field; transmission remains finite.
 const restored=structuredClone(changed);restored.solution.parameters.modes[0].delaySPerM=control.solution.parameters.modes[0].delaySPerM;
 assert.deepEqual(restored,control);
 const negative=compareInSubprocess(changed);
 assert.equal(negative.exit,1,negative.output);assert.equal(negative.evidence.status,'fail');
 assert.equal(negative.evidence.reason,undefined);assert.doesNotMatch(negative.output,/INVALID_VALUE|NaN|SCHEMA/);
 const failures=negative.evidence.comparisons.filter((c:any)=>c.status==='fail');assert.equal(failures.length,1);
 assert.equal(failures[0].mode,'even');assert.equal(failures[0].metric,'homogeneous TEM delay s/m');
 assert.ok(Math.abs(failures[0].error-.09)<1e-12);assert.ok(failures[0].error>.08);
 assert.equal(failures[0].actual,changed.solution.parameters.modes[0].delaySPerM);
 assert.equal(failures[0].reference,control.solution.parameters.modes[0].delaySPerM);
});
test('malformed comparator subprocess inputs fail for the expected reason',()=>{
 const cases:[(c:any)=>void,string,string][]=[
  [c=>delete c.solution,'missing-evidence','MISSING_EVIDENCE'],
  [c=>c.rlgc.unit='F/m','validation-failure','UNIT_MISMATCH'],
  [c=>c.solution.network.ports=['wrong'],'validation-failure','PORT_MISMATCH'],
  [c=>c.solution.frequenciesHz=[1],'validation-failure','BAND_MISMATCH'],
  [c=>c.network.kind='wrong','validation-failure','SCHEMA_MISMATCH'],
  [c=>c.solution.network.parameters[0][0][0].real=null,'validation-failure','NETWORK_MATRIX_INVALID'],
  [c=>c.solution.parameters.capacitanceFPerM=[[1]],'validation-failure','MATRIX_INVALID'],
  [c=>c.solution.network.parameters[0][0][0].real=Number.POSITIVE_INFINITY,'validation-failure','NETWORK_MATRIX_INVALID'],
 ];
 for(const [mutate,status,reason] of cases){
  const c=calculation();mutate(c);const actual=compareInSubprocess(c);
  assert.equal(actual.exit,1,actual.output);assert.equal(actual.evidence.status,status);
  assert.equal(actual.evidence.reason,'Error: '+reason);
 }
});
test('supervisor subprocess rejects missing evidence, unsupported, failed comparison, child nonzero and timeout',()=>{
 for(const [body,expected] of [["",'missing-evidence'],["writeFileSync(process.env.P7_OUTPUT_DIR+'/stripline-comparison.json',JSON.stringify({status:'unsupported'}))",'unsupported'],["writeFileSync(process.env.P7_OUTPUT_DIR+'/stripline-comparison.json',JSON.stringify({status:'fail'}))",'validation-failure'],["process.exit(3)",'child-failure'],["setTimeout(()=>{},5000)",'timeout']] as const){
 const folder=mkdtempSync(join(tmpdir(),'p7-supervision-'));try{const command=[process.execPath,'--input-type=module','-e',"import{writeFileSync}from'node:fs';"+body];const p=spawnSync('python',['scripts/verify-p7-closure-supervised.py'],{cwd:root,env:{...process.env,P7_CLOSURE_CONTROL:'stripline',P7_SUPERVISOR_OUTPUT:folder,P7_SUPERVISOR_TEST_COMMAND:JSON.stringify(command),P7_SUPERVISOR_TEST_TIMEOUT:'0.5'},encoding:'utf8',timeout:15000});assert.notEqual(p.status,0,p.stdout+p.stderr);assert.equal(JSON.parse(readFileSync(join(folder,'supervision.json'),'utf8')).status,expected);}finally{rmSync(folder,{recursive:true,force:true});}
 }
});
