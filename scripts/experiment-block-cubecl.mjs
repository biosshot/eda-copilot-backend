// Full native block cycle: initial search/singles and deferred pair continuation.
// Saved problems and fixtures are never edited. Validation stays outside timing.
import { readFileSync, writeFileSync, mkdirSync, readdirSync, openSync, closeSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { spawnSync } from 'node:child_process';

const args = Object.fromEntries(process.argv.slice(2).map(s => { const i=s.indexOf('='); return [s.slice(0,i),s.slice(i+1)]; }));
const mode=args.backend ?? 'cpu';
if (!['cpu','cubecl','auto'].includes(mode)) throw Error('backend must be cpu, cubecl or auto');
// Parent saves native stderr as evidence and annotates timings with actual backend.
if (!process.env.PCB_BLOCK_EXPERIMENT_CHILD) {
  const out=resolve(args.out ?? `debugging/cubecl-block-migration-2026-09-30/${mode}-${Date.now()}`);
  mkdirSync(out,{recursive:true});
  const stdout=openSync(join(out,'stdout.log'),'w'),stderr=openSync(join(out,'stderr.log'),'w');
  const child=spawnSync(process.execPath,[process.argv[1],...process.argv.slice(2).filter(a=>!a.startsWith('out=')),`out=${out}`],{
    env:{...process.env,PCB_BLOCK_EXPERIMENT_CHILD:'1'},stdio:['ignore',stdout,stderr],windowsHide:true});
  closeSync(stdout);closeSync(stderr);
  const logs=readFileSync(join(out,'stderr.log'),'utf8');
  const parse=tag=>logs.split(/\r?\n/).filter(l=>l.startsWith(tag+' ')).map(l=>JSON.parse(l.slice(tag.length+1)));
  if(child.status===0) {
    const path=join(out,'results.json'),report=JSON.parse(readFileSync(path));
    report.nativeCalls=parse('[block-backend]');report.fallbacks=parse('[block-gpu-fallback]');
    report.runtime=parse('[block-gpu-runtime]');report.gpuStages=parse('[block-gpu-stage]');
    const actual=[...new Set(report.nativeCalls.map(c=>c.backend))];
    report.actualBackend=actual.length===1?actual[0]:actual.length?'mixed':'unreported';
    const perRun=report.nativeCalls.length/report.rows.length;
    for(const row of report.rows) row.nativeCalls=report.nativeCalls.slice(row.run*perRun,(row.run+1)*perRun);
    writeFileSync(path,JSON.stringify(report,null,2));
  }
  process.stdout.write(readFileSync(join(out,'stdout.log'),'utf8'));
  if(child.status!==0) process.stderr.write(logs.slice(-12000));
  if(child.error) console.error(child.error);
  process.exit(child.status ?? 1);
}
process.env.PCB_BLOCK_BACKEND=mode;
process.env.PCB_BLOCK_SOLVER_PROFILE='1';
const require=createRequire(import.meta.url);
const addonPath=resolve(process.env.PCB_BOARD_PACKER_NATIVE_PATH ?? `native/pcb-board-packer/${require('../native/pcb-board-packer/platform.cjs').nativeFilename()}`);
const addon=require(addonPath);
const root=resolve(args.root ?? 'debugging/pcb-layout/runs/PortableScope/2026-09-29T12-02-18-665Z/native/block/process-19228-thread-0');
const prefixes=(args.blocks ?? '00079,00097,00091').split(',');
const runs=Number(args.runs ?? 4),workers=Number(args.workers ?? 1);
if (!Number.isInteger(runs)||runs<1||!Number.isInteger(workers)||workers<1||workers>8) throw Error('invalid runs/workers');
const out=resolve(args.out ?? `debugging/cubecl-block-migration-2026-09-30/${mode}-${Date.now()}`);
mkdirSync(out,{recursive:true});
const hash=b=>createHash('sha256').update(b).digest('hex');
const sourceFiles=['native/pcb-board-packer/Cargo.toml','native/pcb-board-packer/Cargo.lock','native/pcb-board-packer/src/lib.rs',
  'native/pcb-board-packer/src/block_solver.rs','native/pcb-board-packer/src/block_solver/cubecl.rs',
  'native/pcb-board-packer/src/block_solver/compact_candidates.rs','native/pcb-board-packer/src/block_solver/gpu_kernels.rs',
  'native/pcb-board-packer/src/compute/mod.rs','native/pcb-board-packer/src/compute/gpu.rs',
  'native/pcb-board-packer/src/compute/workspace.rs','native/pcb-board-packer/src/compute/numerics.rs','native/pcb-board-packer/src/block_solver/gpu_frontier.rs','scripts/experiment-block-cubecl.mjs'];
// A frozen pre-migration addon can retain the source evidence from its baseline.
const sourceEvidence=args.sourceReport ? JSON.parse(readFileSync(resolve(args.sourceReport))) : null;
if(sourceEvidence&&sourceEvidence.addonSha256!==hash(readFileSync(addonPath)))throw Error('sourceReport belongs to a different addon');
const sourceHashes=sourceEvidence ? sourceEvidence.sourceHashes
  : Object.fromEntries(sourceFiles.map(p=>[p,hash(readFileSync(p))]));
const reference=args.reference ? JSON.parse(readFileSync(resolve(args.reference))) : null;
const pairsOnly=args.pairsOnly==='1';
if(pairsOnly&&!reference)throw Error('pairsOnly requires a full-cycle reference with original pairSeed');
const blocks=prefixes.map(prefix=>{
  const matches=readdirSync(root).filter(n=>n.startsWith(`${prefix}-`));
  if(matches.length!==1) throw Error(`capture prefix ${prefix}: ${matches.length} matches`);
  const path=join(root,matches[0],'problem.json'),bytes=readFileSync(path),problem=JSON.parse(bytes);
  if(problem.pairSeed) throw Error('start with a full block input, not pair-only continuation');
  let replay=problem;
  if(pairsOnly) {
    const seed=reference.rows[0].results.find(r=>r.name===matches[0])?.initial?.pairSeed;
    if(!seed)throw Error(`no original pairSeed for ${matches[0]}`);
    replay={...problem,deferPairs:false,pairSeed:seed};
  }
  return {name:matches[0],path,sha256:hash(bytes),replaySha256:pairsOnly?hash(JSON.stringify(replay)):null,problem:replay};
});
const rows=[];
const expected=reference ? (pairsOnly ? prefixes.map(p=>{
  const r=reference.rows[0].results.find(r=>r.name.startsWith(p+'-'));
  return {name:r.name,initial:r.pairs,pairs:null};
}) : reference.rows[0].results) : null;
for(let run=0;run<runs;run++) {
  const problems=blocks.map(b=>b.problem);
  const call=ps=>workers===1 ? ps.map(p=>addon.solveBlockPrimitives(p)) : addon.solveBlockPrimitivesBatch(ps,workers);
  const started=performance.now();
  const initial=call(problems);
  const initialMs=performance.now()-started;
  const pairIndices=initial.flatMap((s,i)=>problems[i].deferPairs && s.pairSeed && problems[i].primitives.length<=12 ? [i] : []);
  const pairProblems=pairIndices.map(i=>({...problems[i],deferPairs:false,pairSeed:initial[i].pairSeed}));
  const pairStarted=performance.now();
  const pairs=pairProblems.length ? call(pairProblems) : [];
  const pairsMs=performance.now()-pairStarted,totalMs=performance.now()-started;
  // No comparison, file IO or reference scoring inside the timer.
  const results=blocks.map((b,i)=>({name:b.name,initial:initial[i],pairs:pairs[pairIndices.indexOf(i)]??null}));
  const exactReferenceMatch=reference ? isDeepStrictEqual(results,expected) : null;
  const row={run,first:run===0,initialMs,pairsMs,totalMs,exactReferenceMatch,results};
  rows.push(row);
  writeFileSync(join(out,'results.json'),JSON.stringify({backendRequested:mode,precision:'f64',workers,
    validation:!!process.env.PCB_BLOCK_GPU_VERIFY,verifyPruning:!!process.env.PCB_BLOCK_GPU_VERIFY_PRUNE,verifyFrontier:!!process.env.PCB_BLOCK_GPU_VERIFY_FRONTIER,noPrune:!!process.env.PCB_BLOCK_GPU_NO_PRUNE,
    scope:pairsOnly?'deferred-pairs call replayed with original pairSeed':'full native block cycle; includes deferred pairs for <=12 primitives; not whole board',
    addonPath,addonSha256:hash(readFileSync(addonPath)),sourceReport:args.sourceReport??null,sourceHashes,blocks:blocks.map(({problem,...b})=>b),rows},null,2));
  console.log(JSON.stringify({run,initialMs,pairsMs,totalMs,exactReferenceMatch,
    blocks:results.map(r=>({name:r.name,rank:(r.pairs??r.initial).rank}))}));
}
