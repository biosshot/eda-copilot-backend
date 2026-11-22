import {readFileSync,writeFileSync} from 'node:fs';
import {createCanvas} from 'canvas';
const folder='debugging/p7-channel-plan/';
const geometry=JSON.parse(readFileSync(folder+'prepared-geometry-original.json','utf8'));
const evidence=JSON.parse(readFileSync(folder+'stm32-diagnostic-sensitivity-v3.json','utf8'));
const plan=evidence.samples[0].plan;
const canvas=createCanvas(1400,850),ctx=canvas.getContext('2d');
ctx.fillStyle='#f8fafc';ctx.fillRect(0,0,1400,850);ctx.fillStyle='#0f172a';ctx.font='bold 27px Arial';ctx.fillText('STM32: automatic physical channel domains',30,43);ctx.font='16px Arial';ctx.fillText('Full source-contact coverage passes. Channel admission still requires spatial/domain and resource gates; no 5% claim.',30,73);
const contour=(polygon:any,map:(p:number[])=>number[])=>{ctx.beginPath();for(const ring of [polygon.outer,...polygon.holes]){ring.forEach((p:number[],i:number)=>{const v=map(p);if(i===0)ctx.moveTo(v[0],v[1]);else ctx.lineTo(v[0],v[1]);});ctx.closePath();}};
function panel(x:number,y:number,w:number,h:number,bounds:number[][],title:string){
 ctx.fillStyle='white';ctx.fillRect(x,y,w,h);ctx.strokeStyle='#cbd5e1';ctx.strokeRect(x,y,w,h);ctx.fillStyle='#0f172a';ctx.font='bold 19px Arial';ctx.fillText(title,x+14,y+27);
 const scale=Math.min((w-40)/(bounds[1][0]-bounds[0][0]),(h-70)/(bounds[1][1]-bounds[0][1]));const ox=x+(w-scale*(bounds[1][0]-bounds[0][0]))/2,oy=y+48+(h-65-scale*(bounds[1][1]-bounds[0][1]))/2;const map=(p:number[])=>[ox+(p[0]-bounds[0][0])*scale,oy+(p[1]-bounds[0][1])*scale];
 ctx.save();ctx.beginPath();ctx.rect(x+4,y+40,w-8,h-44);ctx.clip();
 for(const r of geometry.regions){contour(r.polygon,map);ctx.fillStyle=r.layer==='TOP'?'rgba(220,65,30,.25)':'rgba(30,90,220,.20)';ctx.fill('evenodd');}
 if(geometry.outline){contour(geometry.outline,map);ctx.strokeStyle='#64748b';ctx.lineWidth=1;ctx.stroke();}
 const colors=['#0284c7','#16a34a','#9333ea'];plan.candidateDomainsMm.forEach((r:number[][],i:number)=>{const a=map(r[0]),b=map(r[1]);ctx.strokeStyle=colors[i];ctx.lineWidth=2;ctx.strokeRect(a[0],a[1],b[0]-a[0],b[1]-a[1]);});
 for(const contact of plan.contacts){for(const r of geometry.regions.filter((r:any)=>r.source.path===contact.sourcePath&&r.layer===contact.layer)){contour(r.polygon,map);ctx.strokeStyle=contact===plan.contacts.at(-1)?'#0891b2':'#111827';ctx.lineWidth=2.5;ctx.stroke();}}
 ctx.restore();return map;
}
panel(25,100,650,650,[[-31,-21],[31,21]],'Captured copper and extraction location');
const r=plan.candidateDomainsMm.at(-1),margin=0.1;
panel(700,100,675,500,[[r[0][0]-margin,r[0][1]-margin],[r[1][0]+margin,r[1][1]+margin]],'Signal via, finite return and surrounding copper');
ctx.font='17px Arial';let y=635;for(const [label,color]of [['TOP captured copper','#dc411e'],['BOTTOM captured copper','#1e5adc'],['Automatic environment 1 / 2 / 3','#9333ea']]){ctx.fillStyle=color;ctx.fillRect(720,y-14,20,14);ctx.fillStyle='#0f172a';ctx.fillText(label,755,y);y+=31;}
ctx.font='16px Arial';ctx.fillStyle='#334155';ctx.fillText('All native copper holes and gaps retained.',720,749);ctx.fillText('20 topology warnings at 1 um curve tolerance; 6 at 0.1 um.',30,788);ctx.fillText('None is on the selected port source. Decisions and source objects are retained in the JSON evidence.',30,815);
writeFileSync(folder+'stm32-automatic-domains.png',canvas.toBuffer('image/png'));
console.log(folder+'stm32-automatic-domains.png');
