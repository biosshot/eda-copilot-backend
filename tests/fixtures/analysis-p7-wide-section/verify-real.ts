import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {executeScenario} from '../../../src/analyze/analysis-dsl/scenario.ts';
import {localRequest} from '../../../src/analyze/analysis-dsl/local-request.ts';
import {boardSectionCandidates} from '../../../src/analyze/board-channel-plan.ts';
import {extractWideBoardSectionCoefficients} from '../../../src/analyze/segmented-channel.ts';
import {createProblemExecutor} from '../../../src/analyze/execute-problem.ts';
const manifest=JSON.parse(readFileSync('tests/fixtures/analysis-p7-closure/manifest.json','utf8'));const out=process.env.P7_OUTPUT_DIR??'debugging/p7-integration';mkdirSync(out,{recursive:true});
for(const id of (process.env.P7_BOARD_IDS??'STM32G0_CAN_RS485').split(',')){
 const e=manifest.cases.find((v:any)=>v.id===id),board=JSON.parse(readFileSync(e.board,'utf8')),code=readFileSync(e.dsl,'utf8').split('a.plot(')[0].replace(/(const (?:input|output)=a.port\(\{[^\n]*)(\}\);)/g,'$1,positiveLayer:"TOP",negativeLayer:"TOP"$2');
 const r=await executeScenario({board,code},{filename:'real-section.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{
  const started=performance.now();let contacts:any;const base=localRequest(task as any,p=>contacts=p.contacts);const selection=boardSectionCandidates(board,base.stack,contacts);const execute=createProblemExecutor(true);const previews=[];
  for(const c of selection.candidates.slice(0,15)){
   const returns=selection.g.regions.filter(r=>selection.g.islands.find(i=>i.id===c.physicalReturnIsland)?.region_ids.includes(r.id));
   const inside=(ring:number[][],p:number[])=>{let yes=false;for(let i=0,j=ring.length-1;i<ring.length;j=i++){const a=ring[i],b=ring[j];if((a[1]>p[1])!==(b[1]>p[1])&&p[0]<(b[0]-a[0])*(p[1]-a[1])/(b[1]-a[1])+a[0])yes=!yes;}return yes;};const chosen=returns.find(r=>r.layer==='BOTTOM'&&inside(r.polygon.outer,c.origin)&&!r.polygon.holes.some(h=>inside(h,c.origin)))??returns.find(r=>inside(r.polygon.outer,c.origin)&&!r.polygon.holes.some(h=>inside(h,c.origin)))??returns[0];const slice={atMm:c.origin,transverseUnit:c.transverseUnit,halfWidthMm:c.widthsMm.at(-1),airMarginMm:c.airMarginMm,referenceLayer:chosen.layer,referenceSourcePath:chosen.source.path,referenceXMm:0};
   try{const s:any=execute({kind:'board-section',boardJson:selection.boardJson,stack:base.stack,slice});previews.push({candidate:c,sourceMapping:s.sourceMapping,geometry:s.geometry,slice});if(process.env.P7_REAL_SOLVE==='1'&&s.sourceMapping.filter((m:any)=>m.role==='signal').length<=2)break;}catch(e:any){previews.push({candidate:c,error:e.message,slice});}
  }
  writeFileSync(out+'/'+id+'-geometry.json',JSON.stringify({base,contacts,candidates:selection.candidates,previews,seconds:(performance.now()-started)/1000}));console.log(JSON.stringify({id,candidates:selection.candidates.length,previews:previews.map((p:any)=>({run:p.candidate.runId,length:p.candidate.lengthMm,mapping:p.sourceMapping,error:p.error}))}));
  if(process.env.P7_REAL_SOLVE==='1'){
   const chosen:any=previews.find((p:any)=>p.geometry&&p.sourceMapping.filter((m:any)=>m.role==='signal').length<=2);if(!chosen)throw Error('No supported certified physical candidate');
   const c=chosen.candidate,w=c.widthsMm.at(-1),air=c.airMarginMm,area=4e-8;
   const make=(axis:any,width:number,margin:number,mesh:number)=>({axis,slice:{...chosen.slice,halfWidthMm:width,airMarginMm:margin},maxTriangleAreaM2:mesh});
   const slices=[make('baseline',c.widthsMm.at(-2),air*1.5,area),make('physical-width',w,air*1.5,area),make('baseline',w,air,area),make('air-boundary',w,air*1.5,area),make('baseline',w,air*1.5,area*2),make('mesh',w,air*1.5,area)];
   const input={boardJson:selection.boardJson,stack:base.stack,slices,frequencies:[1e6,1e7,1e8],lengthMm:c.lengthMm};
   writeFileSync(out+'/real-input.json',JSON.stringify({id,selection:c,input,contacts,policy:selection.policy,geometrySeconds:(performance.now()-started)/1000}));
   const result=extractWideBoardSectionCoefficients(input,{maxSeconds:180,maxMemoryMiB:1024});writeFileSync(out+'/real-result.json',JSON.stringify(result));
   if(result.status!=='coefficients-only')throw Error('REAL_SECTION_UNRESOLVED');
   const warm=extractWideBoardSectionCoefficients(input,{maxSeconds:180,maxMemoryMiB:1024});writeFileSync(out+'/real-warm.json',JSON.stringify(warm));
   writeFileSync(out+'/comparison.json',JSON.stringify({status:'pass',scope:'Real STM32 coefficients only; D/full channel unvalidated',endToEndSeconds:(performance.now()-started)/1000}));
  }
  return {id:'geometry',status:'complete',method:'finite candidate',diagnostics:[],assumptions:[],metrics:{}};
 });if(r.status!=='complete')throw Error(JSON.stringify(r.error));
}


