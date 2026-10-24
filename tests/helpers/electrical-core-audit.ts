import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {networkResource} from '../../src/analyze/analysis-dsl/frequency-model.ts';
import type {AnalysisDslOutput} from '../../src/analyze/run-analysis-dsl.ts';
export const coreCaseNames=['ac','sampled','sampled refined','matched','reflected','cascade frequency'];
export function coreCases(result:AnalysisDslOutput){
 const logs=result.reports.flatMap(r=>r.logs).filter((l:any)=>l.message==='core.case') as any[];
 const calculations=result.reports.flatMap(r=>r.calculations).filter((c:any)=>c.phase==='completed') as any[];
 assert.deepEqual(logs.map(l=>l.data.name),coreCaseNames);assert.equal(calculations.length,coreCaseNames.length);
 return new Map(logs.map(l=>{const found=calculations.filter(c=>c.result.id===l.data.resultId);assert.equal(found.length,1);return[l.data.name,found[0].result];}));
}
const finite=(v:unknown)=>{assert.equal(typeof v,'number');assert.ok(Number.isFinite(v));return v as number;};
const close=(v:unknown,w:number,absolute:number,relative=0)=>assert.ok(Math.abs(finite(v)-w)<=absolute+relative*Math.abs(w),`${v} differs from ${w}`);
function axis(values:unknown,expected:number[]){assert.deepEqual(values,expected);assert.ok(expected.every((v,i)=>Number.isFinite(v)&&(i===0||v>expected[i-1])));}
function ramp(t:number,rise=1e-9){return Math.max(0,Math.min(1,t/rise));}
function sampledReference(t:number){const kdc=1/1.1,kinf=1/1.6,tau=1.6/(2*Math.PI*73*1.1),rise=1e-4;return t<=rise?kinf*t/rise+(kdc-kinf)*(t-tau*(1-Math.exp(-t/tau)))/rise:kdc-(kdc-kinf)*tau/rise*(1-Math.exp(-rise/tau))*Math.exp(-(t-rise)/tau);}
function bounce(t:number,rs:number,rl:number){const gs=(rs-50)/(rs+50),gl=(rl-50)/(rl+50),launch=50/(rs+50);let input=launch*ramp(t),output=0;for(let k=0;k<20;k++){output+=launch*(1+gl)*(gs*gl)**k*ramp(t-(2*k+1)*2e-9);input+=launch*(1+gs)*gl*(gs*gl)**k*ramp(t-2*(k+1)*2e-9);}return[input,output];}
/** Saved-field audit, independent closed forms; never executes native/DSL or
 * regenerates missing rows. All port traces and every expected plot are required. */
