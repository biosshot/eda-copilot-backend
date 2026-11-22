import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {networkResource} from '../../src/analyze/analysis-dsl/frequency-model.ts';
function expectedSource(second:boolean){return networkResource({id:second?'cell2':'cell1',kind:'network',source:'Authored full-C conductors; no intermodel EM extraction',validity:{status:'conditional',reason:'Authored synthetic coupon only',domain:{bandHz:[0,1e9]}},analyses:['si','networkParameters'],ports:['join','probe'],data:{format:'coupled-rlc',referenceTopology:'infinity',potentialNodes:['p','n'],currentDofs:['j'],incidence:[[0,0,1],[1,0,-1]],portIncidence:[[0,0,1],[1,0,-1],[0,1,1],[1,1,-1]],resistanceOhm:[[0,0,100]],inductanceH:[[0,0,1e-6]],capacitanceF:second?[[0,0,7e-9],[0,1,-3e-9],[1,0,-3e-9],[1,1,7e-9]]:[[0,0,3e-9],[0,1,-2e-9],[1,0,-2e-9],[1,1,3e-9]],conservedPotentialModes:[[1,1]],portTerminals:[{positive:{node:'p'},negative:{node:'n'}},{positive:{node:'p'},negative:{node:'n'}}]}});}
const hash=(v:unknown)=>createHash('sha256').update(JSON.stringify(v)).digest('hex');
const sorted=(v:any):any=>Array.isArray(v)?v.map(sorted):v&&typeof v==='object'?Object.fromEntries(Object.keys(v).sort().map(k=>[k,sorted(v[k])])):v;
const finite=(x:any):number=>{assert.equal(typeof x,'number');assert.ok(Number.isFinite(x));return x;};
const near=(x:any,y:number,t:number)=>assert.ok(Math.abs(finite(x)-y)<=t,`${x} != ${y}, tolerance ${t}`);
export const physicalNames=['both','both refined','both common','both frequency','single','single refined','single common','single frequency'];
export function physicalCases(result:any){
 assert.equal(result.status,'complete',JSON.stringify(result.error));
 const logs=result.reports.flatMap((r:any)=>r.logs).filter((l:any)=>l.message==='physical.case');
 assert.deepEqual(logs.map((l:any)=>l.data.name),physicalNames);
 const completed=result.reports.flatMap((r:any)=>r.calculations).filter((c:any)=>c.phase==='completed');assert.equal(completed.length,8);
 return new Map<string,any>(logs.map((l:any)=>{const found=completed.filter((c:any)=>c.result.id===l.data.resultId);assert.equal(found.length,1);return[l.data.name,found[0].result];}));
}
export function physicalBoth(t:number){const d=Math.sqrt(1e16-4/(.5e-6*7.5e-9)),r1=(-1e8+d)/2,r2=(-1e8-d)/2;return(-r2*Math.exp(r1*t)+r1*Math.exp(r2*t))/(r1-r2);}
// Direct handwritten five-state ODE, no production matrices/solver/helpers.
function derivative(x:number[]){return[(-7*x[3]-12*x[4])*1e9/155,(47*x[3]-8*x[4])*1e9/155,(-3*x[3]+17*x[4])*1e9/155,(x[0]-x[1]-100*x[3])*1e6,(x[0]-x[2]-100*x[4])*1e6];}
function step(x:number[],dt:number){let term=x.slice(),out=x.slice();for(let k=1;k<=12;k++){term=derivative(term).map(v=>v*dt/k);out=out.map((v,j)=>v+term[j]);}return out;}
export function physicalSingle(axis:number[],substep=.25e-9){let x=[2,1,1,0,0],at=0;return axis.map(t=>{while(at<t-1e-18){const h=Math.min(substep,t-at);x=step(x,h);at+=h;}return x.slice();});}
type Z=[number,number];
const add=(a:Z,b:Z):Z=>[a[0]+b[0],a[1]+b[1]],neg=(a:Z):Z=>[-a[0],-a[1]],mul=(a:Z,b:Z):Z=>[a[0]*b[0]-a[1]*b[1],a[0]*b[1]+a[1]*b[0]],div=(a:Z,b:Z):Z=>[(a[0]*b[0]+a[1]*b[1])/(b[0]**2+b[1]**2),(a[1]*b[0]-a[0]*b[1])/(b[0]**2+b[1]**2)];
function frequencyReference(single:boolean,f:number){
 const w=2*Math.PI*f,y=div([1,0],[100,w*1e-6]);
 if(!single){const z=div([1,0],add([0,w*7.5e-9],[2*y[0],2*y[1]]));return[[z,z],[z,z]];}
 const c=[[10,-2,-3],[-2,3,0],[-3,0,7]],b=[[1,1],[-1,0],[0,-1]];
 const m:Z[][]=c.map((row,i)=>row.map((v,j)=>add([0,w*v*1e-9],[y[0]*(b[i][0]*b[j][0]+b[i][1]*b[j][1]),y[1]*(b[i][0]*b[j][0]+b[i][1]*b[j][1])])));
 const result:Z[][]=[[[],[]],[[],[]]] as any;
 for(let input=0;input<2;input++){
  const a=m.map((row,i)=>[...row.map(z=>z.slice() as Z),[b[i][input],0] as Z]);
  for(let k=0;k<3;k++){let best=k;for(let i=k+1;i<3;i++)if(Math.hypot(...a[i][k])>Math.hypot(...a[best][k]))best=i;[a[k],a[best]]=[a[best],a[k]];const pivot=a[k][k];for(let j=k;j<4;j++)a[k][j]=div(a[k][j],pivot);for(let i=0;i<3;i++)if(i!==k){const q=a[i][k];for(let j=k;j<4;j++)a[i][j]=add(a[i][j],neg(mul(q,a[k][j])));}}
  result[0][input]=add(a[0][3],neg(a[1][3]));result[1][input]=add(a[0][3],neg(a[2][3]));
 }
 return result;
}
export function auditElectricalPhysical(result:any){
 const cases=physicalCases(result),report:Record<string,any>={};
 const plots=result.reports.flatMap((r:any)=>r.plots);assert.equal(plots.length,8);
 const negatives=result.reports.flatMap((r:any)=>r.logs).filter((l:any)=>l.message==='physical.negative');assert.deepEqual(negatives.map((l:any)=>l.data.kind),['charge','weld','steady','permission','delay','table']);
 for(const l of negatives)assert.match(l.data.error,/COUPLED_|authorize SI|contacts|return|trace/);
 for(const name of physicalNames){const c=cases.get(name),v=c.solution;assert.ok(v);assert.equal(c.status,'complete');assert.ok(v.conditional);
  if(name.endsWith('frequency')){
   assert.ok(v.assembledModel);assert.equal(v.assembledModel.referenceTopology,'infinity');assert.match(v.validityLimitations,/intermodel mutual C\/L/);
   assert.equal(v.provenance.length,2);for(const [index,p] of v.provenance.entries()){const expected=expectedSource(index===1);assert.deepEqual(p.sourceValidity,expected.validity);assert.deepEqual(p.allowedAnalyses,expected.analyses);assert.equal(p.resource.resourceId,expected.id);assert.equal(p.resource.sha256,hash(sorted(expected)));assert.equal(v.sourceHashes[index].sha256,hash(expected));}
   let error=0;for(let k=0;k<v.frequenciesHz.length;k++){const ref=frequencyReference(name.startsWith('single'),finite(v.frequenciesHz[k]));for(let i=0;i<2;i++)for(let j=0;j<2;j++){const z=v.parameters[k][i][j];for(const [part,expected]of [['real',ref[i][j][0]],['imaginary',ref[i][j][1]]]as const){error=Math.max(error,Math.abs(finite(z[part])-expected));near(z[part],expected,1e-10+1e-8*Math.abs(expected));}}}
   const plot=plots.filter((p:any)=>p.title===name);assert.equal(plot.length,1);assert.equal(plot[0].data.unit,'Ohm');assert.equal(plot[0].data.validity.status,'conditional');assert.deepEqual(plot[0].data.axis.values,v.frequenciesHz);for(let i=0;i<v.frequenciesHz.length;i++)near(plot[0].data.values[i],Math.hypot(v.parameters[i][0][0].real,v.parameters[i][0][0].imaginary),1e-10);
   report[name]={frequencyError:error};continue;
  }
  const single=name.startsWith('single'),common=name.endsWith('common'),nodes=single?3:2;assert.ok(v.physicalRecovery.mapped);assert.equal(v.physicalRecovery.potentials.length,4);assert.ok(v.physicalRecovery.potentials.every((p:any)=>Number.isInteger(p.index)));assert.equal(v.zeroEnergyGauges,0);
  assert.match(c.assumptions.join(' '),/intermodel mutual C\/L/);assert.equal(v.portMapping.instances.length,2);assert.deepEqual(v.portMapping.connectionContacts,[single?'positive':'both']);
  for(const [index,source] of v.sourceResources.entries()){const expected=expectedSource(index===1);assert.deepEqual(source.sourceValidity,expected.validity);assert.deepEqual(source.allowedAnalyses,expected.analyses);assert.equal(source.resource.resourceId,expected.id);assert.equal(source.resource.source,expected.source);assert.equal(source.resource.sha256,source.sha256);assert.equal(hash(expected),source.sha256);}
  const plot=plots.filter((p:any)=>p.title===name+':left');assert.equal(plot.length,1);assert.equal(plot[0].data.unit,'V');assert.equal(plot[0].data.validity.status,'conditional');assert.deepEqual(plot[0].data.axis.values,v.axis);assert.deepEqual(plot[0].data.values,v.portVoltages.map((v:number[])=>v[0]));
  const axis=v.axis.map(finite);assert.equal(axis.length,501);for(let i=1;i<axis.length;i++)assert.ok(axis[i]>axis[i-1]);
  for(const key of ['conductorPotentialsV','conductorChargesC','energyJ','portVoltages'])assert.equal(v[key].length,axis.length);
  const reference=single?physicalSingle(axis):axis.map((t:number)=>{const d=physicalBoth(t);return[1.5+d/2,1.5-d/2];});
  if(single&&!common){const check=physicalSingle(axis,.125e-9);for(let i=0;i<axis.length;i++)for(let j=0;j<5;j++)near(reference[i][j],check[i][j],1e-10);}
  let error=0,maxChargeError=0;
  for(let i=0;i<axis.length;i++){
   assert.equal(v.conductorPotentialsV[i].length,nodes);assert.equal(v.conductorChargesC[i].length,nodes);
   for(let j=0;j<nodes;j++){const expected=common?2:reference[i][j];error=Math.max(error,Math.abs(finite(v.conductorPotentialsV[i][j])-expected));}
   const total=v.conductorChargesC[i].map(finite).reduce((a:number,b:number)=>a+b,0);maxChargeError=Math.max(maxChargeError,Math.abs(total-(common?20e-9:15e-9)));near(total,common?20e-9:15e-9,1e-18);
   for(const port of v.portVoltages[i])finite(port);near(v.portVoltages[i][0],v.conductorPotentialsV[i][0]-v.conductorPotentialsV[i][1],1e-12);near(v.portVoltages[i][1],v.conductorPotentialsV[i][0]-v.conductorPotentialsV[i][single?2:1],1e-12);
   finite(v.energyJ[i]);if(common)near(v.energyJ[i],20e-9,1e-20);
  }
  assert.ok(error<=(name.includes('refined')?.000125:.00025),`${name} reference ${error}`);near(v.energyJ[0],common?20e-9:15e-9,1e-20);
  const q=v.quality;for(const k of ['relativeResidual','workJ','dissipatedJ','storedEnergyChangeJ','beNumericalDissipationJ','rawEnergyBalanceJ','energyBalanceJ','energyNormalizationJ','normalizedEnergyBalance','maximumAcceptedStepS'])finite(q[k]);
  near(q.rawEnergyBalanceJ,q.energyBalanceJ+q.beNumericalDissipationJ,1e-20);assert.ok(q.dissipatedJ>=0);assert.ok(q.beNumericalDissipationJ>=0);const norm=Math.abs(q.workJ)+q.dissipatedJ+Math.abs(q.storedEnergyChangeJ)+q.beNumericalDissipationJ;near(q.energyNormalizationJ,norm,1e-20);assert.ok(Math.abs(q.energyBalanceJ)<=1e-14+1e-7*norm);assert.ok(Math.abs(q.energyBalanceJ)<=(norm===0?1e-20:1e-4*norm));near(q.normalizedEnergyBalance,norm===0?0:Math.abs(q.energyBalanceJ)/norm,1e-12);assert.ok(q.normalizedEnergyBalance<=1e-4);assert.ok(q.maximumAcceptedStepS<=(name.includes('refined')?.5e-9:1e-9)*(1+1e-12));assert.ok(v.memoryPlanBytes<64*1048576);
  report[name]={referenceErrorV:error,maxChargeErrorC:maxChargeError,reference:reference.map(r=>r.slice(0,nodes)),quality:q,memoryPlanBytes:v.memoryPlanBytes};
 }
 for(const name of ['both','single']){const coarse=report[name].referenceErrorV,fine=report[name+' refined'].referenceErrorV;assert.ok(fine<=.51*coarse||fine<1e-6);}
 return{status:'pass',cases:report,negativeCount:negatives.length};
}
