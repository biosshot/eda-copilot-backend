import {readFileSync,writeFileSync,readdirSync,existsSync} from 'node:fs';
import {join,resolve} from 'node:path';
import {isDeepStrictEqual} from 'node:util';
const root=resolve(process.argv[2]??'debugging/cubecl-block-migration-2026-09-30');
const median=a=>{a=[...a].sort((a,b)=>a-b);return a[Math.floor(a.length/2)];};
const summary=[];
for(const name of readdirSync(root).filter(n=>n.startsWith('final-'))) {
 const file=join(root,name,'results.json');if(!existsSync(file))continue;
 const r=JSON.parse(readFileSync(file));if(!r.nativeCalls)continue;
 const keys=r.blocks.map(b=>({name:b.name,ids:JSON.parse(readFileSync(b.path)).primitives.map(p=>p.id).sort().join('|')}));
 const runs=r.rows.map(row=>({run:row.run,wallMs:row.totalMs,pairsMs:row.pairsMs,exactReferenceMatch:row.exactReferenceMatch,
  sameAsFirst:isDeepStrictEqual(row.results,r.rows[0].results),blocks:keys.map(b=>{
   const calls=row.nativeCalls.filter(c=>[...c.block].sort().join('|')===b.ids);
   return {name:b.name,backends:calls.map(c=>c.backend),beamMs:calls.reduce((s,c)=>s+c.beamMs,0),singlesMs:calls.reduce((s,c)=>s+c.singlesMs,0),
    pairsMs:calls.reduce((s,c)=>s+c.totalMs-c.beamMs-c.singlesMs,0),fullNativeMs:calls.reduce((s,c)=>s+c.totalMs,0),batches:calls.reduce((s,c)=>s+(c.gpuBatches??0),0),candidates:calls.reduce((s,c)=>s+(c.gpuCandidates??0),0)};
  })}));
 const warm=runs.slice(1);const medianBlocks=warm.length?keys.map(b=>Object.fromEntries(['beamMs','singlesMs','pairsMs','fullNativeMs'].map(k=>[k,median(warm.map(r=>r.blocks.find(x=>x.name===b.name)[k]))]).concat([['name',b.name]]))):[];
 summary.push({name,backend:r.actualBackend,workers:r.workers,addonSha256:r.addonSha256,firstWallMs:runs[0].wallMs,warmMedianMs:warm.length?median(warm.map(r=>r.wallMs)):null,
 initializations:r.runtime.length,maxMutexWaitMs:Math.max(0,...r.gpuStages.map(s=>s.runtime.mutexWaitMs)),maxWorkspaceBytes:Math.max(0,...r.gpuStages.map(s=>s.runtime.workspaceBytes)),fallbacks:r.fallbacks,runs,medianBlocks});
}
writeFileSync(join(root,'final-summary.json'),JSON.stringify(summary,null,2));
console.log(JSON.stringify(summary.map(({runs,fallbacks,...r})=>r),null,2));
