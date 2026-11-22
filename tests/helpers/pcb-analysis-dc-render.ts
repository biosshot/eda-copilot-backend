export { renderDcReport } from '../../src/analyze/render/dc-html.ts';
import {readFile,readdir,writeFile} from 'node:fs/promises';
import {join,relative} from 'node:path';
const escape=(s:unknown)=>String(s).replace(/[&<>"']/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]!));
const style=`body{font:15px system-ui;background:#111b26;color:#e4edf5;margin:24px}a{color:#8dd5ff}h1{margin-bottom:8px}small,.muted{color:#aac0d3}select,input,button{background:#233449;color:#eff6ff;border:1px solid #587086;padding:6px;margin:4px}table{border-collapse:collapse;font-size:13px;margin:12px 0;width:100%}td,th{border:1px solid #405267;text-align:left;padding:7px;vertical-align:top}pre{white-space:pre-wrap;overflow-wrap:anywhere;background:#1d2b3a;padding:12px}.tools{position:sticky;top:0;background:#142332;z-index:2;padding:8px}.layout{display:grid;grid-template-columns:minmax(600px,2fr) minmax(280px,1fr);gap:16px}canvas{width:100%;background:#0a1018;border:1px solid #506177}.legend{height:16px;background:linear-gradient(to right,hsl(240 85% 55%),hsl(180 85% 55%),hsl(120 85% 55%),hsl(60 85% 55%),hsl(0 85% 55%))}.status{padding:8px;border-left:4px solid #ffc26a;background:#303747}.cards{display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:18px}.card{background:#213144;border:1px solid #4b6178;padding:16px}.card img{width:100%;max-height:190px;object-fit:contain}`;

export async function renderDcIndex(directory:string){
  const entries: {fixture:string;output:string;status:string;sourceSha256:string}[]=[];
  for(const file of await readdir(directory))if(file.endsWith('.json'))entries.push(JSON.parse(await readFile(join(directory,file),'utf8')));
  entries.sort((a,b)=>a.fixture.localeCompare(b.fixture));
  const cards=[];
  let solvedBoards=0,attempts=0;
  for(const e of entries){
    const file=relative(directory,join(e.output,'index.html')).replaceAll('\\','/');
    const saved=JSON.parse(await readFile(join(e.output,'dc.json'),'utf8'));
    const calculations=saved.result.reports.flatMap((r:{calculations:unknown[]})=>r.calculations).filter((c:{phase:string})=>c.phase==='completed');
    attempts+=calculations.length;if(calculations.length===4&&calculations.every((c:{result:{solution:{status:string}}})=>c.result.solution.status==='complete')&&saved.audit?.status!=='fail'&&(e as {audit?:{status:string}}).audit?.status==='pass')solvedBoards++;
    const statuses=calculations.map((c:{result:{solution:{status:string;diagnostics:{code:string}[]}}})=>c.result.solution.status+(c.result.solution.diagnostics.length?': '+c.result.solution.diagnostics.map(d=>d.code).join(', '):''));
    const first=calculations[0]?.result.solution;
    const geometry=first?.geometry;
    if(geometry?.outline){
      const path=(p:{outer:number[][];holes?:number[][][]})=>[p.outer,...(p.holes??[])].map(r=>r.map((p,i)=>(i?'L':'M')+p.join(' ')).join(' ')+'Z').join(' ');
      const points=geometry.outline.outer as number[][],xs=points.map(p=>p[0]),ys=points.map(p=>p[1]),x=Math.min(...xs)-1,y=Math.min(...ys)-1,w=Math.max(...xs)-x+1,h=Math.max(...ys)-y+1;
      const layer=geometry.layers[0];
      const copper=layer.copper.map((p:{outer:number[][]})=>`<path d="${path(p)}" fill="#6d7d8f" fill-rule="evenodd"/>`).join('');
      const svg=`<svg xmlns="http://www.w3.org/2000/svg" viewBox="${x} ${y} ${w} ${h}" width="600" height="400"><title>${escape(e.fixture)}: real ${escape(layer.layer)} copper geometry preview</title><rect x="${x}" y="${y}" width="${w}" height="${h}" fill="#101820"/><path d="${path(geometry.outline)}" stroke="#e5edf5" stroke-width=".12" fill="none"/>${copper}</svg>`;
      await writeFile(join(e.output,'thumbnail.svg'),svg);
    }
    const thumb=relative(directory,join(e.output,'thumbnail.svg')).replaceAll('\\','/');
    cards.push(`<article class="card"><h2><a href="${escape(file)}">${escape(e.fixture)}</a></h2>${geometry?.outline?`<a href="${escape(file)}"><img src="${escape(thumb)}" alt="Top-layer copper geometry preview; open report for computed fields"></a>`:''}<p>${escape(statuses.join(' / ')||saved.result.error?.code||e.status)}</p><small>SHA256 ${escape(e.sourceSha256)}</small><p><a href="${escape(file)}">Open layer / field / mode report</a></p></article>`);
  }
  await writeFile(join(directory,'index.html'),`<!doctype html><html><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Eight captured PCBs — DC review</title><style>${style}.card{overflow-wrap:anywhere;min-width:0}</style><h1>DC corpus review</h1><p class="status">${solvedBoards}/${entries.length} real boards solved; ${attempts} physical attempts with saved numerical audit. ${solvedBoards>=3?'Numerical coverage reached; see the verification report for mesh convergence and unresolved inputs.':'Three solved real boards remain an unmet acceptance criterion.'}</p><p>Real production DSL → CPU/F64 Rust → fields/checks. Test hypotheses, unknown CAD freshness, finite mesh. Thumbnails show copper geometry only. A rejected case has no calculated heatmap. Open each report for sources, return paths, input hash and diagnostics.</p><p><a href="../dc-coupon/index.html">Separate synthetic numerical reference</a> — not a member of the eight-board corpus.</p><div class="cards">${cards.join('\n')}</div></html>`);
}
