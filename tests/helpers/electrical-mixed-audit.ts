import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {networkResource} from '../../src/analyze/analysis-dsl/frequency-model.ts';
import type {AnalysisDslOutput} from '../../src/analyze/run-analysis-dsl.ts';
export const mixedCaseNames=['rc','rc refined','rc frequency','rlc','rlc refined','rlc frequency','reflections','reflections refined','reflections frequency'];
export function mixedCases(result:AnalysisDslOutput){
 const logs=result.reports.flatMap(r=>r.logs).filter((l:any)=>l.message==='mixed.case') as any[];
 const calculations=result.reports.flatMap(r=>r.calculations).filter((c:any)=>c.phase==='completed') as any[];
 assert.deepEqual(logs.map(l=>l.data.name),mixedCaseNames);assert.equal(calculations.length,mixedCaseNames.length);
 return new Map(logs.map(l=>{const found=calculations.filter(c=>c.result.id===l.data.resultId);assert.equal(found.length,1);return[l.data.name,found[0].result];}));
}
const finite=(x:any)=>{assert.equal(typeof x,'number');assert.ok(Number.isFinite(x));return x as number;};
const close=(x:any,y:number,tol:number)=>assert.ok(Math.abs(finite(x)-y)<=tol,`${x} != ${y}, tolerance ${tol}`);
function integral(t:number,tau:number,m:number){if(t<=0)return 0;let sum=0,term=1;for(let q=0;q<m;q++){if(q)term*=t/tau/q;sum+=(m-q)*term;}return t-tau*m+tau*Math.exp(-t/tau)*sum;}
function erlangRamp(t:number,tau:number,m:number){return(integral(t,tau,m)-integral(t-.5e-9,tau,m))/.5e-9;}
export function mixedReference(name:string,t:number){
 if(name==='rc')return erlangRamp(t-2e-9,1e-9,1)/3;
 if(name==='reflections'){let sum=0;for(let k=0;k<4;k++){let choose=1;for(let j=0;j<=k;j++){if(j)choose*=(k-j+1)/j;sum+=(2/3)*1.2*(-1/3)**k*choose*(-1)**(k-j)*1.2**j*erlangRamp(t-(2+4*k)*1e-9,.66e-9,j+1);}}return sum;}
 const a=50e-9*50*20e-12,b=50e-9+55*50*20e-12,alpha=b/(2*a),omega=Math.sqrt(105/a-alpha**2);
 const step=(x:number)=>x<=0?0:50/105*(1-Math.exp(-alpha*x)*(Math.cos(omega*x)+alpha/omega*Math.sin(omega*x)));
 const n=200,h=.5e-9/n;let sum=0;for(let i=0;i<=n;i++)sum+=step(t-5e-9-i*h)*(i===0||i===n?1:i%2===0?2:4);return sum*h/(3*.5e-9);
}
type Z=[number,number];
const add=(a:Z,b:Z):Z=>[a[0]+b[0],a[1]+b[1]],neg=(a:Z):Z=>[-a[0],-a[1]],mul=(a:Z,b:Z):Z=>[a[0]*b[0]-a[1]*b[1],a[0]*b[1]+a[1]*b[0]],scale=(a:Z,x:number):Z=>[a[0]*x,a[1]*x],div=(a:Z,b:Z):Z=>scale(mul(a,[b[0],-b[1]]),1/(b[0]**2+b[1]**2));
type M=[Z,Z,Z,Z];
const product=(a:M,b:M):M=>[add(mul(a[0],b[0]),mul(a[1],b[2])),add(mul(a[0],b[1]),mul(a[1],b[3])),add(mul(a[2],b[0]),mul(a[3],b[2])),add(mul(a[2],b[1]),mul(a[3],b[3]))];
export function mixedFrequencyReference(name:string,f:number){
 const w=2*Math.PI*f,line=(delay:number):M=>[[Math.cos(w*delay),0],[0,50*Math.sin(w*delay)],[0,Math.sin(w*delay)/50],[Math.cos(w*delay),0]];
 const shunt=name==='reflections',z:Z=[shunt?0:name==='rc'?50:5,shunt?0:name==='rc'?0:w*50e-9],y:Z=[0,w*(name==='rc'?30e-12:20e-12)];
 let m=product(line(2e-9),[add([1,0],mul(z,y)),z,y,[1,0]]);if(name==='rlc')m=product(m,line(3e-9));
 const[A,B,C,D]=m,b=scale(B,1/50),c=scale(C,50),den=add(add(A,b),add(c,D));
 const s11=div(add(add(A,b),neg(add(c,D))),den),s22=div(add(add(neg(A),b),add(neg(c),D)),den),s21=div([2,0],den),s12=div(scale(add(mul(A,D),neg(mul(B,C))),2),den);
 return[[s11,s12],[s21,s22]];
}
/** Audit saved rows only; references use closed forms/ABCD, never native stepping. */
export function auditElectricalMixed(result:AnalysisDslOutput){
 assert.equal(result.status,'complete',JSON.stringify(result.error));const cases=mixedCases(result),plots=result.reports.flatMap(r=>r.plots) as any[];assert.equal(plots.length,24);
 const bindings=result.reports.flatMap(r=>r.logs).filter((l:any)=>l.message==='mixed.binding') as any[];assert.equal(bindings.length,1);
 const contacts=bindings[0].data.portContacts;
 if(contacts){
  const expected=contacts.input.net==='CAN_TX'?['16344fabf9da9061e151','3f778668945e47f4e30','dbff26c58b652962e7']:['e795e157','e833e3','e750e18'];
  assert.deepEqual([contacts.input.id,contacts.output.id,contacts.return.id],expected);assert.equal(contacts.return.net,'GND');assert.equal(contacts.input.net,contacts.output.net);assert.notEqual(contacts.input.id,contacts.output.id);
 }
 const provenance=' External coupon terminals: '+JSON.stringify(contacts);
 const wave=(id:string,delayS:number)=>networkResource({id,kind:'network',ports:['a','b'],analyses:['networkParameters','si'],source:'Authored exact causal 50 Ohm line'+provenance,data:{format:'delayed-scattering',referenceTopology:'common',nature:'passive',referenceOhm:50,scattering:[[0,1],[1,0]],delayS}});
 const lumped=(id:string,R:number,L:number,C:number,shunt=false)=>networkResource({id,kind:'network',ports:['a','b'],analyses:['networkParameters','si'],source:'Authored discrete physical filter'+provenance,data:{format:'coupled-rlc',referenceTopology:'grounded',potentialNodes:shunt?['v']:['a','b'],currentDofs:shunt?[]:['series'],incidence:shunt?[]:[[0,0,1],[1,0,-1]],portIncidence:shunt?[[0,0,1],[0,1,1]]:[[0,0,1],[1,1,1]],resistanceOhm:shunt?[]:[[0,0,R]],inductanceH:shunt?[]:[[0,0,L]],capacitanceF:[[shunt?0:1,shunt?0:1,C]]}});
 const sources:Record<string,ReturnType<typeof networkResource>[]>= {rc:[wave('rc-line',2e-9),lumped('rc-filter',50,0,30e-12)],rlc:[wave('rlc-first',2e-9),lumped('rlc-filter',5,50e-9,20e-12),wave('rlc-last',3e-9)],reflections:[wave('bounce-line',2e-9),lumped('bounce-C',0,0,20e-12,true)]};
 const summary:any={status:'pass',physicalPortContacts:contacts,cases:{}};
 for(const name of ['rc','rlc','reflections']){
  let coarse:any;
  for(const suffix of ['',' refined']){
   const s=cases.get(name+suffix).solution,q=s.quality;
   assert.equal(s.status,'complete');assert.equal(s.conditional,false);Object.values(q).forEach(finite);assert.ok(q.relativeResidual<=1e-8);
   assert.deepEqual(s.portMapping.outputs.map((p:any)=>p.name),['input','output']);
   assert.equal(s.sourceResources.length,name==='rlc'?3:2);assert.ok(s.sourceResources.every((r:any)=>r.allowedAnalyses.includes('si')&&/^[a-f0-9]{64}$/.test(r.sha256)));
   assert.deepEqual(s.portMapping.outputs,[{name:'input',index:0},{name:'output',index:name==='rlc'?5:3}]);
   assert.equal(s.portMapping.instances.length,sources[name].length);
   for(const [index,r] of s.sourceResources.entries()){
    const expected=sources[name][index];assert.equal(r.resource.resourceId,expected.id);assert.equal(r.resource.kind,'network');assert.equal(r.resource.source,expected.source);
    assert.deepEqual(r.allowedAnalyses,expected.analyses);assert.equal(r.sourceValidity,null);assert.equal(r.sha256,r.resource.sha256);assert.equal(r.sha256,createHash('sha256').update(JSON.stringify(expected)).digest('hex'));
    assert.equal(s.portMapping.instances[index].instanceId,r.instanceId);assert.deepEqual(s.portMapping.instances[index].sourcePorts,['a','b']);assert.deepEqual(s.portMapping.instances[index].logicalPorts,['input','output']);
   }
   const count=Math.ceil((name==='rlc'?25e-9:16e-9)/.1e-9)+1,axis=s.axis as number[];assert.equal(axis.length,count);
   axis.forEach((t,i)=>{finite(t);close(t,Math.min(i*.1e-9,name==='rlc'?25e-9:16e-9),1e-22);if(i)assert.ok(t>axis[i-1]);});
   for(const key of ['portVoltages','driverCurrentsA','portCurrentsA','energyJ','lumpedEnergyJ','delayEnergyJ','sourceWorkJ','resistiveLossJ','attenuationLossJ','beNumericalDissipationJ'])assert.equal(s[key].length,axis.length,key);
   const error:number[]=[],reference:number[]=[];
   for(let i=0;i<axis.length;i++){
    assert.equal(s.portVoltages[i].length,2);assert.equal(s.driverCurrentsA[i].length,1);assert.equal(s.portCurrentsA[i].length,2);
    s.portVoltages[i].forEach(finite);s.portCurrentsA[i].forEach(finite);
    const expected=mixedReference(name,axis[i]);reference.push(expected);error.push(Math.abs(s.portVoltages[i][1]-expected));
    close(s.driverCurrentsA[i][0],(Math.min(1,axis[i]/.5e-9)-s.portVoltages[i][0])/(name==='reflections'?25:50),1e-12);
    close(s.energyJ[i],finite(s.lumpedEnergyJ[i])+finite(s.delayEnergyJ[i]),1e-20);
    for(const key of ['resistiveLossJ','attenuationLossJ','beNumericalDissipationJ']){finite(s[key][i]);assert.ok(s[key][i]>=-1e-22);if(i)assert.ok(s[key][i]>=s[key][i-1]-1e-22);}
   }
   const maximum=Math.max(...error);assert.ok(maximum<=.001001,`${name} error ${maximum}`);
   const ul=q.finalLumpedEnergyJ-q.initialLumpedEnergyJ,ud=q.finalDelayEnergyJ-q.initialDelayEnergyJ;
   const raw=q.workJ-q.externalDissipatedJ-q.rlcDissipatedJ-q.internalDissipatedJ-ul-ud;
   const norm=Math.abs(q.workJ)+q.externalDissipatedJ+q.rlcDissipatedJ+q.internalDissipatedJ+Math.abs(ul)+Math.abs(ud)+q.beNumericalDissipationJ;
   close(q.rawEnergyBalanceJ,raw,1e-20);close(q.energyBalanceJ,raw-q.beNumericalDissipationJ,1e-20);close(q.historyQuadratureDefectJ,q.energyBalanceJ,1e-20);close(q.energyNormalizationJ,norm,1e-20);assert.ok(Math.abs(q.energyBalanceJ)<=1e-4*norm);
   close(s.sourceWorkJ.at(-1),q.workJ,1e-20);close(s.resistiveLossJ.at(-1),q.externalDissipatedJ+q.rlcDissipatedJ,1e-20);close(s.attenuationLossJ.at(-1),q.internalDissipatedJ,1e-20);close(s.beNumericalDissipationJ.at(-1),q.beNumericalDissipationJ,1e-20);close(s.energyJ.at(-1),q.finalStoredEnergyJ,1e-20);
   for(const [j,port]of ['input','output'].entries()){const p=plots.filter(p=>p.title===name+suffix+':'+port);assert.equal(p.length,1);assert.equal(p[0].data.kind,'series');assert.equal(p[0].data.unit,'V');assert.equal(p[0].data.validity.status,'valid');assert.deepEqual(p[0].data.axis,{name:'timeS',unit:'s',values:axis});assert.deepEqual(p[0].data.values,s.portVoltages.map((v:number[])=>v[j]));}
   summary.cases[name+suffix]={maximumReferenceErrorV:maximum,normalizedEnergyBalance:Math.abs(q.energyBalanceJ)/norm,rawEnergyBalanceJ:raw,reference,error};
   if(coarse){const refinement=s.portVoltages.map((v:number[],i:number)=>Math.abs(v[1]-coarse.portVoltages[i][1]));assert.ok(Math.max(...refinement)<=.0005);summary.cases[name+suffix].refinement=refinement;assert.ok(q.beNumericalDissipationJ<=coarse.quality.beNumericalDissipationJ*1.01+1e-20);}else coarse=s;
  }
  const s=cases.get(name+' frequency').solution;assert.equal(s.status,'complete');assert.equal(s.parameters.length,21);assert.deepEqual(s.ports.map((p:any)=>p.name),['input','output']);
  assert.deepEqual(s.frequenciesHz,Array.from({length:21},(_,i)=>1e6+999e6*(i/20)));
  for(let i=0;i<21;i++){assert.equal(s.parameters[i].length,2);s.parameters[i].forEach((row:any[])=>assert.equal(row.length,2));const expected=mixedFrequencyReference(name,s.frequenciesHz[i]);for(let out=0;out<2;out++)for(let input=0;input<2;input++){close(s.parameters[i][out][input].real,expected[out][input][0],1e-8);close(s.parameters[i][out][input].imaginary,expected[out][input][1],1e-8);}}
  for(const [out,labelOut]of ['input','output'].entries())for(const [input,labelIn]of ['input','output'].entries()){
   const found=plots.filter(p=>p.title===name+':'+labelOut+'<-'+labelIn);assert.equal(found.length,1);const d=found[0].data;
   assert.equal(d.kind,'series');assert.equal(d.unit,'1');assert.equal(d.validity.status,s.conditional?'conditional':'valid');assert.deepEqual(d.axis,{name:'frequencyHz',unit:'Hz',values:s.frequenciesHz});
   assert.equal(d.values.length,21);d.values.forEach((v:unknown,i:number)=>close(v,Math.hypot(s.parameters[i][out][input].real,s.parameters[i][out][input].imaginary),1e-12));
  }
 }
 return summary;
}
