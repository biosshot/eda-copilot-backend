import {readFileSync,writeFileSync,readdirSync,mkdirSync,existsSync} from 'node:fs';
import {gunzipSync,gzipSync} from 'node:zlib';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {loadNativeBoardPacker} from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import {withBlockCandidateCapture} from '../src/pcb-layout/pcb-auto-place-v2/block-quality.ts';
import {buildPlacementGraph} from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import {solvePlacementSubtreeSync} from '../src/pcb-layout/pcb-auto-place-v2/tree-solver.ts';

// Correctness replay only, not a performance benchmark. Exact encoded initial
// problems from this same native build may reuse their saved output.
const root='docs/experiments/placement-performance-2026-09-28/Telemetry';
const nativeHash=createHash('sha256').update(readFileSync('native/pcb-board-packer/pcb-board-packer.win32-x64-msvc.node')).digest('hex');
const bank=new Map();
const canonical=x=>JSON.stringify(x,(_k,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);
for(const dir of ['staged-v2','staged-block-check'])if(existsSync(`${root}/${dir}/provenance.json`)
    &&JSON.parse(readFileSync(`${root}/${dir}/provenance.json`)).nativeHash===nativeHash)for(const f of readdirSync(`${root}/${dir}`).filter(f=>f.startsWith('block-')&&f.endsWith('.json.gz'))){
    const d=JSON.parse(gunzipSync(readFileSync(`${root}/${dir}/${f}`)));
    if(d.problem.deferPairs&&d.solution.pairSeed)bank.set(canonical(d.problem),d.solution);
}
const out=`${root}/staged-block-check`;mkdirSync(out,{recursive:true});
writeFileSync(`${out}/provenance.json`,JSON.stringify({nativeHash,kind:'correctness replay, not a timing benchmark'},null,2));
const addon=loadNativeBoardPacker(),single=addon.solveBlockPrimitives,batch=addon.solveBlockPrimitivesBatch;
let hits=0,misses=0,serial=Date.now();
const run=problems=>{
    const answers=problems.map(p=>p.deferPairs?bank.get(canonical(p)):undefined);
    const missing=problems.filter((p,i)=>!answers[i]);
    hits+=answers.filter(Boolean).length;misses+=missing.length;
    const computed=missing.length>1?batch(missing,6):missing.map(p=>single(p));
    let j=0;
    return problems.map((p,i)=>{
        const result=answers[i]?structuredClone(answers[i]):computed[j++];
        if(!answers[i]&&p.deferPairs)writeFileSync(`${out}/block-${++serial}.json.gz`,gzipSync(JSON.stringify({problem:p,solution:result})));
        return result;
    });
};
Object.defineProperty(addon,'solveBlockPrimitives',{configurable:true,value:p=>run([p])[0]});
Object.defineProperty(addon,'solveBlockPrimitivesBatch',{configurable:true,value:ps=>run(ps)});
const ceilings={voltage_iso:5664.15,current_iso:11877.64,adc:12714.13};
for(const name of process.argv.slice(2).length?process.argv.slice(2):Object.keys(ceilings)){
    const input=JSON.parse(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json'));
    const graph=buildPlacementGraph(input);
    const find=n=>n.kind==='block'&&n.label===name?n:n.children.map(find).find(Boolean);
    let selected;
    const result=withBlockCandidateCapture((label,pool,chosen)=>{
        if(label===name){selected=chosen;writeFileSync(`${out}/${name}.json.gz`,gzipSync(JSON.stringify({label,pool,selected:chosen})));}
    },()=>solvePlacementSubtreeSync({input,graph,node:find(graph.root)}));
    assert.ok(selected?.length,`${name}: no legal candidates`);
    console.log(JSON.stringify({name,quality:selected[0].quality.score,ceiling:ceilings[name],hits,misses,
        diagnostics:result.diagnostics.filter(d=>d.message.startsWith('Staged block search:')).map(d=>d.message)}));
    assert.ok(selected[0].quality.score<=ceilings[name],`${name}: quality regressed`);
}