export function auditElectricalCore(result:AnalysisDslOutput){
 assert.equal(result.status,'complete',JSON.stringify(result.error));const cases=coreCases(result);
 const plots=result.reports.flatMap(r=>r.plots)as any[];assert.equal(plots.length,14);
 const checkPlot=(title:string,unit:string,expectedAxis:number[],values:number[],status:string)=>{
  const found=plots.filter(p=>p.title===title);assert.equal(found.length,1,title);const d=found[0].data;
  assert.equal(d.kind,'series');assert.equal(d.unit,unit);assert.equal(d.validity.status,status);assert.deepEqual(d.axis,{name:title.startsWith('ac:')||title.startsWith('cascade:')?'frequencyHz':'timeS',unit:title.startsWith('ac:')||title.startsWith('cascade:')?'Hz':'s',values:expectedAxis});assert.equal(d.values.length,values.length);d.values.forEach((v:unknown,i:number)=>close(v,values[i],1e-12,1e-10));
 };
 const ac=cases.get('ac').solution;assert.equal(ac.status,'complete');assert.ok(ac.quality.relativeResidual<=1e-8);assert.equal(ac.acExcitation.sources.length,1);assert.ok(ac.acExcitation.convention.includes('separate from impedance tests'));
 const frequencies=Array.from({length:41},(_,i)=>1000+(100000-1000)*(i/40));axis(ac.axis,frequencies);
 const input:number[]=[],output:number[]=[],transfer:number[]=[],phase:number[]=[];let maximumAc=0;
 for(let i=0;i<frequencies.length;i++){
  const im=2*Math.PI*frequencies[i]*2e-9,re=.0001,denRe=1+1050*re,denIm=1050*im,den=denRe**2+denIm**2;
  const out={real:(denRe+.5*denIm)/den,imaginary:(.5*denRe-denIm)/den};
  const vin={real:(1+1000*re)*out.real-1000*im*out.imaginary,imaginary:(1+1000*re)*out.imaginary+1000*im*out.real};
  for(const[name,z]of [['out',out],['in',vin]]as const){const actual=ac.nodeVoltages[name][i];for(const field of ['real','imaginary']as const){close(actual[field],z[field],1e-12,1e-9);maximumAc=Math.max(maximumAc,Math.abs(actual[field]-z[field]));}}
  input.push(Math.hypot(vin.real,vin.imaginary));output.push(Math.hypot(out.real,out.imaginary));transfer.push(1/Math.hypot(1+1000*re,1000*im));phase.push(-Math.atan2(1000*im,1+1000*re)*180/Math.PI);
 }
 for(const [name,values,unit]of [['input',input,'V'],['output',output,'V'],['transfer',transfer,'1'],['phase',phase,'deg']]as const)checkPlot('ac:'+name,unit,frequencies,values,'valid');
 for(const values of Object.values(ac.currents)as any[]){assert.equal(values.length,41);for(const z of values){finite(z.real);finite(z.imaginary);}}
 assert.ok(ac.resolvedModels.length>=3);assert.ok(ac.resources.peakRssBytes>0);
 let maximumTime=0,refinement=0;
 for(const name of ['sampled','sampled refined']){
  const s=cases.get(name).solution;assert.equal(s.status,'complete');assert.equal(s.conditional,true);assert.equal(s.fit.validity.status,'conditional');assert.ok(s.fit.sourceLimitations.includes('unseen narrow resonances'));assert.ok(s.quality.relativeResidual<=1e-8);
  assert.equal(s.sourceResources.length,1);assert.equal(s.sourceResources[0].resource.resourceId,'authored-sampled-rc');assert.equal(s.sourceResources[0].sha256,s.sourceResources[0].resource.sha256);assert.match(s.sourceResources[0].sha256,/^[a-f0-9]{64}$/);
  assert.deepEqual(s.portMapping.outputs,[{name:'output',index:0}]);assert.deepEqual(s.portMapping.instances[0].sourcePorts,['p']);assert.deepEqual(s.portMapping.instances[0].logicalPorts,['output']);
  const sourceAxis=[0,...Array.from({length:161},(_,i)=>10**(-1+i/20))];
  const expectedResource=networkResource({id:'authored-sampled-rc',kind:'network',ports:['p'],analyses:['networkParameters','si'],source:'Independent one-pole Y formula sampled before native fitting; closure authored explicitly',data:{format:'network-parameters',representation:'Y',referenceTopology:'common',nature:'passive',frequenciesHz:sourceAxis,parameters:sourceAxis.map(f=>{const q=f/73;return[[{real:.002+.01*q*q/(1+q*q),imaginary:.01*q/(1+q*q)}]];}),temporal:{method:'passive-relaxation',normalizationOhm:50,highFrequency:{conductanceS:[.012],capacitanceF:[0]}}}});
  assert.equal(s.sourceResources[0].sha256,createHash('sha256').update(JSON.stringify(expectedResource)).digest('hex'));
  const expected=Array.from({length:2001},(_,i)=>i===2000?.02:i*1e-5);axis(s.axis,expected);assert.equal(s.portVoltages.length,expected.length);assert.equal(s.driverCurrentsA.length,expected.length);
  const voltage=s.portVoltages.map((row:unknown[],i:number)=>{assert.equal(row.length,1);const v=finite(row[0]);const error=Math.abs(v-sampledReference(expected[i]));maximumTime=Math.max(maximumTime,error);assert.ok(error<=.001+1e-6);assert.equal(s.driverCurrentsA[i].length,1);close(s.driverCurrentsA[i][0],(ramp(expected[i],1e-4)-v)/50,1e-12,1e-9);return v;});
  checkPlot(name+':output','V',expected,voltage,'conditional');
  for(const row of s.fit.refinement){for(const value of row.kkt)assert.ok(value<=1e-9);}
  const last=s.fit.refinement.at(-1);assert.ok(last.trainingMaximum<=.005+1e-9&&last.holdoutMaximum<=.005+1e-9&&last.orderMaximumChange<=.001);
  assert.deepEqual(s.fit.frequenciesHz,[0,...Array.from({length:161},(_,i)=>10**(-1+i/20))]);assert.equal(s.fit.sourceAdmittanceS.length,162);assert.equal(s.fit.fittedAdmittanceS.length,162);
  s.fit.coefficients.forEach((m:any)=>{assert.ok(m.polesPerS.every((v:number)=>v>0));assert.ok(m.conductanceStrengthsS.every((v:number)=>v>=0));close(m.conductanceStrengthsS.reduce((a:number,b:number)=>a+b,0),.01,1e-10);});
  assert.ok(Math.abs(s.quality.energyBalanceJ)<=1e-14+1e-7*(Math.abs(s.quality.workJ)+Math.abs(s.quality.dissipatedJ)+Math.abs(s.quality.storedEnergyChangeJ)+Math.abs(s.quality.beNumericalDissipationJ)));
 }
 const coarse=cases.get('sampled').solution,fine=cases.get('sampled refined').solution;
 for(let i=0;i<fine.axis.length;i++)refinement=Math.max(refinement,Math.abs(coarse.portVoltages[i][0]-fine.portVoltages[i][0]));assert.ok(refinement<=.0005);
 for(const[name,rl]of [['matched',50],['reflected',100]]as const){
  const s=cases.get(name).solution;assert.equal(s.conditional,false);assert.ok(s.quality.relativeResidual<=1e-8);const expected=Array.from({length:301},(_,i)=>i===300?30e-9:i*.1e-9);axis(s.axis,expected);assert.equal(s.portVoltages.length,301);assert.equal(s.driverCurrentsA.length,301);assert.ok(s.history.acceptedKnots>2);assert.ok(s.history.deliveredFormulaCorners>2);assert.ok(s.history.numericValues<=2_000_000);assert.ok(s.resources.peakRssBytes>0);
  for(let i=0;i<expected.length;i++){assert.equal(s.portVoltages[i].length,2);const reference=bounce(expected[i],25,rl);for(let j=0;j<2;j++){const error=Math.abs(finite(s.portVoltages[i][j])-reference[j]);maximumTime=Math.max(maximumTime,error);assert.ok(error<=.001+1e-6);}close(s.driverCurrentsA[i][0],(ramp(expected[i])-s.portVoltages[i][0])/25,1e-12,1e-9);}
  for(const[j,port]of ['input','output'].entries())checkPlot(name+':'+port,'V',expected,s.portVoltages.map((v:number[])=>v[j]),'valid');
  close(s.quality.internalDissipatedJ,0,1e-22);assert.ok(Math.abs(s.quality.energyBalanceJ)<=1e-12+1e-3*(Math.abs(s.quality.workJ)+Math.abs(s.quality.externalDissipatedJ)+Math.abs(s.quality.finalStoredEnergyJ-s.quality.initialStoredEnergyJ)));
 }
 const cascade=cases.get('cascade frequency').solution;axis(cascade.frequenciesHz,Array.from({length:21},(_,i)=>1e7+(1e8-1e7)*(i/20)));assert.equal(cascade.parameters.length,21);assert.equal(cascade.conditional,false);
 for(let i=0;i<2;i++)for(let j=0;j<2;j++){
  const magnitude:number[]=[];for(let f=0;f<21;f++){assert.equal(cascade.parameters[f].length,2);assert.equal(cascade.parameters[f][i].length,2);const z=cascade.parameters[f][i][j],gain=i===j?0:.5,theta=-2*Math.PI*cascade.frequenciesHz[f]*2e-9;close(z.real,gain*Math.cos(theta),1e-10);close(z.imaginary,gain*Math.sin(theta),1e-10);magnitude.push(Math.hypot(z.real,z.imaginary));}
  checkPlot('cascade:'+['input','output'][i]+'<-'+['input','output'][j],'1',cascade.frequenciesHz,magnitude,'valid');
 }
 const checks=result.reports.flatMap(r=>r.checks);assert.deepEqual(checks.map(c=>c.status),['fail','pass']);assert.ok(result.reports.flatMap(r=>r.logs).some((l:any)=>l.message==='core.binding'));
 return{status:'pass',cases:cases.size,plots:plots.length,maximumAcAbsoluteError:maximumAc,maximumTimeAbsoluteError:maximumTime,independentTimeRefinementV:refinement,expectedFailedChecks:1,followingPasses:1};
}
