import test from 'node:test';
import assert from 'node:assert/strict';
// Independent stationary reaction/diffusion reference. The prescribed core is
// isothermal75C; outer sheet data are the exact sinh solution. No time capacity.
// Full P1 error extrema lie on triangle edges: error is affine in y and concave
// in x, so edge endpoints and analytic stationary points give continuous Linf.
const k=.3,t=.035e-3,coreThickness=1.53e-3,kt=k*t,g=1/(t/(2*k)+coreThickness/(2*k)),ell=Math.sqrt(kt/g),L=.9172439459396457e-3,H=L,rise=30,base=75,exact=x=>rise*Math.sinh((L-x)/ell)/Math.sinh(L/ell);
function solve(n:number,diagonal:'uniform'|'checkerboard',law:'lumped'|'consistent'){const points:number[][]=[];for(let j=0;j<=n;j++)for(let i=0;i<=n;i++)points.push([L*i/n,H*j/n]);const boundary=points.map(([x,y])=>x===0||x===L||y===0||y===H),free:number[]=[];points.forEach((p,i)=>{if(!boundary[i])free.push(i);});const index=new Map(free.map((v,i)=>[v,i])),N=free.length;assert.ok(N<=1024);const A=new Float64Array(N*N),rhs=new Float64Array(N),elements:{ids:number[];ps:number[][];area:number;matrix:number[][]}[]=[],support=new Float64Array(points.length),id=(i:number,j:number)=>j*(n+1)+i;
for(let j=0;j<n;j++)for(let i=0;i<n;i++){const[a,b,c,d]=[id(i,j),id(i+1,j),id(i+1,j+1),id(i,j+1)],slash=diagonal==='uniform'||(i+j)%2===0;for(const ids of slash?[[a,b,c],[a,c,d]]:[[a,b,d],[b,c,d]]){const ps=ids.map(i=>points[i]),[p,q,r]=ps,det=(q[0]-p[0])*(r[1]-p[1])-(r[0]-p[0])*(q[1]-p[1]),area=Math.abs(det)/2,grad=ps.map((p,j)=>[(ps[(j+1)%3][1]-ps[(j+2)%3][1])/det,(ps[(j+2)%3][0]-ps[(j+1)%3][0])/det]),matrix=ids.map((id,a)=>ids.map((id,b)=>kt*area*(grad[a][0]*grad[b][0]+grad[a][1]*grad[b][1])+(law==='lumped'?(a===b?g*area/3:0):g*area*(a===b?2:1)/12)));ids.forEach(i=>support[i]+=area/3);elements.push({ids,ps,area,matrix});for(let a=0;a<3;a++){const ia=index.get(ids[a]);if(ia===undefined)continue;for(let b=0;b<3;b++){const ib=index.get(ids[b]);if(ib===undefined)rhs[ia]-=matrix[a][b]*exact(points[ids[b]][0]);else A[ia*N+ib]+=matrix[a][b];}}}}
// Independent bounded dense Cholesky, not the production sparse/PCG engine.
const start=performance.now();for(let i=0;i<N;i++)for(let j=0;j<=i;j++){let value=A[i*N+j];for(let h=0;h<j;h++)value-=A[i*N+h]*A[j*N+h];if(i===j){assert.ok(value>0);A[i*N+j]=Math.sqrt(value);}else A[i*N+j]=value/A[j*N+j];}const x=new Float64Array(rhs);for(let i=0;i<N;i++){for(let j=0;j<i;j++)x[i]-=A[i*N+j]*x[j];x[i]/=A[i*N+i];}for(let i=N-1;i>=0;i--){for(let j=i+1;j<N;j++)x[i]-=A[j*N+i]*x[j];x[i]/=A[i*N+i];}const values=points.map((p,i)=>boundary[i]?exact(p[0]):x[index.get(i)!]),residual=new Float64Array(points.length);let continuousMax=0,nodalMax=0,where:unknown=null;const candidate=(position:number[],value:number,kind:string)=>{const error=Math.abs(value-exact(position[0]));if(error>continuousMax){continuousMax=error;where={pointMm:position.map(x=>x*1e3),fieldRiseK:value,exactRiseK:exact(position[0]),kind};}};
for(let i=0;i<points.length;i++){nodalMax=Math.max(nodalMax,Math.abs(values[i]-exact(points[i][0])));candidate(points[i],values[i],'vertex');}
for(const e of elements){for(let a=0;a<3;a++)for(let b=0;b<3;b++)residual[e.ids[a]]+=e.matrix[a][b]*values[e.ids[b]];for(let j=0;j<3;j++){const a=e.ids[j],b=e.ids[(j+1)%3],pa=points[a],pb=points[b],dx=pb[0]-pa[0];if(dx===0)continue;const slope=(values[b]-values[a])/dx,z=-slope*ell*Math.sinh(L/ell)/rise;if(z<1)continue;const criticalX=L-ell*Math.acosh(z),tau=(criticalX-pa[0])/dx;if(tau>0&&tau<1)candidate([criticalX,pa[1]+tau*(pb[1]-pa[1])],values[a]+tau*(values[b]-values[a]),'analytic edge stationary point');}}
const residualL2=Math.hypot(...free.map(i=>residual[i])),rhsNorm=Math.hypot(...rhs),normalized=residualL2/(1+rhsNorm);assert.ok(normalized<1e-11);let sink=0;for(let i=0;i<points.length;i++)sink+=g*support[i]*values[i];const boundaryHeat=points.reduce((v,p,i)=>v+(boundary[i]?residual[i]:0),0);assert.ok(Math.abs(sink-boundaryHeat)<1e-11);const center=id(n/2,n/2);return{n,diagonal,law,hMm:L/n*1e3,hOverEll:L/n/ell,nodes:points.length,freeNodes:N,minimumTriangleQuality:Math.sqrt(3)/2,maximumContinuousP1ErrorK:continuousMax,relativeContinuousError:continuousMax/rise,maximumNodalErrorK:nodalMax,minimumFieldRiseK:Math.min(...values),normalizedOriginalNodalResidual:normalized,powerBalanceW:sink-boundaryHeat,heatSinkW:sink,center:{pointMm:points[center].map(x=>x*1e3),supportAreaMm2:support[center]*1e6,temperatureC:base+values[center],exactC:base+exact(points[center][0])},maximum:where,elapsedMs:performance.now()-start,centerline:points.map((p,i)=>({p,v:values[i]})).filter(v=>Math.abs(v.p[1]-H/2)<1e-14).map(v=>({xMm:v.p[0]*1e3,temperatureC:base+v.v}))};}

