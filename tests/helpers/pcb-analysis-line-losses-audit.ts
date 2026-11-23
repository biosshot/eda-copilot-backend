import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {auditSectionOrigin,sectionResults,expectedSeries,parameterChange} from './pcb-analysis-cross-section-audit.ts';
const near=(a:number,b:number,rel=1e-8,abs=1e-20)=>{assert.ok(Number.isFinite(a)&&Number.isFinite(b));assert.ok(Math.abs(a-b)<=abs+rel*Math.abs(b),`${a} != ${b}`);};
type Z={real:number;imaginary:number};
const add=(a:Z,b:Z):Z=>({real:a.real+b.real,imaginary:a.imaginary+b.imaginary});
const mul=(a:Z,b:Z):Z=>({real:a.real*b.real-a.imaginary*b.imaginary,imaginary:a.real*b.imaginary+a.imaginary*b.real});
const scale=(a:Z,x:number):Z=>({real:a.real*x,imaginary:a.imaginary*x});
const div=(a:Z,b:Z):Z=>scale(mul(a,{real:b.real,imaginary:-b.imaginary}),1/(b.real*b.real+b.imaginary*b.imaginary));
const complexNear=(a:Z,b:Z)=>{near(a.real,b.real,1e-8,1e-10);near(a.imaginary,b.imaginary,1e-8,1e-10);};
const modal=(a:number[][],k:number)=>a.length===1?a[0][0]:(a[0][0]+a[1][1]+(k===0?1:-1)*(a[0][1]+a[1][0]))/2;
const MU0=1.25663706212e-6;
function surfaceImpedance(delta:number,sigma:number,sheetThickness?:number):Z {
 const thick=1/(sigma*delta);
 if(sheetThickness===undefined)return {real:thick,imaginary:thick};
 // Coth((1+i)t/delta), computed through exp(-2(1+i)t/delta).
 const angle=2*sheetThickness/delta, magnitude=Math.exp(-angle);
 const decay={real:magnitude*Math.cos(angle),imaginary:-magnitude*Math.sin(angle)};
 return scale(mul({real:1,imaginary:1},div(add({real:1,imaginary:0},decay),add({real:1,imaginary:0},scale(decay,-1)))),thick);
}
/** Independent ABCD oracle for short real-board sections, not production's
 * interface/reflected-wave representation. */
