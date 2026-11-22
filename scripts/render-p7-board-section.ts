import {readFileSync,writeFileSync} from 'node:fs';
import {createCanvas} from 'canvas';
const folder='debugging/p7-channel-plan/';
const data=JSON.parse(readFileSync(folder+'bandamp-captured-section.json','utf8'));
const r=data.result,g=data.extracted.geometry,mapping=data.extracted.sourceMapping;
const rf=mapping.find((m:any)=>m.sourcePaths.includes('copper.fixed.tracks[20]'));
if(!rf)throw new Error('Original captured RF track attribution is required');
const basis=r.conductorOrder.indexOf(rf.conductorId),potential=r.potentialsV[basis];
const canvas=createCanvas(1450,900),ctx=canvas.getContext('2d');
ctx.fillStyle='#f8fafc';ctx.fillRect(0,0,1450,900);ctx.fillStyle='#0f172a';ctx.font='bold 27px Arial';ctx.fillText('BandAmpV5: captured RF transverse section',30,42);ctx.font='16px Arial';ctx.fillText('General polygon P1 FEM, finite coplanar copper, explicit physical reference. Section result only; no full-channel 5% claim.',30,74);
const bounds=[[-.0035,-.0015],[.0035,.0031]],x=35,y=130,w=900,h=640;
const scale=Math.min(w/(bounds[1][0]-bounds[0][0]),h/(bounds[1][1]-bounds[0][1]));const map=(p:number[])=>[x+(p[0]-bounds[0][0])*scale,y+(p[1]-bounds[0][1])*scale];
ctx.fillStyle='white';ctx.fillRect(x,y,w,h);ctx.save();ctx.beginPath();ctx.rect(x,y,w,h);ctx.clip();
for(const t of r.triangles){const value=t.reduce((s:number,i:number)=>s+potential[i],0)/3;ctx.beginPath();t.forEach((i:number,k:number)=>{const p=map(r.nodesM[i]);if(k===0)ctx.moveTo(p[0],p[1]);else ctx.lineTo(p[0],p[1]);});ctx.closePath();ctx.fillStyle=`hsl(${220*(1-value)},80%,${94-42*value}%)`;ctx.fill();}
for(const c of g.conductors){ctx.beginPath();for(const ring of [c.contour.outerM,...c.contour.holesM]){ring.forEach((p:number[],i:number)=>{const v=map(p);if(i===0)ctx.moveTo(v[0],v[1]);else ctx.lineTo(v[0],v[1]);});ctx.closePath();}ctx.fillStyle=c.id===rf.conductorId?'#ef4444':'#334155';ctx.fill('evenodd');ctx.strokeStyle='#0f172a';ctx.lineWidth=1;ctx.stroke();}
ctx.restore();ctx.strokeStyle='#94a3b8';ctx.strokeRect(x,y,w,h);
ctx.fillStyle='#0f172a';ctx.font='18px Arial';ctx.fillText('Unit potential: selected RF conductor 1 V; all other section conductors 0 V.',35,810);ctx.fillText('Actual holes/gaps retained. External ground connections and local transitions remain separate models.',35,840);
const sx=980;ctx.font='bold 21px Arial';ctx.fillText('Full coupled matrices',sx,135);
function table(key:string,unit:string,factor:number,yy:number){ctx.font='bold 17px Arial';ctx.fillText(unit,sx,yy);ctx.font='16px Consolas';r.parameters[key].forEach((row:number[],i:number)=>ctx.fillText(row.map(v=>(v*factor).toFixed(3).padStart(9)).join(' '),sx,yy+30+i*27));}
table('capacitanceFPerM','C [pF/m]',1e12,182);table('inductanceHPerM','L [nH/m]',1e9,330);
ctx.font='bold 17px Arial';ctx.fillText('Modes: Z norm [Ohm], delay [ps/mm]',sx,490);ctx.font='16px Consolas';r.parameters.modes.forEach((m:any,i:number)=>ctx.fillText(`${m.name}: ${m.impedanceOhm.toFixed(2)}   ${(m.delaySPerM*1e9).toFixed(3)}`,sx,525+i*29));
ctx.font='16px Arial';ctx.fillText(`${r.nodesM.length} nodes; ${r.triangles.length} triangles`,sx,650);ctx.fillText(`Field solve: ${r.elapsedSeconds.toFixed(2)} s`,sx,680);ctx.fillText(`Energy residual: ${r.quality.normalizedEnergyReactionError.toExponential(2)}`,sx,710);ctx.fillText('Each C/C0 factor serves all three RHS',sx,740);ctx.fillText('Board preparation excluded from field timing',sx,770);
writeFileSync(folder+'bandamp-captured-section.png',canvas.toBuffer('image/png'));console.log(folder+'bandamp-captured-section.png');
