export function validatePhysicalReference(c:any,r:any){
 const g=r?.geometry,l=c?.line,u=r?.uncertainty;
 if(!u||!Number.isFinite(u.relativeBound)||u.relativeBound<0||u.relativeBound>.02)throw Error('UNKNOWN_REFERENCE_UNCERTAINTY');
 const mismatch=(field:string):never=>{throw Error('PHYSICAL_REFERENCE_MISMATCH '+field);};
 const positive=(value:any,field:string)=>{if(!Number.isFinite(value)||value<=0)mismatch(field+' must be finite and positive');};
 // Only IEEE-754 representation tolerance (mm or dimensionless), never the 8% physical error allowance.
 const equal=(a:number,b:number,field:string)=>{if(Math.abs(a-b)>32*Number.EPSILON*Math.max(1,Math.abs(a),Math.abs(b)))mismatch(field);};
 if(!g||!l||l.kind!=='coupled-stripline')mismatch('coupled-stripline geometry required');
 for(const key of ['widthMm','gapMm','copperThicknessMm','relativePermittivity'])positive(g[key],'reference.'+key);
 for(const key of ['widthMm','gapMm','copperThicknessMm','topDielectricHeightMm','bottomDielectricHeightMm'])positive(l[key],'line.'+key);
 for(const key of ['topMaterial','bottomMaterial']){
  positive(l[key]?.relativePermittivity,key+'.relativePermittivity');
  if(!Number.isFinite(l[key]?.lossTangent)||l[key].lossTangent!==0)mismatch(key+'.lossTangent must be zero for lossless reference');
  equal(g.relativePermittivity,l[key].relativePermittivity,key+'.relativePermittivity');
 }
 for(const key of ['widthMm','gapMm','copperThicknessMm'])equal(g[key],l[key],key);
 for(const key of ['groundPlanesYMm','conductorYMm']){
  if(!Array.isArray(g[key])||g[key].length!==2||g[key].some((v:any)=>!Number.isFinite(v)))mismatch('reference.'+key+' requires two finite coordinates');
  if(g[key][1]<=g[key][0])mismatch('reference.'+key+' must increase');
 }
 // Reference convention: mm, lower ground y=0, y increases toward upper ground.
 equal(g.groundPlanesYMm[0],0,'lower ground origin');
 equal(g.conductorYMm[0],l.bottomDielectricHeightMm,'lower conductor face');
 equal(g.conductorYMm[1],l.bottomDielectricHeightMm+l.copperThicknessMm,'upper conductor face');
 equal(g.conductorYMm[1]-g.conductorYMm[0],g.copperThicknessMm,'reference conductor thickness');
 equal(g.groundPlanesYMm[1]-g.conductorYMm[1],l.topDielectricHeightMm,'upper dielectric clearance');
 if(!(Number.isFinite(u.meshDifference)&&u.meshDifference>=0&&u.meshDifference<.01&&Number.isFinite(u.domainDifference)&&u.domainDifference>=0&&u.domainDifference<.01))throw Error('REFERENCE_SENSITIVITY_UNVALIDATED');
 return 'validated';
}
import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
import {localCoreIdentity} from '../src/analyze/local-model-cache.ts';
import {verifyHash,compareValue,validateControl} from '../tests/helpers/p7-closure-reference.ts';
if(process.argv[1]?.replaceAll('\\','/').endsWith('/verify-p7-closure-baseline.ts')){
const manifest=JSON.parse(readFileSync('tests/fixtures/analysis-p7-closure/manifest.json','utf8'));
for(const c of [...manifest.cases,...manifest.supplementalControls,...(manifest.coupons??[])]){verifyHash(c.dsl,c.dslSha256);if(c.board)verifyHash(c.board,c.boardSha256);if(c.reference?.path)verifyHash(c.reference.path,c.reference.sha256);}
const folder=process.env.P7_OUTPUT_DIR??'debugging/p7-closure/'+Date.now();mkdirSync(folder,{recursive:true});
const c=manifest.supplementalControls.find((c:any)=>c.id==='coupled-stripline'),start=performance.now(),core=localCoreIdentity();
const result=await runAnalysisDsl({code:readFileSync(c.dsl,'utf8')},{timeoutMs:130000,maxOutputBytes:128*1024*1024});
writeFileSync(folder+'/dsl-result.json',JSON.stringify(result));
const evidence:any={core,wallSeconds:(performance.now()-start)/1000,manifest,resources:'Native resources retained in dsl-result; OS process-tree supervision required for accepted peak',status:'blocked',comparisons:[],realCases:manifest.cases.slice(1).map((x:any)=>({id:x.id,status:x.status,diagnostic:x.diagnostic}))};
try{
 if(result.status!=='complete')throw Error(JSON.stringify(result.error));
 const v:any=result.reports.flatMap((r:any)=>r.calculations).find((x:any)=>x.phase==='completed')?.result?.solution;
 validateControl(v,manifest.frequenciesHz,c.ports);
 const r=JSON.parse(readFileSync(c.reference.path,'utf8')).data,ds=r.effectivePermittivity.map((e:number)=>Math.sqrt(e)/299792458),zs=r.impedanceOhm;
 const cs=ds.map((d:number,i:number)=>d/zs[i]),ls=ds.map((d:number,i:number)=>d*zs[i]);
 for(const [key,ms,floor] of [['capacitanceFPerM',cs,1e-15],['inductanceHPerM',ls,1e-12]] as const){for(let i=0;i<2;i++)for(let j=0;j<2;j++){const ref=(ms[0]+(i===j?1:-1)*ms[1])/2;evidence.comparisons.push({metric:key,i,j,...compareValue(v.parameters[key][i][j],ref,floor)});}}
 for(let i=0;i<2;i++)for(const [key,expected,floor] of [['impedanceOhm',zs[i],.01],['delaySPerM',ds[i],1e-13]] as const)evidence.comparisons.push({metric:key,mode:i,...compareValue(v.parameters.modes[i][key],expected,floor)});
 // Independent lossless modal ABCD -> 50 Ohm S; transform even/odd waves to physical ports.
 const complex=(re:number,im:number)=>({real:re,imaginary:im});
 const mul=(a:any,b:any)=>complex(a.real*b.real-a.imaginary*b.imaginary,a.real*b.imaginary+a.imaginary*b.real);
 const div=(a:any,b:any)=>{const d=b.real*b.real+b.imaginary*b.imaginary;return complex((a.real*b.real+a.imaginary*b.imaginary)/d,(a.imaginary*b.real-a.real*b.imaginary)/d);};
 for(let f=0;f<manifest.frequenciesHz.length;f++){
  const modes=zs.map((z:number,i:number)=>{const theta=2*Math.PI*manifest.frequenciesHz[f]*.02*ds[i],den=complex(2*Math.cos(theta),(z/50+50/z)*Math.sin(theta));return {t:div(complex(2,0),den),r:div(complex(0,(z/50-50/z)*Math.sin(theta)),den)};});
  for(let i=0;i<4;i++)for(let j=0;j<4;j++){const k=(i<2)===(j<2)?'r':'t',sign=i%2===j%2?1:-1,ref=complex((modes[0][k].real+sign*modes[1][k].real)/2,(modes[0][k].imaginary+sign*modes[1][k].imaginary)/2),actual=v.network.parameters[f][i][j];const error=Math.hypot(actual.real-ref.real,actual.imaginary-ref.imaginary)/Math.max(Math.hypot(ref.real,ref.imaginary),1e-4);evidence.comparisons.push({metric:'complex S',frequencyHz:manifest.frequenciesHz[f],i,j,actual,reference:ref,error,status:error<=.08?'pass':'fail'});}
 }
 evidence.comparisonPassed=evidence.comparisons.every((x:any)=>x.error+0.02<=0.08);evidence.physicalValidation=validatePhysicalReference(c,JSON.parse(readFileSync(c.reference.path,'utf8')));evidence.status=evidence.comparisonPassed&&evidence.physicalValidation==='validated'?'pass':'fail';evidence.native=v;
}catch(error){evidence.diagnostic=String(error);}
writeFileSync(folder+'/comparison.json',JSON.stringify(evidence,null,2));console.log(JSON.stringify({folder,status:evidence.status,diagnostic:evidence.diagnostic,maxError:Math.max(...evidence.comparisons.map((x:any)=>x.error))}));if(evidence.status!=='pass')process.exitCode=1;

}
