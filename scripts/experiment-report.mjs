import {readFileSync,writeFileSync,mkdirSync,readdirSync,existsSync,copyFileSync} from 'node:fs';
import {createHash} from 'node:crypto';
const dest='docs/experiments/block-placement-2026-09-26'; mkdirSync(dest,{recursive:true});
const round=n=>Math.round(n*100)/100;
const blocks=['ordinary-experiments','v2-experiments','ablation-experiments'].map(name=>{
 const data=JSON.parse(readFileSync(`.test-output/${name}.json`));
 return {name,nativeHash:data.nativeHash,rows:data.rows.map(({solution,...row})=>row)};
});
const boards=[];
for(const fixture of ['Telemetry','ESPower','esp32c3']) {
 const root=`.test-output/board-experiments/${fixture}`;
 for(const variant of readdirSync(root)) {
  const path=`${root}/${variant}/summary.json`; if(!existsSync(path))continue;
  const s=JSON.parse(readFileSync(path)); const placements=JSON.parse(readFileSync(`${root}/${variant}/placement.json`)).placements;
  const input=JSON.parse(readFileSync(`tests/fixtures/block-placement/${fixture}/input.json`));
  const baseline=JSON.parse(readFileSync(`${root}/B0/placement.json`)).placements;
  const fixedChanged=input.components.filter(c=>c.pcb.fixedPlacement).filter(c=>{
   const a=baseline.find(p=>p.designator===c.designator),b=placements.find(p=>p.designator===c.designator);
   return !a||!b||['x','y','rotate','layer'].some(k=>a[k]!==b[k]);
  }).map(c=>c.designator);
  const routePath=`${root}/${variant}/route-probe.json`; let route;
  if(existsSync(routePath)) { const {jobs,...r}=JSON.parse(readFileSync(routePath)); route={...r,jobs:jobs.length,found:jobs.filter(j=>j.status==='found').length, vias:jobs.reduce((s,j)=>s+j.vias,0)}; }
  const checks=Object.fromEntries(['outsideBoard','overlaps','boardHoleViolations','constraintRegionViolations','layerViolations','hintViolations'].map(k=>[k,s.report[k]?.length??null]));
  boards.push({...s,report:undefined,checks,route,fixedChanged,placementHash:createHash('sha256').update(JSON.stringify(placements)).digest('hex')});
 }
}
writeFileSync(`${dest}/measurements.json`,JSON.stringify({blocks,boards},null,2));
const table=boards.map(s=>{ const m=s.stageMetrics.at(-1); return `| ${s.fixture} | ${s.variant} | ${s.ok} | ${round(m.hpwl)} | ${round(m.pairSum)} | ${round(m.pairMax)} | ${s.fixedChanged.length} | ${round(s.ms/1000)} |`; }).join('\n');
writeFileSync(`${dest}/board-results.md`,`# Full-board exploratory results\n\nTiming is single-run wall time under variable concurrent load; it is not a performance acceptance benchmark. Lengths are mm. HPWL and pairSum are different metrics and must not be added. Placement ok is not routing completeness.\n\n| Fixture | Variant | Placement ok | HPWL | Two-terminal sum | Worst pair | Fixed changes | Seconds |\n|---|---|---|---:|---:|---:|---:|---:|\n${table}\n`);
for(const variant of ['B0','ALLX']) copyFileSync(`.test-output/usb-comparison/${variant}.png`,`${dest}/usb-${variant}.png`);
console.log({blockRuns:blocks.reduce((s,b)=>s+b.rows.length,0),boardRuns:boards.length,fixedChanges:boards.reduce((s,b)=>s+b.fixedChanged.length,0)});
