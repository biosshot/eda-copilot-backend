/** Physical geometry plans only; this script performs no volume extraction. */
import {writeFileSync} from 'node:fs';
import {createCanvas} from 'canvas';
import {segmentedCouponVariant,segmentedCouponDslFor} from '../tests/helpers/pcb-analysis-segmented-coupon.ts';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {localRequest} from '../src/analyze/analysis-dsl/local-request.ts';
import {capturedBoardGeometry} from '../src/analyze/geometry-snapshot.ts';
import {boardChannelPlan} from '../src/analyze/board-channel-plan.ts';
import {validateChannelOwnership} from '../src/analyze/segmented-channel.ts';
const variants=['straight','rotated45','rotated90','bend','branch','via','coupled','meander'],canvas=createCanvas(2000,1200),ctx=canvas.getContext('2d');
ctx.fillStyle='#f8fafc';ctx.fillRect(0,0,2000,1200);ctx.fillStyle='#0f172a';ctx.font='bold 25px Arial';ctx.fillText('P7 physical source/corridor controls — geometry only, no numerical certificate',28,40);
const evidence:any[]=[];
for(const [index,variant]of variants.entries()){
 const board=segmentedCouponVariant(variant),code=segmentedCouponDslFor(variant).split('a.plot(')[0];
 const result=await executeScenario({board,code},{filename:'corridor-plot.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{
  let contacts:any;const base=localRequest(task as any,p=>contacts=p.contacts),plan=boardChannelPlan(board,base,contacts);validateChannelOwnership(plan);const g=capturedBoardGeometry(JSON.stringify(board)),points=plan.audit.cells.flatMap((c:any)=>c.polygonMm),xs=points.map((p:number[])=>p[0]),ys=points.map((p:number[])=>p[1]),lo=[Math.min(...xs),Math.min(...ys)],hi=[Math.max(...xs),Math.max(...ys)],x=15+500*(index%4),y=75+550*Math.floor(index/4),scale=Math.min(440/(hi[0]-lo[0]),435/(hi[1]-lo[1])),map=(p:number[])=>[x+20+(p[0]-lo[0])*scale,y+75+(p[1]-lo[1])*scale];
  ctx.fillStyle='white';ctx.fillRect(x,y,475,525);ctx.strokeStyle='#cbd5e1';ctx.strokeRect(x,y,475,525);ctx.fillStyle='#0f172a';ctx.font='bold 21px Arial';ctx.fillText(variant,x+15,y+28);ctx.font='15px Arial';ctx.fillText(`${plan.blocks.filter(b=>b.kind==='section').length} sections / ${plan.blocks.filter(b=>b.kind==='local').length} locals`,x+15,y+51);
  const ring=(ps:number[][])=>{ps.forEach((p,i)=>{const a=map(p);if(i)ctx.lineTo(a[0],a[1]);else ctx.moveTo(a[0],a[1]);});ctx.closePath();};
  ctx.save();ctx.beginPath();ctx.rect(x+5,y+60,465,455);ctx.clip();
  for(const r of g.regions){ctx.beginPath();for(const p of [r.polygon.outer,...r.polygon.holes])ring(p);ctx.fillStyle=r.layer==='TOP'?'rgba(220,65,30,.65)':'rgba(30,90,220,.40)';ctx.fill('evenodd');}
  for(const cell of plan.audit.cells){ctx.beginPath();ring(cell.polygonMm);ctx.strokeStyle=plan.blocks.find(b=>b.id===cell.id)?.kind==='section'?'#16a34a':'#a855f7';ctx.lineWidth=2;ctx.stroke();}
  for(const face of plan.audit.faces){const t=[-face.normal[1],face.normal[0]],a=map([face.origin[0]+t[0]*face.span[0],face.origin[1]+t[1]*face.span[0]]),b=map([face.origin[0]+t[0]*face.span[1],face.origin[1]+t[1]*face.span[1]]);ctx.beginPath();ctx.moveTo(a[0],a[1]);ctx.lineTo(b[0],b[1]);ctx.strokeStyle='#0f172a';ctx.setLineDash([4,4]);ctx.stroke();ctx.setLineDash([]);}
  ctx.restore();evidence.push({variant,plan});return{id:'geometry',status:'complete',method:'Geometry-only source ownership',diagnostics:[],assumptions:[],metrics:{}};
 });if(result.status!=='complete')throw new Error(variant+': '+JSON.stringify(result.error));
}
ctx.font='16px Arial';for(const [i,[color,label]]of [['#dc411e','TOP source copper'],['#1e5adc','BOTTOM finite copper'],['#16a34a','Uniform section'],['#a855f7','Joint local'],['#0f172a','Dashed physical cuts']].entries()){ctx.fillStyle=color;ctx.fillRect(25+i*390,1171,22,4);ctx.fillText(label,58+i*390,1179);}
writeFileSync('debugging/p7-channel-plan/corridor-controls.json',JSON.stringify(evidence));writeFileSync('debugging/p7-channel-plan/corridor-controls.png',canvas.toBuffer('image/png'));