function abcd(z:Z,gamma:Z,length:number){
 const a=gamma.real*length,b=gamma.imaginary*length;
 const ch={real:Math.cosh(a)*Math.cos(b),imaginary:Math.sinh(a)*Math.sin(b)};
 const sh={real:Math.sinh(a)*Math.cos(b),imaginary:Math.cosh(a)*Math.sin(b)};
 const B=mul(z,sh),C=div(sh,z),den=add(scale(ch,2),add(scale(B,1/50),scale(C,50)));
 return {r:div(add(scale(B,1/50),scale(C,-50)),den),t:div({real:2,imaginary:0},den)};
}
const labels=[['R','Ohm/m','resistanceOhmPerM'],['Linternal','H/m','internalInductanceHPerM'],['G','S/m','conductanceSPerM'],['alpha','Np/m','alphaNpPerM'],['beta','rad/m','betaRadPerM'],['Zc real','Ohm','real'],['Zc imaginary','Ohm','imaginary'],['phase delay','s','phaseDelayS']];
function auditLossConvergence(current:any,previous:any,n:number,input:any){
 const a=current.lossIntegrals,b=previous.lossIntegrals,rows=current.lossConvergence.comparisons;assert.equal(rows.length,a.surfaces.length+a.regions.length+2*n);let max=0;
 function compare(A:number[][],B:number[][],record:any){
  const normalized=Array.from({length:n},()=>Array(n).fill(0));let entry=0;
  for(let i=0;i<n;i++)for(let j=0;j<n;j++){const absolute=Math.abs(A[i][j]-B[i][j]),diagonal=Math.sqrt(Math.max(A[i][i],B[i][i])*Math.max(A[j][j],B[j][j]));assert.ok(diagonal>0||absolute===0);const value=diagonal>0?absolute/diagonal:0;near(record.absoluteChanges[i][j],absolute,1e-8,1e-24);near(record.normalizedChanges[i][j],value,1e-8,1e-14);normalized[i][j]=value;entry=Math.max(entry,value);}
  const power=n===1?normalized[0][0]:.5*(Math.abs(normalized[0][0]+normalized[1][1])+Math.hypot(normalized[0][0]-normalized[1][1],2*normalized[0][1]));near(record.maxNormalizedChange,entry,1e-8,1e-14);near(record.normalizedPowerErrorBound,power,1e-8,1e-14);max=Math.max(max,power);
 }
 for(const [kind,key]of [['surfaces','gramPerM'],['regions','capacitanceFPerM']]as const){assert.equal(a[kind].length,b[kind].length);a[kind].forEach((v:any,i:number)=>{assert.equal(v.id,b[kind][i].id);const hits=rows.filter((r:any)=>r[kind==='surfaces'?'surface':'region']===v.id);assert.equal(hits.length,1);compare(v[key],b[kind][i][key],hits[0].comparison);});}
 function total(raw:any,conductor:boolean){const matrix=Array.from({length:n},()=>Array(n).fill(0));for(const r of raw[conductor?'surfaces':'regions']){const sigma=input.conductors?(r.id==='bottom'||r.id==='top'?input.conductors.referencePlanes[r.id].material.conductivitySM:input.conductors.signal.conductivitySM):null,weight=conductor?(sigma===null?0:1/Math.sqrt(sigma)):(r.id==='air'?0:input.material.lossTangent),m=r[conductor?'gramPerM':'capacitanceFPerM'];for(let i=0;i<n;i++)for(let j=0;j<n;j++)matrix[i][j]+=weight*m[i][j];}return matrix;}
 for(const [label,conductor]of [['conductor',true],['dielectric',false]]as const){const A=total(a,conductor),B=total(b,conductor);for(let k=0;k<n;k++){const x=modal(A,k),y=modal(B,k),difference=Math.abs(x-y),change=x===0&&y===0?0:difference/Math.max(Math.abs(x),Math.abs(y)),hits=rows.filter((r:any)=>r.coefficient===label&&r.mode===k);assert.equal(hits.length,1);near(hits[0].absoluteChange,difference,1e-8,1e-24);near(hits[0].relativeChange,change,1e-8,1e-14);max=Math.max(max,change);}}
 near(current.lossConvergence.maxNormalizedChange,max,1e-8,1e-14);
}
export function auditLineLosses(result:any,board:any,e:any){
 assert.equal(result.status,'complete',JSON.stringify(result.error));auditSectionOrigin(board,e);
 const foil=process.env.P7_SECTION_FOIL!=='0';
 const sections=sectionResults(result).filter((r:any)=>r.solution?.field);assert.equal(sections.length,4);
 const responses=sectionResults(result).filter((r:any)=>r.solution?.representation==='S'),cascade=e.tracks[0]==='dcc17162579803b7';assert.equal(responses.length,cascade?8:4);
 const plots=result.reports.flatMap((r:any)=>r.plots),descriptors=result.reports.flatMap((r:any)=>r.descriptors);
 const axis=Array.from({length:e.frequencyHz[2]},(_,i)=>e.frequencyHz[0]+i*(e.frequencyHz[1]-e.frequencyHz[0])/(e.frequencyHz[2]-1));
 const titles=new Set<string>();let maxComplexError=0;let maxPowerResidual=0;
 function series(title:string,unit:string,want:number[]){titles.add(title);const got=expectedSeries(plots,title,unit,axis);got.forEach((v,i)=>near(v,want[i],1e-8,1e-10));}
 function table(title:string,want:any){titles.add(title);const hits=plots.filter((p:any)=>p.title===title);assert.equal(hits.length,1,title);assert.deepEqual(hits[0].data,want);}
 for(const [index,z]of sections.entries()){
  const s=z.solution,p=s.parameters,n=e.tracks.length,name=e.modes[index],loss=index>0;
  const d=descriptors.find((d:any)=>d.id===s.lineId);assert.ok(d);assert.deepEqual(d.input,s.input);assert.equal(s.geometrySha256,createHash('sha256').update(JSON.stringify(d.input)).digest('hex'));
  for(const key of ['kind','widthMm','lengthMm','gapMm','copperThicknessMm','dielectricHeightMm'])if(e[key]!==undefined)assert.equal(s.input[key],e[key]);
  assert.equal(s.input.material.relativePermittivity,e.epsilon);assert.equal(s.input.material.lossTangent,index===1||index===3?e.lossTangent:0);
  assert.equal(s.validity.status,'conditional');assert.equal(z.rlgc.validity.status,'conditional');assert.equal(z.rlgc.unit,'1');assert.equal(z.characteristicImpedance.unit,'Ohm');assert.equal(z.delay.unit,'s');
  assert.deepEqual(s.frequenciesHz,axis);assert.equal(s.field.potentialsV.length,n);assert.equal(s.field.electricFieldsVPerM.length,n);
  near(s.convergence[0].stepM,e.mesh.initialMaxEdgeMm*.001);assert.equal(s.convergence[0].domainScale,e.mesh.initialDomainScale);
  assert.deepEqual(s.field,sections[0].solution.field,'all four modes use the same accepted PEC/dielectric field discretization');
  s.convergence.forEach((v:any,i:number)=>{if(i){near(v.maxRelativeChange,parameterChange(v.parameters,s.convergence[i-1].parameters),1e-8,1e-12);if(loss)auditLossConvergence(v,s.convergence[i-1],n,s.input);if(v.kind==='mesh'){assert.equal(v.level,s.convergence[i-1].level+1);near(v.stepM,s.convergence[i-1].stepM/2);}else{near(v.domainScale,2*s.convergence[i-1].domainScale);near(v.stepM,s.convergence[i-1].stepM);}}});
  for(const kind of ['mesh','domain']){const last=s.convergence.filter((v:any)=>v.kind===kind).at(-1);assert.ok(last.maxRelativeChange!==null&&last.maxRelativeChange<=.01);if(loss){assert.ok(last.lossConvergence.maxNormalizedChange<=.02);assert.equal(last.lossConvergence.comparisons.filter((v:any)=>v.surface).length,n+1);}}
  if(index<2)assert.equal(s.input.conductors,undefined);else {assert.equal(s.input.conductors.signal.conductivitySM,e.conductivitySM);assert.equal(s.input.conductors.referencePlanes.bottom.thicknessMm,e.referenceThicknessMm);assert.equal(s.input.conductors.referencePlanes.bottom.material.conductivitySM,e.conductivitySM);}
  const net=s.network;assert.equal(net.parameters.length,axis.length);assert.equal(net.ports.length,2*n);assert.equal(net.referenceOhm,50);
  const response=responses[index*(cascade?2:1)].solution;assert.deepEqual(response.parameters,net.parameters);assert.deepEqual(response.frequenciesHz,axis);assert.deepEqual(response.ports.map((p:any)=>p.name),net.ports);
  if(cascade){const c=responses[index*2+1].solution;assert.deepEqual(c.frequenciesHz,axis);assert.equal(c.parameters.length,axis.length);c.parameters.forEach((matrix:Z[][],k:number)=>{assert.equal(matrix.length,2);for(let i=0;i<2;i++){assert.equal(matrix[i].length,2);for(let j=0;j<2;j++)complexNear(matrix[i][j],scale(net.parameters[k][i][j],i!==j?.5:i===0?1:.25));}});}
  const calculated:any[]=loss?p.frequencyDependent.samples:axis.map(f=>({frequencyHz:f,modes:p.modes.map((m:any)=>({...m,resistanceOhmPerM:0,internalInductanceHPerM:0,conductanceSPerM:0,alphaNpPerM:0,betaRadPerM:2*Math.PI*f*m.delaySPerM,characteristicImpedanceOhm:{real:m.impedanceOhm,imaginary:0},phaseDelayS:m.delaySPerM*e.lengthMm*.001}))}));
  const matrix=(m:any)=>{assert.ok(Array.isArray(m));assert.equal(m.length,n);m.forEach((row:any)=>{assert.ok(Array.isArray(row));assert.equal(row.length,n);row.forEach((v:any)=>assert.ok(Number.isFinite(v)));});};
  for(const m of [p.capacitanceFPerM,p.vacuumCapacitanceFPerM,p.inductanceHPerM])matrix(m);
  assert.deepEqual(s.field.capacitance,p.capacitanceFPerM);assert.deepEqual(s.field.vacuumCapacitance,p.vacuumCapacitanceFPerM);
  for(let i=0;i<n;i++)for(let j=0;j<n;j++)near(p.vacuumCapacitanceFPerM[i].reduce((sum:number,v:number,k:number)=>sum+v*p.inductanceHPerM[k][j],0),i===j?1.25663706212e-6*8.8541878128e-12:0,1e-8,1e-26);
  if(loss&&n===1){assert.equal(z.characteristicImpedance.kind,'series');assert.deepEqual(z.characteristicImpedance.axis,{name:'frequencyHz',unit:'Hz',values:axis});assert.equal(z.characteristicImpedance.values.length,axis.length);z.characteristicImpedance.values.forEach((v:Z,k:number)=>complexNear(v,calculated[k].modes[0].characteristicImpedanceOhm));assert.equal(z.delay.kind,'series');assert.deepEqual(z.delay.axis,{name:'frequencyHz',unit:'Hz',values:axis});assert.equal(z.delay.values.length,axis.length);z.delay.values.forEach((v:number,k:number)=>near(v,calculated[k].modes[0].phaseDelayS));}
  else if(loss){assert.equal(z.characteristicImpedance.kind,'table');assert.deepEqual(z.characteristicImpedance.data.frequenciesHz,axis);const rows=z.characteristicImpedance.data.rows;assert.equal(rows.length,4*axis.length);for(let k=0;k<axis.length;k++)for(const [q,label,factor]of [[0,'even',1],[1,'odd',1],[1,'differential',2],[0,'common',.5]]as const){const hits=rows.filter((r:any)=>r.frequencyHz===axis[k]&&r.name===label);assert.equal(hits.length,1);complexNear(hits[0],scale(calculated[k].modes[q].characteristicImpedanceOhm,factor));}assert.equal(z.delay.kind,'table');assert.deepEqual(z.delay.data.frequenciesHz,axis);assert.equal(z.delay.data.rows.length,2*axis.length);for(let k=0;k<axis.length;k++)for(let q=0;q<2;q++){const row=z.delay.data.rows.filter((r:any)=>r.frequencyHz===axis[k]&&r.name===calculated[k].modes[q].name);assert.equal(row.length,1);near(row[0].value,calculated[k].modes[q].phaseDelayS);}}
  else if(n===1){assert.equal(z.characteristicImpedance.kind,'scalar');near(z.characteristicImpedance.value,p.modes[0].impedanceOhm);near(z.delay.value,p.modes[0].delaySPerM*e.lengthMm*.001);}
  if(loss){
   const integrals=s.lossIntegrals;assert.ok(integrals);assert.equal(integrals.surfaces.length,n+1);assert.equal(integrals.regions.length,2);assert.ok(integrals.maxCurrentNormalizationError<=1e-8);assert.ok(integrals.maxRegionalCapacitanceError<=1e-8);
   const totals=Array(n).fill(0);
   const allEdges=Array.from({length:n+1},()=>new Set<string>());s.field.triangles.forEach((t:number[])=>{for(const [a,b]of [[t[0],t[1]],[t[1],t[2]],[t[2],t[0]]]){const tag=s.field.boundary[a];if(tag!==0&&tag===s.field.boundary[b]&&(tag>0||Math.abs(s.field.nodes[a][1])<1e-16&&Math.abs(s.field.nodes[b][1])<1e-16))allEdges[tag>0?tag-1:n].add([Math.min(a,b),Math.max(a,b)].join(':'));}});
   integrals.surfaces.forEach((surface:any,q:number)=>{
    assert.equal(surface.id,q<n?(n===1?'signal':q===0?'positive':'negative'):'bottom');assert.equal(surface.currentAPerMPerA.length,n);assert.equal(surface.integratedCurrentAPerA.length,n);assert.ok(surface.edges.length>1);
    const nodeMap=new Map(surface.nodeIndices.map((v:number,i:number)=>[v,i]));assert.equal(nodeMap.size,surface.nodeIndices.length);
    for(let k=0;k<n;k++)assert.equal(surface.currentAPerMPerA[k].length,surface.nodeIndices.length);
    const edgeKeys=surface.edges.map(([a,b]:number[])=>[Math.min(a,b),Math.max(a,b)].join(':'));assert.equal(new Set(edgeKeys).size,edgeKeys.length);assert.deepEqual(new Set(edgeKeys),allEdges[q],'complete physical surface, including every corner-adjacent edge');
    const sums=Array(n).fill(0),gram=Array.from({length:n},()=>Array(n).fill(0));
    surface.edges.forEach(([ia,ib]:number[])=>{const a=s.field.nodes[ia],b=s.field.nodes[ib],len=Math.hypot(a[0]-b[0],a[1]-b[1]);assert.ok(len>0);const i=nodeMap.get(ia) as number,j=nodeMap.get(ib) as number;
     if(q===n){near(a[1],0);near(b[1],0);}else {const box=s.field.geometry.conductors[q].boundsM;assert.ok([0,2].some(k=>Math.abs(a[0]-box[k])<1e-15&&Math.abs(b[0]-box[k])<1e-15)||[1,3].some(k=>Math.abs(a[1]-box[k])<1e-15&&Math.abs(b[1]-box[k])<1e-15));}
     for(let k=0;k<n;k++){const A=surface.currentAPerMPerA[k][i],B=surface.currentAPerMPerA[k][j];assert.ok(Number.isFinite(A)&&Number.isFinite(B));sums[k]+=len*(A+B)/2;for(let l=0;l<n;l++){const C=surface.currentAPerMPerA[l][i],D=surface.currentAPerMPerA[l][j];gram[k][l]+=len*(2*A*C+A*D+B*C+2*B*D)/6;}}
    });
    for(let k=0;k<n;k++){near(sums[k],surface.integratedCurrentAPerA[k],1e-8,1e-9);totals[k]+=sums[k];if(q<n)near(sums[k],q===k?1:0,1e-8,1e-9);for(let l=0;l<n;l++)near(gram[k][l],surface.gramPerM[k][l]);}
   });totals.forEach(v=>near(v,0,0,1e-8));
   assert.equal(s.field.boundary.length,s.field.nodes.length);assert.equal(s.field.epsilonR.length,s.field.triangles.length);
   s.field.nodes.forEach((v:number[])=>{assert.equal(v.length,2);v.forEach(x=>assert.ok(Number.isFinite(x)));});
   for(let k=0;k<n;k++){assert.equal(s.field.potentialsV[k].length,s.field.nodes.length);assert.equal(s.field.electricFieldsVPerM[k].length,s.field.triangles.length);s.field.potentialsV[k].forEach((v:number,i:number)=>{assert.ok(Number.isFinite(v)&&v>=-1e-8&&v<=1+1e-8);if(s.field.boundary[i])near(v,s.field.boundary[i]===k+1?1:0,0,1e-12);});}
   const regional=integrals.regions.map(()=>Array.from({length:n},()=>Array(n).fill(0)));
   s.field.triangles.forEach((t:number[],q:number)=>{assert.equal(t.length,3);t.forEach(i=>assert.ok(Number.isInteger(i)&&i>=0&&i<s.field.nodes.length));const [a,b,c]=t.map(i=>s.field.nodes[i]),area=((b[0]-a[0])*(c[1]-a[1])-(c[0]-a[0])*(b[1]-a[1]))/2;assert.ok(area>0);const y=(a[1]+b[1]+c[1])/3,region=s.field.geometry.dielectrics.findIndex((d:any)=>y>d.yMinM&&y<d.yMaxM);assert.ok(region>=0);near(s.field.epsilonR[q],s.field.geometry.dielectrics[region].relativePermittivity);const grad=[[(b[1]-c[1])/(2*area),(c[0]-b[0])/(2*area)],[(c[1]-a[1])/(2*area),(a[0]-c[0])/(2*area)],[(a[1]-b[1])/(2*area),(b[0]-a[0])/(2*area)]];for(let i=0;i<n;i++){const A=s.field.electricFieldsVPerM[i][q];assert.equal(A.length,2);for(let d=0;d<2;d++)near(A[d],-t.reduce((sum,node,j)=>sum+s.field.potentialsV[i][node]*grad[j][d],0),1e-8,1e-7);for(let j=0;j<n;j++){const B=s.field.electricFieldsVPerM[j][q];regional[region][i][j]+=8.8541878128e-12*s.field.epsilonR[q]*area*(A[0]*B[0]+A[1]*B[1]);}}});
   regional.forEach((r:any,k:number)=>r.forEach((row:number[],i:number)=>row.forEach((v,j)=>near(v,integrals.regions[k].capacitanceFPerM[i][j]))));
   assert.deepEqual(p.frequencyDependent.frequenciesHz,axis);assert.equal(calculated.length,axis.length);
   assert.deepEqual(z.rlgc.data.samples,calculated);assert.deepEqual(z.rlgc.data.frequenciesHz,axis);
   for(const [key,unit]of [['resistanceOhmPerM','Ohm/m'],['externalInductanceHPerM','H/m'],['internalInductanceHPerM','H/m'],['totalInductanceHPerM','H/m'],['conductanceSPerM','S/m'],['capacitanceFPerM','F/m'],['alphaNpPerM','Np/m'],['betaRadPerM','rad/m'],['characteristicImpedanceOhm','Ohm'],['phaseDelayS','s']])assert.equal(z.rlgc.data.units[key],unit);
   for(const [k,sample]of calculated.entries()){
    near(sample.frequencyHz,axis[k]);const w=2*Math.PI*axis[k];assert.equal(sample.surfaceContributions.length,n+1);assert.equal(sample.regionContributions.length,2);assert.equal(sample.modes.length,n);
    for(const key of ['resistanceOhmPerM','externalInductanceHPerM','internalInductanceHPerM','totalInductanceHPerM','conductanceSPerM','capacitanceFPerM'])matrix(sample[key]);sample.surfaceContributions.forEach((s:any)=>matrix(s.resistanceOhmPerM));sample.regionContributions.forEach((r:any)=>{matrix(r.capacitanceFPerM);matrix(r.conductanceSPerM);});
    for(let i=0;i<n;i++)for(let j=0;j<n;j++){
     let R=0,Lin=0,G=0,C=0;
     sample.surfaceContributions.forEach((v:any,q:number)=>{
      const surface=integrals.surfaces[q],sigma=index>=2?e.conductivitySM:null;
      assert.equal(v.id,surface.id);assert.equal(v.conductivitySM,sigma);
      let impedance={real:0,imaginary:0};
      if(sigma!==null){
       const delta=Math.sqrt(2/(w*MU0*sigma));
       near(v.skinDepthM,delta);
       let sheetThickness: number | undefined;
       if(foil){
        const thicknessMm=q<n?s.input.copperThicknessMm:s.input.conductors.referencePlanes[v.id].thicknessMm;
        const thickness=thicknessMm*1e-3;
        const width=s.input.widthMm*1e-3;
        sheetThickness=q<n?Math.min(thickness,Math.max(thickness/2,width*thickness*surface.gramPerM[q][q])):thickness;
       }else assert.ok(delta<=v.minimumSkinDimensionM/10);
       impedance=surfaceImpedance(delta,sigma,sheetThickness);
      }
      const gram=surface.gramPerM[i][j];
      const resistance=impedance.real*gram;
      near(v.resistanceOhmPerM[i][j],resistance);
      R+=resistance;
      Lin+=impedance.imaginary*gram/w;
     });
     sample.regionContributions.forEach((v:any,q:number)=>{assert.equal(v.id,integrals.regions[q].id);const tangent=v.id==='air'?0:(index===1||index===3?e.lossTangent:0);assert.equal(v.lossTangent,tangent);near(v.capacitanceFPerM[i][j],regional[q][i][j]);near(v.conductanceSPerM[i][j],w*tangent*regional[q][i][j]);G+=v.conductanceSPerM[i][j];C+=v.capacitanceFPerM[i][j];});
     near(sample.resistanceOhmPerM[i][j],R);near(sample.internalInductanceHPerM[i][j],Lin);near(sample.externalInductanceHPerM[i][j],p.inductanceHPerM[i][j]);near(sample.totalInductanceHPerM[i][j],p.inductanceHPerM[i][j]+Lin);near(sample.conductanceSPerM[i][j],G);near(sample.capacitanceFPerM[i][j],C);near(C,p.capacitanceFPerM[i][j]);
    }
    sample.modes.forEach((m:any,q:number)=>{const R=modal(sample.resistanceOhmPerM,q),L=modal(sample.totalInductanceHPerM,q),G=modal(sample.conductanceSPerM,q),C=modal(sample.capacitanceFPerM,q);assert.ok(R>=0&&G>=0);const gamma={real:m.alphaNpPerM,imaginary:m.betaRadPerM},zc=m.characteristicImpedanceOhm;complexNear(mul(gamma,zc),{real:R,imaginary:w*L});complexNear(div(gamma,zc),{real:G,imaginary:w*C});near(m.phaseDelayS,m.betaRadPerM*e.lengthMm*.001/w);assert.ok(m.guards.resistanceToExternalReactance<=(foil?.3:.05)&&m.guards.conductanceToSusceptance<=.05);});
   }
   assert.ok(net.maxPowerBalanceResidual<=1e-8);assert.equal(net.powerBalances.length,axis.length*n);net.powerBalances.forEach((b:any)=>{near(b.incidentW-b.reflectedW-b.transmittedW,b.integratedDissipatedW,1e-8,1e-10);near(b.normalizedResidual,Math.abs(b.incidentW-b.reflectedW-b.transmittedW-b.integratedDissipatedW)/b.incidentW,1e-8,1e-14);maxPowerResidual=Math.max(maxPowerResidual,b.normalizedResidual);});
  }
  table(name+' RLGC',z.rlgc);table(name+' complex Zc',z.characteristicImpedance);table(name+' phase delay',z.delay);
  for(let q=0;q<n;q++)for(const [label,unit,key]of labels)series(name+' '+calculated[0].modes[q].name+' '+label,unit,calculated.map(v=>key==='real'||key==='imaginary'?v.modes[q].characteristicImpedanceOhm[key]:v.modes[q][key]));
  axis.forEach((_,k)=>{const modalS=calculated[k].modes.map((m:any)=>abcd(m.characteristicImpedanceOhm,{real:m.alphaNpPerM,imaginary:m.betaRadPerM},e.lengthMm*.001));assert.equal(net.parameters[k].length,2*n);for(let i=0;i<2*n;i++){assert.equal(net.parameters[k][i].length,2*n);for(let j=0;j<2*n;j++){let want={real:0,imaginary:0};for(let q=0;q<n;q++){const v=Math.floor(i/n)===Math.floor(j/n)?modalS[q].r:modalS[q].t;want=add(want,scale(v,n===1?1:q===1&&i%n!==j%n?-.5:.5));}const got=net.parameters[k][i][j];complexNear(got,want);maxComplexError=Math.max(maxComplexError,Math.hypot(got.real-want.real,got.imaginary-want.imaginary));}}});
  const ij=n===1?[[0,0,'S11'],[1,0,'S21']]:[[0,0,'S11'],[2,0,'S21'],[1,0,'Near crosstalk'],[3,0,'Far crosstalk']];
  for(const [i,j,label]of ij)for(const [suffix,unit]of [['magnitude','1'],['phase','deg']])series(name+' '+label+' '+suffix,unit,net.parameters.map((v:any)=>suffix==='magnitude'?Math.hypot(v[i][j].real,v[i][j].imaginary):Math.atan2(v[i][j].imaginary,v[i][j].real)*180/Math.PI));
  if(e.tracks[0]==='dcc17162579803b7')for(const [suffix,unit]of [['magnitude','1'],['phase','deg']])series(name+' Line + Attenuator Pad S21 '+suffix,unit,net.parameters.map((v:any)=>suffix==='magnitude'?.5*Math.hypot(v[1][0].real,v[1][0].imaginary):Math.atan2(v[1][0].imaginary,v[1][0].real)*180/Math.PI));
 }
 const last=sections.at(-1).solution.network.parameters;series('continued transmission magnitude','1',last.map((v:any)=>Math.hypot(v[e.tracks.length][0].real,v[e.tracks.length][0].imaginary)));
 assert.equal(plots.length,titles.size,'unexpected/duplicate plots');const checks=result.reports.flatMap((r:any)=>r.checks);assert.deepEqual(checks.map((c:any)=>c.status),e.expectedChecks);
 return {status:'pass',sections:4,frequencies:axis.length,bases:e.tracks.length,maxComplexError,maxPowerResidual,plots:plots.length};
}
