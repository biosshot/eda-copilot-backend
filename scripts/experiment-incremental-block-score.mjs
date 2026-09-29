import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {gzipSync,gunzipSync} from 'node:zlib';
import {createHash} from 'node:crypto';
import assert from 'node:assert/strict';
import {loadNativeBoardPacker} from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
const root='docs/experimental/pcb/placement-performance-2026-09-28/Telemetry';
const source=`${root}/isolated-block-metric`,out=`${root}/incremental-score`;mkdirSync(out,{recursive:true});
const unpack=p=>JSON.parse(gunzipSync(readFileSync(p)));
const old=JSON.parse(readFileSync(`${source}/summary.json`));
const addon=loadNativeBoardPacker(),rows=[];
const equal=(a,b)=>{
    assert.deepEqual(a.states,b.states);
    assert.equal(a.rank.hardCount,b.rank.hardCount);
    assert.ok(Math.abs(a.rank.score-b.rank.score)<1e-7);
    assert.equal(a.checkpoints.length,b.checkpoints.length);
    a.checkpoints.forEach((c,i)=>{assert.equal(c.stage,b.checkpoints[i].stage);assert.deepEqual(c.states,b.checkpoints[i].states);
        assert.equal(c.rank.hardCount,b.checkpoints[i].rank.hardCount);assert.ok(Math.abs(c.rank.score-b.checkpoints[i].rank.score)<1e-7);});
};
for(const name of ['lte_power','usb_charge','current_iso']){
    const before=unpack(`${source}/${name}-geometric-0.json.gz`);
    const ps=unpack(`${source}/${name}-inputs.json.gz`).map(p=>({...p,experiments:{...p.experiments,routingMetric:'geometric'}}));
    const invoke=ps=>ps.length===1?[addon.solveBlockPrimitives(ps[0])]:addon.solveBlockPrimitivesBatch(ps,6);
    let t=performance.now();const initial=invoke(ps);const initialMs=performance.now()-t;
    initial.forEach((s,i)=>equal(s,before.initial[i]));
    const pairProblems=before.result.pairHypotheses.map(i=>{const p={...ps[i],pairSeed:initial[i].pairSeed};delete p.deferPairs;return p;});
    t=performance.now();const pairs=pairProblems.length?invoke(pairProblems):[];const pairsMs=performance.now()-t;
    pairs.forEach((s,i)=>equal(s,before.pairs[i]));
    const previous=old.rows.find(r=>r.name===name).geometric;
    const row={name,initialMs,pairsMs,ms:initialMs+pairsMs,beforeMs:previous.medianMs,beforeTimes:previous.times,
        identicalCheckpoints:true,metrics:previous.metrics};rows.push(row);
    writeFileSync(`${out}/${name}.json.gz`,gzipSync(JSON.stringify({initial,pairs,row})));
    writeFileSync(`${out}/summary.json`,JSON.stringify({nativeHash:createHash('sha256').update(readFileSync('native/pcb-board-packer/pcb-board-packer.win32-x64-msvc.node')).digest('hex'),
        detailProfile:Boolean(process.env.PCB_BLOCK_SOLVER_DETAIL),reference:source,rows},null,2));
    console.log(JSON.stringify(row));
}
