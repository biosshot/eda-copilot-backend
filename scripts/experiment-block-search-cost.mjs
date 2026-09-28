import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {gunzipSync,gzipSync} from 'node:zlib';
import assert from 'node:assert/strict';
import {loadNativeBoardPacker} from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import {applyNativeBoardPackSolution} from '../src/pcb-layout/pcb-auto-place-v2/native/apply-board-solution.ts';
import {blockQuality,legalBlockCandidate,selectBlockCandidates} from '../src/pcb-layout/pcb-auto-place-v2/block-quality.ts';
import {selectPairSeeds} from '../src/pcb-layout/pcb-auto-place-v2/block-search-stages.ts';
import {createClearanceResolver} from '../src/pcb-layout/pcb-auto-place/clearance-resolver.ts';
const root='docs/experiments/placement-performance-2026-09-28/Telemetry';
const src=`${root}/isolated-block-metric`,out=`${root}/search-cost/${process.env.BLOCK_COST_LABEL??'exact'}`;
mkdirSync(out,{recursive:true});
const unpack=p=>JSON.parse(gunzipSync(readFileSync(p)));
const input=JSON.parse(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json'));
const clearance=createClearanceResolver(input),native=loadNativeBoardPacker();
for(const name of process.argv.slice(2).length?process.argv.slice(2):['current_iso']){
    const before=unpack(`${src}/${name}-geometric-0.json.gz`),all=unpack(`${src}/${name}-inputs.json.gz`);
    const ids=name==='current_iso'?[0,1,6]:all.map((_,i)=>i);
    const ps=ids.map(i=>({...all[i],experiments:{...all[i].experiments,routingMetric:'geometric'}}));
    const call=p=>p.length===1?[native.solveBlockPrimitives(p[0])]:native.solveBlockPrimitivesBatch(p,6);
    let t=performance.now();const initial=call(ps),initialMs=performance.now()-t,pool=[],seeds=[];
    const add=(p,s,h)=>{const primitives=applyNativeBoardPackSolution(p.primitives.map(p=>({...p,children:[]})),s,4);
        const c={hypothesis:String(h),stage:s.stage,primitives,quality:blockQuality(input,primitives),index:h};
        if(!s.rank.hardCount&&legalBlockCandidate(input,primitives,clearance))pool.push(c);
        if(!s.rank.hardCount&&s.stage==='singles')seeds.push(c);};
    initial.forEach((s,i)=>s.checkpoints.filter(c=>c.stage!=='pairs').forEach(c=>add(ps[i],c,ids[i])));
    const choices=selectPairSeeds(seeds,pool),pps=choices.map(c=>{const i=ids.indexOf(c.index),p={...ps[i],pairSeed:initial[i].pairSeed};delete p.deferPairs;return p;});
    t=performance.now();const pairs=pps.length?call(pps):[],pairsMs=performance.now()-t;
    pairs.forEach((s,i)=>s.checkpoints.forEach(c=>add(pps[i],c,choices[i].index)));
    const best=selectBlockCandidates(pool)[0];
    const exact=process.env.BLOCK_COST_ALLOW_CHANGE!=='1';
    if(exact){initial.forEach((s,i)=>assert.deepEqual(s.checkpoints,before.initial[ids[i]].checkpoints));
        pairs.forEach((s,i)=>assert.deepEqual(s.checkpoints,before.pairs[before.result.pairHypotheses.indexOf(choices[i].index)].checkpoints));}
    const summary={name,ids,initialMs,pairsMs,totalMs:initialMs+pairsMs,exact,selected:{hypothesis:best.hypothesis,stage:best.stage,quality:best.quality},before:before.result.selected};
    writeFileSync(`${out}/${name}.json.gz`,gzipSync(JSON.stringify({initial,pairs,summary})));
    writeFileSync(`${out}/${name}.json`,JSON.stringify(summary,null,2));
    console.log(JSON.stringify({name,initialMs,pairsMs,totalMs:summary.totalMs,score:best.quality.score,beforeScore:before.result.selected.quality.score,exact}));
}
