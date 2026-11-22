import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {createCanvas} from 'canvas';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {localRequest} from '../src/analyze/analysis-dsl/local-request.ts';
import {boardChannelPlan,boardSectionCandidates} from '../src/analyze/board-channel-plan.ts';
const folder=process.env.P7_OUTPUT_DIR??'debugging/p7-b2-c1-geometry';mkdirSync(folder,{recursive:true});
const manifest=JSON.parse(readFileSync('tests/fixtures/analysis-p7-closure/manifest.json','utf8'));
for(const id of ['STM32G0_CAN_RS485','NanoScope','ThunderF722']){
 const entry=manifest.cases.find((c:any)=>c.id===id);if(!entry)throw Error(id);
 const board=JSON.parse(readFileSync(entry.board,'utf8')),code=readFileSync(entry.dsl,'utf8').split('a.plot(')[0].replace(/(const (?:input|output)=a.port\(\{[^\n]*)(\}\);)/g,'$1,positiveLayer:"TOP",negativeLayer:"TOP"$2'),start=performance.now();let evidence:any;
 const result=await executeScenario({board,code},{filename:'wide-return-geometry.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{
  let contacts:any;const base=localRequest(task as any,p=>contacts=p.contacts);
  try{const plan=boardChannelPlan(board,base,contacts);evidence={status:'planned',audit:plan.audit};}
  catch(e:any){if(!e.corridorAudit)throw e;evidence={status:'refused',code:e.code,audit:e.corridorAudit};}
  const finite=boardSectionCandidates(board,base.stack,contacts);evidence.finiteCandidates=finite.candidates;
  return {id:'geometry',status:'complete',method:'geometry candidates only',diagnostics:[],assumptions:[],metrics:{}};
 });
 if(result.status!=='complete')throw Error(JSON.stringify(result.error));
 const a=evidence.audit,cs=a.corridors;evidence.summary={id,seconds:(performance.now()-start)/1000,rssBytes:process.memoryUsage().rss,peakRssBytes:process.resourceUsage().maxRSS*1024,privateBytes:null,privateNote:'not measured by Node; requires OS supervisor',beforeCount:cs.reduce((s:number,c:any)=>s+c.admissibleIntervalsMm.length,0),afterCount:evidence.finiteCandidates.length,beforeLengthMm:cs.flatMap((c:any)=>c.admissibleIntervalsMm).reduce((s:number,v:number[])=>s+v[1]-v[0],0),afterLengthMm:evidence.finiteCandidates.reduce((s:number,c:any)=>s+c.lengthMm,0),sources:a.sourceCopper?.length,signalRegions:a.signalRegions,contacts:a.contacts,returnIslands:a.returnIslands};
 writeFileSync(`${folder}/${id}.json`,JSON.stringify(evidence));console.log(JSON.stringify(evidence.summary));
 const canvas=createCanvas(1400,1000),ctx=canvas.getContext('2d'),rs=a.sourceCopper??[];let xmin=Infinity,ymin=Infinity,xmax=-Infinity,ymax=-Infinity;
 for(const r of rs)for(const p of r.polygon.outer){xmin=Math.min(xmin,p[0]);xmax=Math.max(xmax,p[0]);ymin=Math.min(ymin,p[1]);ymax=Math.max(ymax,p[1]);}
 const scale=Math.min(1300/(xmax-xmin),800/(ymax-ymin)),map=(p:number[])=>[50+(p[0]-xmin)*scale,120+(p[1]-ymin)*scale];
 const ring=(ps:number[][])=>{ps.forEach((p,i)=>{const [x,y]=map(p);i?ctx.lineTo(x,y):ctx.moveTo(x,y);});ctx.closePath();};
 ctx.fillStyle='white';ctx.fillRect(0,0,1400,1000);ctx.fillStyle='black';ctx.font='24px Arial';ctx.fillText(`${id}: source copper and bounded candidate domains`,30,35);ctx.font='17px Arial';ctx.fillText(`Geometry only; EM refused. Before ${evidence.summary.beforeCount}, after ${evidence.summary.afterCount}. Slots/branches/bores retained.`,30,70);
 for(const r of rs){ctx.beginPath();for(const p of [r.polygon.outer,...r.polygon.holes])ring(p);ctx.fillStyle=r.layer==='TOP'?'rgba(220,60,40,.18)':'rgba(30,80,210,.12)';ctx.fill('evenodd');}
 for(const c of cs){ctx.beginPath();ring(c.polygonMm);ctx.strokeStyle='#888';ctx.lineWidth=1;ctx.stroke();for(const d of c.wideReturnDomains){ctx.beginPath();ring(d.polygonMm);ctx.fillStyle='rgba(20,180,70,.45)';ctx.fill();}}
 for(const c of evidence.finiteCandidates){const d=c.corridors.at(-1);const [lo,hi]=c.longitudinalMm,t=c.transverseUnit,n=c.direction,centre=c.origin[0]*t[0]+c.origin[1]*t[1],w=c.widthsMm.at(-1);ctx.beginPath();ring([[lo,centre-w],[hi,centre-w],[hi,centre+w],[lo,centre+w]].map(([x,y])=>[n[0]*x+t[0]*y,n[1]*x+t[1]*y]));ctx.strokeStyle='#00a040';ctx.lineWidth=2;ctx.stroke();}
 for(const h of a.bores??[])for(const p of h.polygons){ctx.beginPath();ring(p.outer);ctx.strokeStyle='#000';ctx.stroke();}
 for(const c of a.contacts??[]){const r=rs.find((r:any)=>r.source.path===c.sourcePath&&r.layer===c.layer);if(r){ctx.beginPath();ring(r.polygon.outer);ctx.strokeStyle='#d000d0';ctx.lineWidth=3;ctx.stroke();}}
 writeFileSync(`${folder}/${id}.png`,canvas.toBuffer('image/png'));
}
