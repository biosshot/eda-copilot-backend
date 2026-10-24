import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {networkResource} from '../../src/analyze/analysis-dsl/frequency-model.ts';
/** Frozen pre-fit samples; no production fitter or regenerated reference data. */
export function resonantResource(name='mutual',count=1025){
 const base='tests/fixtures/analysis-resonant-checkpoint/',manifest=readFileSync(base+'frozen-manifest.json');
 const digest=(v:Buffer)=>createHash('sha256').update(v).digest('hex');
 assert.equal(digest(manifest),'342a2af63f6dc2140c987e6a56d6955178b41cfb30a824aa88cd2a3ae58796a2');
 const file=`${name}-${count}.json`,bytes=readFileSync(base+file);assert.equal(digest(bytes),JSON.parse(manifest.toString())[file]);
 const fixture=JSON.parse(bytes.toString());
 const n=fixture.resource.ports.length;
 return networkResource({...fixture.resource,analyses:['si','networkParameters'],validity:{status:'conditional',reason:'Authored frozen RLC coupon; no PCB extraction',domain:{bandHz:[0,1e8]}},data:{...fixture.resource.data,temporal:{method:'passive-resonant-y',normalizationOhm:50,modeTransform:fixture.checkpointContract.orthonormalModes,dcSlopeF:n===1?[1e-9]:[1e-9,2.5e-10],highFrequency:{inverseInductancePerH:n===1?[1e5]:[1e5,4e4]}}}});
}
type Z=[number,number];
const add=(a:Z,b:Z):Z=>[a[0]+b[0],a[1]+b[1]],sub=(a:Z,b:Z):Z=>[a[0]-b[0],a[1]-b[1]],mul=(a:Z,b:Z):Z=>[a[0]*b[0]-a[1]*b[1],a[0]*b[1]+a[1]*b[0]],div=(a:Z,b:Z):Z=>[(a[0]*b[0]+a[1]*b[1])/(b[0]**2+b[1]**2),(a[1]*b[0]-a[0]*b[1])/(b[0]**2+b[1]**2)];
const u=[[Math.cos(.37),-Math.sin(.37)],[Math.sin(.37),Math.cos(.37)]],components=[[50,1e-5,1e-9],[70,2.5e-5,2.5e-10]];
export function resonantPhysicalAc(f:number){
 if(f===0)return [[[0,0],[0,0]],[[0,0],[0,0]]] as Z[][];
 const w=2*Math.PI*f,Z:Z[][]=Array.from({length:2},()=>Array.from({length:2},()=>[0,0] as Z));
 // Full physical R/L/C^-1 congruence, then explicit matrix inverse.
 for(let i=0;i<2;i++)for(let j=0;j<2;j++)for(let m=0;m<2;m++){
  const[R,L,C]=components[m],a=u[i][m]*u[j][m];Z[i][j]=add(Z[i][j],[a*R,a*(w*L-1/(w*C))]);
 }
 const det=sub(mul(Z[0][0],Z[1][1]),mul(Z[0][1],Z[1][0]));
 return[[div(Z[1][1],det),div([-Z[0][1][0],-Z[0][1][1]],det)],[div([-Z[1][0][0],-Z[1][0][1]],det),div(Z[0][0],det)]];
}
export function resonantPhysicalTime(t:number){
 const current=[0,0];let energy=0;
 for(let m=0;m<2;m++){
  const[R,L,C]=components[m],k=1/L,b=1/(L*C),alpha=(R+25)/L/2,beta=Math.sqrt(b-alpha**2),p:Z=[-alpha,beta];
  const integral=(s:number)=>{s=Math.max(0,s);const E=div([Math.exp(-alpha*s)*Math.cos(beta*s)-1,Math.exp(-alpha*s)*Math.sin(beta*s)],p);return[(s-mul([1,-alpha/beta],E)[0])/b,E[1]/beta];};
  const x=[0,0];for(let i=0;i<2;i++){const delay=i===0?0:1.5e-7,amplitude=i===0?1:.4,a=integral(t-delay),b=integral(t-delay-2e-7);for(let j=0;j<2;j++)x[j]+=(a[j]-b[j])/2e-7*amplitude*u[i][m];}
  for(let i=0;i<2;i++)current[i]+=u[i][m]*k*x[1];energy+=.5*k*(b*x[0]**2+x[1]**2);
 }
 const source=[Math.min(1,Math.max(0,t/2e-7)),.4*Math.min(1,Math.max(0,(t-1.5e-7)/2e-7))];
 return{current,voltage:source.map((v,i)=>v-25*current[i]),energy};
}
