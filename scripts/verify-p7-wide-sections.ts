import {mkdirSync,writeFileSync,readFileSync} from 'node:fs';
import {selectSectionDomain,type SectionDomainStep} from '../src/analyze/section-domain.ts';
if(process.env.P7_REAL_BOARD==='1'){await import('../tests/fixtures/analysis-p7-wide-section/verify-real.ts');}else{
const out=process.env.P7_OUTPUT_DIR??'debugging/p7-wide-section/'+Date.now();mkdirSync(out,{recursive:true});process.env.PCB_ANALYSIS_CACHE_DIR=out+'/cache';
const manifest=JSON.parse(readFileSync('tests/fixtures/analysis-p7-wide-section/manifest.json','utf8'));
const rect=(x:number,y:number,X:number,Y:number)=>({outerM:[[x,y],[X,y],[X,Y],[x,Y]],holesM:[]});
const circle=(x:number)=>({outerM:Array.from({length:64},(_,i)=>{const t=i*Math.PI/32;return [x+1e-5*Math.cos(t),.0003+1e-5*Math.sin(t)];}),holesM:[]});
const geometry=(w:number,air:number,area:number)=>({domain:rect(-air,-air,air,air),conductors:[{id:'left',role:'signal',contour:circle(-.00015)},{id:'right',role:'signal',contour:circle(.00015)},{id:'physical-return',role:'reference',contour:rect(-w,-.000035,w,0)}],dielectrics:[{id:'air',relativePermittivity:1,contour:rect(-air,-air,air,air)}],maxTriangleAreaM2:area,maxNodes:100000,maxSeconds:120,maxMemoryMib:1024});
const physicalIdentity={manifest,fullReturnHalfWidthMm:4};const conductorIdentity=['left','right','physical-return'];
const cases:[SectionDomainStep['axis'],number,number,number][]=[['baseline',.002,.007,2e-8],['physical-width',.004,.007,2e-8],['baseline',.004,.006,2e-8],['air-boundary',.004,.007,2e-8],['baseline',.004,.007,4e-8],['mesh',.004,.007,2e-8]];
const steps:SectionDomainStep[]=cases.map(([axis,w,air,area])=>({axis,physicalIdentity,conductorIdentity,request:{line:{kind:'polygon-section',geometry:geometry(w,air,area),lengthMm:10},frequencies:[1e6,1e7,1e8],mesh:{maxRefinements:2}}}));
writeFileSync(out+'/input.json',JSON.stringify({manifest,steps},null,2));
const result=selectSectionDomain(steps);writeFileSync(out+'/result.json',JSON.stringify(result,null,2));
if(result.status!=='coefficients-only')throw Error('Unresolved numerical sensitivity');
const reference=JSON.parse(readFileSync('tests/fixtures/analysis-p7-wide-section/image-reference.json','utf8'));const p=result.report.parameters;const errors:any={};for(const [k,key,floor]of [['C','capacitanceFPerM',1e-15],['L','inductanceHPerM',1e-12]] as const)errors[k]=Math.max(...reference[k].flat().map((v,i)=>Math.abs(p[key].flat()[i]-v)/Math.max(Math.abs(v),floor)));
errors.Z=Math.max(...reference.Z.sort((a:number,b:number)=>a-b).map((v,i)=>Math.abs(result.audit.at(-1).observables.Z[i]-v)/v));
errors.complexS=Math.max(...reference.S.flat(2).map((v:any,i:number)=>{const a=result.report.network.parameters.flat(2)[i];return Math.hypot(a.real-v.real,a.imaginary-v.imaginary)/Math.max(Math.hypot(v.real,v.imaginary),1e-4);}));
const warm=selectSectionDomain(steps);writeFileSync(out+'/warm.json',JSON.stringify(warm,null,2));
const status=Math.max(...Object.values(errors) as number[])+.03<=.08?'pass':'fail';writeFileSync(out+'/comparison.json',JSON.stringify({status,reference,errors,uncertainty:.03,scope:manifest.scope},null,2));console.log(JSON.stringify({out,status,errors,audit:result.audit}));if(status!=='pass')throw Error('Independent accuracy refused');





}
