// Historical A/B harness: its environment modes were retired by the unified policy.
if (process.argv[1]?.endsWith('experiment-telemetry-c9-trace.mjs')) throw new Error('Archived placement experiment: replay on commit 90a77cf; use experiment-telemetry-unified.mjs on this branch.');
import { readFileSync, writeFileSync, mkdirSync, createWriteStream } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { buildPlacementGraph } from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import { solvePlacementSubtreeSync } from '../src/pcb-layout/pcb-auto-place-v2/tree-solver.ts';
import { validatePrimitive } from '../src/pcb-layout/pcb-auto-place/primitive-validation.ts';
import { getPadWorld } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { renderPlacementSubsetSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import { placementMetrics } from './experiment-placement-metrics.mjs';

export const out = 'docs/experiments/telemetry-c9-trace-2026-09-27';
export const blocks = ['current_iso'];
export const variants = [
    { id: 'base', label: 'До: прежняя очередность' },
    { id: 'equal', label: 'Равный бонус критических пар', equal: true },
    { id: 'core', label: 'Прямые связи с ядром', core: true },
    { id: 'branch', label: 'Три варианта следующей детали', branch: true },
    { id: 'scarcity', label: 'Два вывода и дефицит мест', scarcity: true },
    { id: 'combined', label: 'Все четыре изменения', equal: true, core: true, branch: true, scarcity: true },
    { id: 'combined-c9-signal', label: 'Контроль: C9 не powerComponent', equal: true, core: true, branch: true, scarcity: true, c9Signal: true },
].filter(v=>['base','combined','combined-c9-signal'].includes(v.id)).map(v=>({...v, frontier: true, owner: true, access: true, groups: 'all'}));

if (process.argv[2] === '--worker') {
    const [block, tag] = process.argv.slice(3), variant = variants.find(v => v.id === tag);
    if (!blocks.includes(block) || !variant) throw Error('Unknown block/variant');
    Object.assign(process.env, { PCB_BLOCK_TRACE_C9: '1', PCB_BLOCK_PROFILE: 'full', PCB_BLOCK_ROUTING: 'micro', PCB_BLOCK_CANDIDATES: '2',
        PCB_BLOCK_POST_REFINE: '1', PCB_PLACEMENT_PAD_CROSSINGS: '1', PCB_BLOCK_PORTFOLIO: '0',
        PCB_BLOCK_FRONTIER: variant.frontier ? '1' : '0', PCB_BLOCK_PAD_OWNER: variant.owner ? '1' : '0',
        PCB_BLOCK_LOCAL_ACCESS: variant.access ? '1' : '0',
        PCB_BLOCK_ORDER_EQUAL: variant.equal ? '1' : '0', PCB_BLOCK_ORDER_CORE: variant.core ? '1' : '0',
        PCB_BLOCK_ORDER_BRANCH: variant.branch ? '1' : '0', PCB_BLOCK_ORDER_SCARCITY: variant.scarcity ? '1' : '0',
        PCB_BLOCK_RELAX_GROUPS: variant.groups, PCB_NATIVE_SOLVE_CACHE: '0', PCB_POST_PLACE_THREADS: '1' });
    const raw = readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json');
    const input = JSON.parse(raw), graph = buildPlacementGraph(input);
    const find = n => n.kind === 'block' && n.label === block ? n : n.children.map(find).find(Boolean);
    const node = find(graph.root);
    if (!node) throw Error(`Missing ${block}`);
    const captures = [], addon = loadNativeBoardPacker(), original = addon.solveBlockPrimitives;
    addon.solveBlockPrimitives = problem => {
        if(variant.c9Signal) {
            problem=structuredClone(problem);
            const c=problem.components.find(c=>c.designator==='C9');
            if(c)c.powerComponent=false;
        }
        const traced = ['C9','U2'].every(d=>problem.primitives.some(p=>p.placements.some(q=>q.designator===d)));
        if(traced) { mkdirSync(`${out}/${block}`,{recursive:true}); writeFileSync(`${out}/${block}/${tag}-problem.json`,JSON.stringify(problem,null,2)); }
        const start = performance.now(), solution = original(problem);
        if(traced) writeFileSync(`${out}/${block}/${tag}-native.json`,JSON.stringify(solution,null,2));
        captures.push({ ms: performance.now()-start, rank: solution.rank,
            members: problem.primitives.map(p=>({label:p.label, members:p.placements.map(q=>q.designator)})),
            order: solution.states.map(s=>s.placements.map(p=>p.designator)),
            relations: problem.relations, experiments: problem.experiments });
        return solution;
    };
    const originalRefine = addon.refinePostPlacement;
    addon.refinePostPlacement = problem => {
        const result = originalRefine(problem);
        if(['C9','U2'].every(d=>problem.components.some(c=>c.designator===d)))
            writeFileSync(`${out}/${block}/${tag}-postrefine.json`,JSON.stringify(result,null,2));
        return result;
    };
    const start = performance.now(), result = solvePlacementSubtreeSync({ input, graph, node });
    const ms = performance.now()-start, placements=result.root.placements;
    const validation=validatePrimitive(input,result.root,{checkFixed:true});
    const names = new Set(); const collect=n=>{if(n.kind==='component') names.add(n.label);n.children.forEach(collect);};collect(node);
    const inventory = placements.length===names.size && new Set(placements.map(p=>p.designator)).size===names.size && placements.every(p=>names.has(p.designator));
    const orientation = placements.every(p=>{const c=input.components.find(c=>c.designator===p.designator);return c.pcb.allowedRotations.includes(p.rotate)&&c.pcb.allowedLayers.includes(p.layer);});
    const distance = (a,ap,b,bp) => { const point=(ref,pin)=>{const c=input.components.find(c=>c.designator===ref),p=placements.find(p=>p.designator===ref);return p&&getPadWorld(c,p,pin);};const x=point(a,ap),y=point(b,bp);return x&&y?Math.hypot(x.x-y.x,x.y-y.y):null; };
    const pairs=[['C9','1','U2','6'],['C9','2','U2','7'],['C33','1','U10','3'],['U10','11','L1','1'],['U10','9','L1','2'],['U12','4','L2','1'],['U12','2','L2','2'],['C13','1','U2','1'],['C15','1','U2','5']]
        .map(p=>({pair:p.join('.'),mm:distance(...p)})).filter(p=>p.mm!==null);
    const localHardPairs=input.hints.filter(h=>h.relation==='critical_pair'&&h.hard)
        .map(h=>({hint:h,mm:h.source?.type==='pin'&&h.target?.type==='pin'?distance(h.source.designator,h.source.pin_number,h.target.designator,h.target.pin_number):null})).filter(h=>h.mm!==null);
    const hardPairsOk=localHardPairs.every(p=>p.hint.maxDistance==null||p.mm<=p.hint.maxDistance+1e-3);
    const allInput={...input,solverOptions:{...input.solverOptions,ignoredRatsnestSignals:[]}};
    const data={block,variant,ms,inputHash:createHash('sha256').update(raw).digest('hex'),
        nativeHash:createHash('sha256').update(readFileSync('native/pcb-board-packer/pcb-board-packer.win32-x64-msvc.node')).digest('hex'),inventory,orientation,validation,
        metrics:placementMetrics(input,placements),allMetrics:placementMetrics(allInput,placements),pairs,localHardPairs,hardPairsOk,
        placements,diagnostics:result.diagnostics,captures};
    mkdirSync(`${out}/${block}`,{recursive:true});
    writeFileSync(`${out}/${block}/${tag}.json`,JSON.stringify(data,null,2));
    for(const all of [false,true])writeFileSync(`${out}/${block}/${tag}${all?'-with-ignored':''}.svg`,renderPlacementSubsetSvg(input,placements,{ratsnestTopology:'mst',signalPaths:false,includeIgnoredSignals:all,padding:2}).replace(/[ \t]+$/gm,''));
    console.log(JSON.stringify({block,tag,ms,inventory,orientation,ok:validation.ok,metrics:data.metrics,pairs,released:result.diagnostics.filter(d=>d.message.startsWith('Experimental'))}));
} else if (process.argv[1]?.endsWith('experiment-telemetry-c9-trace.mjs')) {
    const requested=process.argv.slice(2), chosen=process.env.PCB_EXPERIMENT_VARIANTS?.split(',');
    const queue=blocks.filter(b=>!requested.length||requested.includes(b)).flatMap(b=>variants.filter(v=>!chosen||chosen.includes(v.id)).map(v=>[b,v.id]));
    mkdirSync('.test-output/telemetry-c9-trace',{recursive:true});
    mkdirSync(out,{recursive:true});
    const statuses=[];
    await Promise.all([0,1].map(async()=>{while(queue.length){const [block,tag]=queue.shift();
        const result=await new Promise(resolve=>{const log=createWriteStream(`.test-output/telemetry-c9-trace/${block}-${tag}.log`);
            const child=spawn(process.execPath,['--import','tsx',import.meta.filename,'--worker',block,tag],{windowsHide:true,stdio:['ignore','pipe','pipe']});
            child.stdout.pipe(log);child.stderr.pipe(log);child.on('error',e=>resolve({block,tag,error:String(e)}));child.on('exit',code=>{log.end();resolve({block,tag,code});});});
        statuses.push(result);console.log(JSON.stringify(result));writeFileSync(`${out}/runs.json`,JSON.stringify(statuses,null,2));
    }}));
    if(statuses.some(s=>s.code!==0))process.exitCode=1;
}
