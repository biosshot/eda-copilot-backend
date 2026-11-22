/** Exact requests/oracles from the existing native electrical.rs
 * rl_step_and_damped_rlc_converge_to_independent_analytic_response test.
 * Runs only this prior debug-deadline failure through the normal release addon. */
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {createHash} from 'node:crypto';
import {resolve} from 'node:path';
const require=createRequire(import.meta.url),native=require('../../native/node-addon/index.cjs');
const {nativeFilename}=require('../../native/node-addon/platform.cjs');
const output=resolve(process.argv[2]??'debugging/p6-verification/release-rlc');mkdirSync(output,{recursive:true});
const runs=[];
for(const rlc of [false,true]){
 const elements=[{id:'v',kind:'voltage-source',nodes:['p','gnd'],voltageV:0,waveform:{kind:'step',from:0,to:1,atS:0,riseS:1e-6}},{id:'l',kind:'inductor',nodes:['p','q'],inductanceH:.001,seriesResistanceOhm:1},rlc?{id:'c',kind:'capacitor',nodes:['q','gnd'],capacitanceF:1e-6,esrOhm:0,eslH:0}:{id:'r',kind:'resistor',nodes:['q','gnd'],resistanceOhm:1}];
 let previous;
 for(const tolerance of [1e-7,1e-8]){
  const request={contextId:'test',circuit:{nodes:['gnd','p','q'],reference:'gnd',ports:[{id:'out',positive:'q',negative:'gnd'}],elements},mode:'time',frequencies:[1,10,100,1000],time:{stopS:.002,sampleStepS:2e-6},initial:'steady',numerics:{relativeTolerance:tolerance,absoluteTolerance:1e-10,maxIterations:1000000}};
  const start=performance.now(),report=new native.AnalysisElectricalSession().solve(request),wallMs=performance.now()-start;
  writeFileSync(resolve(output,`${rlc?'rlc':'rl'}-${tolerance}.json`),JSON.stringify({request,report,wallMs}));
  assert.equal(report.status,'complete',JSON.stringify(report.diagnostics));
  const values=rlc?report.nodeVoltages.q:report.currents.l;
  const step=t=>{if(t<=0)return 0;if(!rlc)return(1-Math.exp(-2000*t))/2;const alpha=500,omega=Math.sqrt(1e9-alpha*alpha);return 1-Math.exp(-alpha*t)*(Math.cos(omega*t)+alpha/omega*Math.sin(omega*t));};
  let error=0;for(let at=0;at<report.axis.length;at++){let expected=0;for(let i=0;i<200;i++)expected+=step(report.axis[at]-(i+.5)*1e-6/200);error=Math.max(error,Math.abs(values[at]-expected/200));}
  assert.ok(error<.005);assert.ok(report.quality.relativeEnergyError<.005);
  if(previous){assert.ok(error<previous.error);assert.ok(report.quality.relativeEnergyError<previous.energy);}
  previous={error,energy:report.quality.relativeEnergyError};runs.push({rlc,tolerance,maxAbsoluteError:error,relativeEnergyError:previous.energy,wallMs,acceptedSteps:report.counters.acceptedSteps});
 }
}
const result={status:'pass',profile:'normal GPU-default release addon; analyzer CPU/F64',deadline:'unchanged native default120s per original request',nativeSha256:createHash('sha256').update(readFileSync('native/node-addon/'+nativeFilename())).digest('hex'),originalTestSha256:createHash('sha256').update(readFileSync('native/analyze/tests/electrical.rs')).digest('hex'),runs};
writeFileSync(resolve(output,'manifest.json'),JSON.stringify(result,null,2));console.log(JSON.stringify(result));
