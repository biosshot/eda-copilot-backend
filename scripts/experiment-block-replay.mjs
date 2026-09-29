// Offline replay. Each native input includes its experiment flags (cache-safe).
import { readFileSync, writeFileSync, readdirSync, mkdirSync } from 'node:fs';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
const require = createRequire(import.meta.url);
const nativePath = resolve(process.env.PCB_BOARD_PACKER_NATIVE_PATH ?? `native/pcb-board-packer/${require('../native/pcb-board-packer/platform.cjs').nativeFilename()}`);
const addon = require(nativePath);
export const variants = {
 NCL: {netCandidates:true,reducedHull:true,longNets:true},
 NCLP: {netCandidates:true,reducedHull:true,longNets:true,extraPasses:true},
 NCLX: {netCandidates:true,reducedHull:true,longNets:true,pairSwaps:true},
 NCLR: {netCandidates:true,reducedHull:true,longNets:true,reinsertPair:true},
 NCLW: {netCandidates:true,reducedHull:true,longNets:true,width:4,keepDenseAccess:true},
 W4A: {width:4,keepDenseAccess:true},
 B0: {}, N: { netCandidates: true }, W4: { width: 4 }, W8: { width: 8 }, W16: { width: 16 },
 S: { stableNetWeight: true }, C1: { reducedHull: true }, C2: { smoothAspect: true }, L: { longNets: true },
 P: { extraPasses: true }, X: { pairSwaps: true }, R: { reinsertPair: true },
 NW: { netCandidates: true, width: 4 },
 NWS: { netCandidates: true, width: 4, stableNetWeight: true },
 NWSC: { netCandidates: true, width: 4, stableNetWeight: true, reducedHull: true, smoothAspect: true },
 ALL: { netCandidates: true, width: 4, stableNetWeight: true, reducedHull: true, smoothAspect: true, longNets: true },
 ALLP: { netCandidates: true, width: 4, stableNetWeight: true, reducedHull: true, smoothAspect: true, longNets: true, extraPasses: true },
 ALLX: { netCandidates: true, width: 4, stableNetWeight: true, reducedHull: true, smoothAspect: true, longNets: true, pairSwaps: true },
 ALLR: { netCandidates: true, width: 4, stableNetWeight: true, reducedHull: true, smoothAspect: true, longNets: true, reinsertPair: true },
};
for (const [label, key] of Object.entries({ noN:'netCandidates', noW:'width', noS:'stableNetWeight', noC1:'reducedHull', noC2:'smoothAspect', noL:'longNets' })) {
 const variant={...variants.ALL}; delete variant[key]; variants[`ALL_${label}`]=variant;
}
export function metrics(problem, solution) {
 const nets = new Map(); const boxes = []; let lockedChanged = 0;
 for (const p of problem.primitives) {
  const s = solution.states.find(s => s.primitiveId === p.id);
  if (!s) throw Error(`Missing primitive ${p.id}`);
  if (p.locked && (s.rotation || s.translationX || s.translationY)) lockedChanged++;
  const cx = (p.bbox.left + p.bbox.right)/2, cy = (p.bbox.top + p.bbox.bottom)/2;
  const r = s.rotation * Math.PI/180;
  const transform = q => ({ x: cx + (q.x-cx)*Math.cos(r)-(q.y-cy)*Math.sin(r)+s.translationX,
   y: cy+(q.x-cx)*Math.sin(r)+(q.y-cy)*Math.cos(r)+s.translationY });
  const corners = [[p.bbox.left,p.bbox.top],[p.bbox.right,p.bbox.top],[p.bbox.left,p.bbox.bottom],[p.bbox.right,p.bbox.bottom]].map(([x,y])=>transform({x,y}));
  boxes.push(...corners);
  for (const cp of p.connectionPoints) {
   if (!cp.net || /^(GND|AGND|DGND|PGND)$/i.test(cp.net)) continue;
   const pts = nets.get(cp.net) ?? []; pts.push({...transform(cp), ref: cp.ref, id: p.id}); nets.set(cp.net,pts);
  }
 }
 const width = Math.max(...boxes.map(p=>p.x))-Math.min(...boxes.map(p=>p.x));
 const height = Math.max(...boxes.map(p=>p.y))-Math.min(...boxes.map(p=>p.y));
 let hpwl = 0; const pairLengths = {};
 for (const [net, pts] of nets) {
  if (new Set(pts.map(p=>p.id)).size < 2) continue;
  hpwl += Math.max(...pts.map(p=>p.x))-Math.min(...pts.map(p=>p.x)) + Math.max(...pts.map(p=>p.y))-Math.min(...pts.map(p=>p.y));
  if (pts.length === 2) pairLengths[net] = Math.hypot(pts[0].x-pts[1].x,pts[0].y-pts[1].y);
 }
 const lengths = Object.values(pairLengths).sort((a,b)=>a-b);
 return { hard: solution.rank.hardCount, lockedChanged, width,height,area:width*height,aspect:Math.max(width/height,height/width), hpwl,
  pairSum:lengths.reduce((a,b)=>a+b,0), pairMax:lengths.at(-1)??0, p95:lengths[Math.max(0,Math.ceil(lengths.length*.95)-1)]??0, pairLengths };
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
 const [variantList = 'B0,N,W4,S,C1,C2,L,P,X', filter = '', root = 'tests/fixtures/block-placement', out = 'debugging/block-replay.json'] = process.argv.slice(2);
 const rows = [];
 for (const fixture of readdirSync(root)) {
  const input = JSON.parse(readFileSync(`${root}/${fixture}/input.json`));
  for (const file of readdirSync(`${root}/${fixture}`).filter(f=>/^block-.*\.json$/.test(f))) {
   const captured = JSON.parse(readFileSync(`${root}/${fixture}/${file}`));
   const name = [...new Set(captured.problem.components.map(c=>c.blockName))].join('+');
   if (name.includes('+') || captured.problem.primitives.length > 12) continue;
   if (!`${fixture}/${name}`.includes(filter) || captured.problem.primitives.length < 2) continue;
   for (const variant of variantList.split(',')) {
    if (!(variant in variants)) throw Error(`Unknown variant ${variant}`);
    const problem = structuredClone(captured.problem); const {width,...experiments} = variants[variant];
    problem.experiments = {...experiments, ignoredNets: input.solverOptions.ignoredRatsnestSignals}; if (width) problem.searchWidth = width;
    const start = performance.now(); const solution = addon.solveBlockPrimitives(problem); const ms = performance.now()-start;
    const row = { fixture,name,file,variant,ms,...metrics(problem,solution),solution };
    rows.push(row); console.log(JSON.stringify({...row,solution:undefined,pairLengths:undefined}));
    mkdirSync(resolve(out,'..'),{recursive:true});
    writeFileSync(out,JSON.stringify({nativeHash:createHash('sha256').update(readFileSync(nativePath)).digest('hex'),rows},null,2));
   }
  }
 }
}