test('stationary interlayer lumping and consistent P1 coupling converge to exact continuous sinh field',()=>{
 const rows=[];
 for(const n of [4,8,16,32]) for(const law of ['lumped','consistent']) for(const diagonal of ['uniform','checkerboard']) rows.push(solve(n,diagonal,law));
 for(const law of ['lumped','consistent']) for(const diagonal of ['uniform','checkerboard']){
  const levels=rows.filter(r=>r.law===law&&r.diagonal===diagonal);
  for(let j=1;j<levels.length;j++) assert.ok(levels[j].maximumContinuousP1ErrorK<levels[j-1].maximumContinuousP1ErrorK);
  assert.ok(levels.find(r=>r.n===16)!.relativeContinuousError<.02);
  assert.ok(levels.at(-1)!.relativeContinuousError<.005);
 }
});
test('stationary reaction connectivity changes support and consistent coupling is not a coarse-mesh positivity fix',()=>{
 const uniform=solve(4,'uniform','lumped'),checker=solve(4,'checkerboard','lumped');
 assert.equal(uniform.nodes,checker.nodes);
 assert.equal(uniform.minimumTriangleQuality,checker.minimumTriangleQuality);
 assert.ok(Math.abs(checker.center.supportAreaMm2/uniform.center.supportAreaMm2-4/3)<1e-12);
 assert.ok(Math.abs(checker.center.temperatureC-uniform.center.temperatureC)>.05);
 assert.ok(solve(2,'checkerboard','consistent').minimumFieldRiseK<0);
 assert.ok(solve(4,'checkerboard','consistent').relativeContinuousError>.1);
});
