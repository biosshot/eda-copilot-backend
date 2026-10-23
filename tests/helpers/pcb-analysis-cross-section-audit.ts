import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
export const sectionResults=(r:any)=>r.reports.flatMap((r:any)=>r.calculations).filter((c:any)=>c.phase==='completed').map((c:any)=>c.result);
const near=(a:number,b:number,rel=1e-8,abs=1e-20)=>{assert.ok(Number.isFinite(a)&&Number.isFinite(b));assert.ok(Math.abs(a-b)<=abs+rel*Math.abs(b),`${a} != ${b}`);};
export function auditSectionOrigin(board:any,e:any){
 const tracks=[...board.copper.fixed.tracks,...board.copper.editable.tracks];assert.equal(e.conditional,true);
 e.tracks.forEach((id:string,k:number)=>{const hits=tracks.filter(t=>t.id===id);assert.equal(hits.length,1);const t=hits[0];assert.equal(t.net,e.net[k]);assert.equal(t.layer,e.layer);near(t.widthMm,e.widthMm);assert.equal(t.points.length,2);
  const [a,b]=t.points,dx=b.x-a.x,dy=b.y-a.y,l=Math.hypot(dx,dy);for(const [x,y]of e.segment[k]){near((x-a.x)*dy-(y-a.y)*dx,0,0,1e-8);const s=((x-a.x)*dx+(y-a.y)*dy)/(l*l);assert.ok(s>=-1e-9&&s<=1+1e-9);}const [p,q]=e.segment[k];near(Math.hypot(p[0]-q[0],p[1]-q[1]),e.lengthMm);
 });
 if(e.tracks.length===2){const [a,b]=e.segment[0],[c,d]=e.segment[1],dx=b[0]-a[0],dy=b[1]-a[1];near(dx*(d[1]-c[1])-dy*(d[0]-c[0]),0,0,1e-8);near(Math.abs((c[0]-a[0])*dy-(c[1]-a[1])*dx)/Math.hypot(dx,dy)-e.widthMm,e.gapMm);}
}
type Z={real:number;imaginary:number};const add=(a:Z,b:Z):Z=>({real:a.real+b.real,imaginary:a.imaginary+b.imaginary});const mul=(a:Z,b:Z):Z=>({real:a.real*b.real-a.imaginary*b.imaginary,imaginary:a.real*b.imaginary+a.imaginary*b.real});const scale=(a:Z,v:number):Z=>({real:a.real*v,imaginary:a.imaginary*v});
/** Independent reflected-wave series, used only as an algebraic audit oracle. */
function reflection(z:number,delay:number,l:number,f:number){const rho=(z-50)/(z+50),t=-2*Math.PI*f*delay*l,p={real:Math.cos(t),imaginary:Math.sin(t)},q=scale(mul(p,p),rho*rho);let term={real:1,imaginary:0},sum=term;for(let k=0;k<2000;k++){term=mul(term,q);sum=add(sum,term);if(Math.hypot(term.real,term.imaginary)<1e-17)break;}return {reflection:scale(mul(add({real:1,imaginary:0},scale(mul(p,p),-1)),sum),rho),transmission:scale(mul(p,sum),1-rho*rho)};}
function expectedSeries(plots:any[],title:string,unit:string,axis:number[]){
 const hits=plots.filter(p=>p.title===title);assert.equal(hits.length,1,`expected exactly one plot ${title}`);
 const plot=hits[0],data=plot.data;assert.equal(plot.kind,'line',title);assert.equal(data.kind,'series',title);assert.equal(data.unit,unit,title);assert.equal(data.validity.status,'conditional',title);
 assert.deepEqual(data.axis,{name:'frequencyHz',unit:'Hz',values:axis},title);assert.ok(Array.isArray(data.values),title);assert.equal(data.values.length,axis.length,`incomplete series ${title}`);
 for(let i=0;i<axis.length;i++)assert.ok(Number.isFinite(data.values[i]),`invalid sample ${i} in ${title}`);
 return data.values as number[];
}
export function auditCrossSection(result:any,board:any,e:any){
 assert.equal(result.status,'complete',JSON.stringify(result.error));auditSectionOrigin(board,e);
 const rr=sectionResults(result),z=rr.find((r:any)=>r.kind==='impedance'||r.solution?.field),s=z?.solution;assert.ok(s?.field,'missing FEM field');const p=s.parameters,n=e.tracks.length;
 const descriptors=result.reports.flatMap((r:any)=>r.descriptors);const d=descriptors.find((d:any)=>d.id===s.lineId);assert.ok(d);assert.deepEqual(d.input,s.input);
 assert.equal(s.geometrySha256,createHash('sha256').update(JSON.stringify(d.input)).digest('hex'));
 for(const key of ['kind','widthMm','lengthMm','gapMm','copperThicknessMm','dielectricHeightMm'])if(e[key]!==undefined){if(typeof e[key]==='number')near(s.input[key],e[key]);else assert.equal(s.input[key],e[key]);}
 near(s.input.material.relativePermittivity,e.epsilon);assert.equal(s.input.material.lossTangent,0);assert.match(s.input.material.state,/hypothesis/);assert.equal(s.validity.status,'conditional');assert.equal(z.rlgc.validity.status,'conditional');assert.equal(z.rlgc.unit,'1');assert.equal(z.characteristicImpedance.unit,'Ohm');assert.equal(z.delay.unit,'s');assert.ok(s.assumptions.some((v:string)=>v.includes('PEC')));assert.ok(s.assumptions.some((v:string)=>v.includes('natural')));
 const ids=n===1?['signal']:['positive','negative'];assert.deepEqual(p.conductorOrder,ids);assert.deepEqual(s.field.geometry.conductors.map((c:any)=>c.id),ids);
 const geom=s.field.geometry;geom.conductors.forEach((c:any,i:number)=>{near(c.boundsM[2]-c.boundsM[0],e.widthMm*1e-3);near(c.boundsM[3]-c.boundsM[1],e.copperThicknessMm*1e-3);near(c.boundsM[1],e.dielectricHeightMm*1e-3);assert.equal(c.role,'signal PEC');if(i)near(c.boundsM[0]-geom.conductors[0].boundsM[2],e.gapMm*1e-3);});
 assert.deepEqual(s.geometry,geom);assert.equal(geom.referenceYM.length,1);near(geom.referenceYM[0],0);near(geom.dielectrics.find((d:any)=>d.id==='bottom').relativePermittivity,e.epsilon);
 const field=s.field,N=field.nodes.length,T=field.triangles.length;assert.ok(N>10&&T>10);assert.equal(field.boundary.length,N);assert.equal(field.epsilonR.length,T);assert.equal(field.potentialsV.length,n);assert.equal(field.electricFieldsVPerM.length,n);
 const c=Array.from({length:n},()=>Array(n).fill(0));
 for(let k=0;k<n;k++){assert.equal(field.potentialsV[k].length,N);assert.equal(field.electricFieldsVPerM[k].length,T);field.potentialsV[k].forEach((v:number,i:number)=>{assert.ok(Number.isFinite(v)&&v>=-1e-8&&v<=1+1e-8);if(field.boundary[i]!==0)near(v,field.boundary[i]===k+1?1:0,0,1e-12);});}
 field.triangles.forEach((t:number[],i:number)=>{assert.equal(t.length,3);t.forEach(v=>assert.ok(Number.isInteger(v)&&v>=0&&v<N));const [a,b,d]=t.map(j=>field.nodes[j]);const det=(b[0]-a[0])*(d[1]-a[1])-(d[0]-a[0])*(b[1]-a[1]);assert.ok(det>0);const y=(a[1]+b[1]+d[1])/3,mat=geom.dielectrics.find((r:any)=>y>r.yMinM&&y<r.yMaxM);assert.ok(mat);near(field.epsilonR[i],mat.relativePermittivity);
  const grad=[[(b[1]-d[1])/det,(d[0]-b[0])/det],[(d[1]-a[1])/det,(a[0]-d[0])/det],[(a[1]-b[1])/det,(b[0]-a[0])/det]];
  for(let k=0;k<n;k++){const E=field.electricFieldsVPerM[k][i];assert.equal(E.length,2);for(let axis=0;axis<2;axis++)near(E[axis],-t.reduce((sum,j,q)=>sum+field.potentialsV[k][j]*grad[q][axis],0),1e-8,1e-7);
   for(let j=0;j<n;j++){const F=field.electricFieldsVPerM[j][i];c[k][j]+=8.8541878128e-12*field.epsilonR[i]*det/2*(E[0]*F[0]+E[1]*F[1]);}
  }
 });
 for(let i=0;i<n;i++)for(let j=0;j<n;j++){near(c[i][j],p.capacitanceFPerM[i][j]);near(field.capacitance[i][j],p.capacitanceFPerM[i][j]);near(field.vacuumCapacitance[i][j],p.vacuumCapacitanceFPerM[i][j]);near(p.capacitanceFPerM[i][j],p.capacitanceFPerM[j][i]);assert.ok(i===j?p.capacitanceFPerM[i][j]>0:p.capacitanceFPerM[i][j]<0);if(i!==j)assert.ok(p.inductanceHPerM[i][j]>0);const x=p.vacuumCapacitanceFPerM[i].reduce((s:number,v:number,k:number)=>s+v*p.inductanceHPerM[k][j],0);near(x,i===j?1.25663706212e-6*8.8541878128e-12:0,1e-8,1e-26);}
 for(const q of [field.quality.actual,field.quality.vacuum])for(const key of ['normalizedResidual','normalizedSymmetry','normalizedChargeBalance','normalizedEnergyReactionError'])assert.ok(Number.isFinite(q[key])&&q[key]<=1e-8);
 const conv=s.convergence;
 function parameterChange(a:any,b:any){let delta=0;for(const key of ['capacitanceFPerM','vacuumCapacitanceFPerM','inductanceHPerM']){const floor=.001*Math.max(...a[key].map((r:number[],i:number)=>Math.abs(r[i])));a[key].forEach((row:number[],i:number)=>row.forEach((v:number,j:number)=>{delta=Math.max(delta,Math.abs(v-b[key][i][j])/Math.max(Math.abs(v),floor));}));}a.modes.forEach((m:any,i:number)=>{for(const key of ['impedanceOhm','delaySPerM'])delta=Math.max(delta,Math.abs(m[key]-b.modes[i][key])/Math.abs(m[key]));});return delta;}
 conv.forEach((v:any,i:number)=>{if(i===0){assert.equal(v.maxRelativeChange,null);assert.equal(v.level,0);}else {near(v.maxRelativeChange,parameterChange(v.parameters,conv[i-1].parameters),1e-8,1e-12);if(v.kind==='mesh'){assert.equal(v.level,conv[i-1].level+1);near(v.stepM,conv[i-1].stepM/2);assert.equal(v.domainScale,conv[i-1].domainScale);}else {near(v.stepM,conv[i-1].stepM);near(v.domainScale,conv[i-1].domainScale*2);}}});
 assert.ok(conv.some((v:any)=>v.kind==='mesh')&&conv.some((v:any)=>v.kind==='domain'));for(const kind of ['mesh','domain']){const last=conv.filter((v:any)=>v.kind===kind).at(-1);assert.ok(last.maxRelativeChange!==null&&last.maxRelativeChange<=.01);}
 assert.deepEqual(conv.at(-1).parameters,p);assert.equal(s.convergenceTolerance,.01);assert.equal(s.mutualAbsoluteFloorRelativeToSelf,.001);
 p.modes.forEach((m:any,i:number)=>{const sign=i===0?1:-1,cm=p.capacitanceFPerM[0][0]+(n===2?sign*p.capacitanceFPerM[0][1]:0),lm=p.inductanceHPerM[0][0]+(n===2?sign*p.inductanceHPerM[0][1]:0);near(m.impedanceOhm,Math.sqrt(lm/cm));near(m.delaySPerM,Math.sqrt(lm*cm));});if(n===2){near(p.differentialOhm,2*p.modes[1].impedanceOhm);near(p.commonOhm,p.modes[0].impedanceOhm/2);}
 const net=s.network,ports=n===1?['input','output']:['positiveInput','negativeInput','positiveOutput','negativeOutput'];assert.deepEqual(net.ports,ports);const axis=s.frequenciesHz;assert.equal(axis.length,e.frequencyHz[2]);near(axis[0],e.frequencyHz[0]);near(axis.at(-1),e.frequencyHz[1]);assert.equal(net.parameters.length,axis.length);assert.equal(net.referenceOhm,50);assert.match(net.phaseConvention,/exp\(-j/);
 let maxComplexError=0;
 axis.forEach((f:number,k:number)=>{const modes=p.modes.map((m:any)=>reflection(m.impedanceOhm,m.delaySPerM,e.lengthMm*1e-3,f));for(let i=0;i<2*n;i++)for(let j=0;j<2*n;j++){let want={real:0,imaginary:0};for(let m=0;m<n;m++)want=add(want,scale(Math.floor(i/n)===Math.floor(j/n)?modes[m].reflection:modes[m].transmission,n===1?1:m===1&&i%n!==j%n?-.5:.5));const got=net.parameters[k][i][j];near(got.real,want.real,1e-8,1e-10);near(got.imaginary,want.imaginary,1e-8,1e-10);maxComplexError=Math.max(maxComplexError,Math.hypot(got.real-want.real,got.imaginary-want.imaginary));}});
 const response=rr.find((r:any)=>r.solution?.representation==='S');assert.ok(response);assert.deepEqual(response.solution.parameters,net.parameters);assert.deepEqual(response.solution.ports.map((p:any)=>p.name),ports);
 const plots=result.reports.flatMap((r:any)=>r.plots),labels=n===1?['S11','S21']:['S11','S21','Near crosstalk','Far crosstalk'],ij=n===1?[[0,0],[1,0]]:[[0,0],[2,0],[1,0],[3,0]];
 labels.forEach((label,k)=>{for(const suffix of ['magnitude','phase']){const values=expectedSeries(plots,label+' '+suffix,suffix==='phase'?'deg':'1',axis);for(let f=0;f<axis.length;f++){const z=net.parameters[f][ij[k][0]][ij[k][1]];near(values[f],suffix==='phase'?Math.atan2(z.imaginary,z.real)*180/Math.PI:Math.hypot(z.real,z.imaginary),1e-8,1e-10);}}});
 if(e.tracks.length===1&&e.tracks[0]==='dcc17162579803b7'){for(const suffix of ['magnitude','phase']){const values=expectedSeries(plots,'Line + Attenuator Pad S21 '+suffix,suffix==='phase'?'deg':'1',axis);for(let k=0;k<axis.length;k++){const z=net.parameters[k][1][0];near(values[k],suffix==='phase'?Math.atan2(z.imaginary,z.real)*180/Math.PI:.5*Math.hypot(z.real,z.imaginary),1e-8,1e-10);}}}
 const checks=result.reports.flatMap((r:any)=>r.checks);assert.deepEqual(checks.map((c:any)=>c.status),e.expectedChecks);assert.match(checks[0].name,/expected fail/);assert.match(checks[1].name,/continued/);
 return {status:'pass',nodes:N,triangles:T,bases:n,plotArrays:plots.filter((p:any)=>p.data.kind==='series').length,maxComplexError,meshLevels:conv.filter((c:any)=>c.kind==='mesh').length,domainLevels:conv.filter((c:any)=>c.kind==='domain').length,quality:field.quality,modes:p.modes};
}
