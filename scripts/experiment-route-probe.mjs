import { readFileSync, writeFileSync, readdirSync } from 'node:fs';
import { createPostPlaceRouteScoreContext, preparePostPlaceRouteComparison, comparePostPlaceRouteCandidate } from '../src/pcb-layout/pcb-auto-place-v2/post-place-route-score.ts';
const [fixture='Telemetry', runSet='board-experiments', baselineVariant='B0'] = process.argv.slice(2);
if (!['board-experiments', 'architecture'].includes(runSet)) throw Error(`Unknown run set: ${runSet}`);
const root=`debugging/${runSet}/${fixture}`;
const input=JSON.parse(readFileSync(`tests/fixtures/block-placement/${fixture}/input.json`));
const readPoses=v=>JSON.parse(readFileSync(`${root}/${v}/placement.json`)).placements;
const context=createPostPlaceRouteScoreContext(input);
const baseline=preparePostPlaceRouteComparison(input,readPoses(baselineVariant),new Set(input.components.map(c=>c.designator)),context);
const rows=[];
for(const variant of readdirSync(root,{withFileTypes:true}).filter(e=>e.isDirectory()).map(e=>e.name)) {
 const result=comparePostPlaceRouteCandidate(input,readPoses(variant),baseline,context);
 const {jobs,...summary}=result;
 rows.push({variant,...summary,jobs:jobs.length,found:jobs.filter(j=>j.status==='found').length,
   noPath:jobs.filter(j=>j.status==='no_path').length, length:jobs.reduce((s,j)=>s+(j.planarLength??0),0),vias:jobs.reduce((s,j)=>s+j.vias,0)});
 writeFileSync(`${root}/${variant}/route-probe.json`,JSON.stringify(result,null,2));
}
writeFileSync(`${root}/route-summary.json`,JSON.stringify(rows,null,2));
console.log(JSON.stringify({fixture,rows}));
