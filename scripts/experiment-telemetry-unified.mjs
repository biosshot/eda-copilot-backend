import { readFileSync, writeFileSync, mkdirSync, createWriteStream, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { spawn } from 'node:child_process';
import { buildPlacementGraph } from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import { solvePlacementSubtreeSync } from '../src/pcb-layout/pcb-auto-place-v2/tree-solver.ts';
import { validatePrimitive } from '../src/pcb-layout/pcb-auto-place/primitive-validation.ts';
import { getPadWorld } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { renderPlacementSubsetSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import { withBlockCandidateCapture } from '../src/pcb-layout/pcb-auto-place-v2/block-quality.ts';
import { placementMetrics } from './experiment-placement-metrics.mjs';

export const out = 'docs/experiments/telemetry-unified-2026-09-27';
export const blocks = ['current_iso', 'usb_charge', 'lte_power', 'logic_power', 'adc'];
export const variants = [{id:'unified', label:'Unified checkpoint portfolio'}];

if (process.argv[2] === '--worker') {
    const [block, tag] = process.argv.slice(3), variant = variants.find(v => v.id === tag);
    if (!blocks.includes(block) || !variant) throw Error('Unknown block/variant');
    Object.assign(process.env, { PCB_NATIVE_SOLVE_CACHE: '0', PCB_POST_PLACE_THREADS: '1' });
    const raw = readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json');
    const input = JSON.parse(raw), sourceInput = structuredClone(input);
    const graph = buildPlacementGraph(input);
    const find = n => n.kind === 'block' && n.label === block ? n : n.children.map(find).find(Boolean);
    const node = find(graph.root);
    if (!node) throw Error(`Missing ${block}`);
    const captures = [], pools = [];
    const addon = loadNativeBoardPacker(), nativeSolve = addon.solveBlockPrimitives;
    const nativeHash = createHash('sha256').update(readFileSync('native/pcb-board-packer/pcb-board-packer.win32-x64-msvc.node')).digest('hex');
    mkdirSync('.test-output/unified-native-cache',{recursive:true});
    addon.solveBlockPrimitives = problem => {
        const key = createHash('sha256').update(nativeHash + JSON.stringify(problem)).digest('hex');
        const path = `.test-output/unified-native-cache/${key}.json`;
        // Explicit offline replay only: caller must establish solver compatibility.
        // Production caching remains keyed by its complete input and addon instance.
        const compatibleHash = process.env.PCB_EXPERIMENT_REPLAY_NATIVE_HASH;
        const compatibleKey = compatibleHash && createHash('sha256').update(compatibleHash + JSON.stringify(problem)).digest('hex');
        const compatiblePath = compatibleKey && `.test-output/unified-native-cache/${compatibleKey}.json`;
        const replayPath = existsSync(path) ? path : compatiblePath && existsSync(compatiblePath) ? compatiblePath : null;
        const cached = Boolean(replayPath), start = performance.now();
        const solution = cached ? JSON.parse(readFileSync(replayPath)) : nativeSolve(problem);
        if (!cached) writeFileSync(path,JSON.stringify(solution));
        captures.push({key,cached,replayedFrom:replayPath === compatiblePath ? compatibleHash : undefined,ms:performance.now()-start,components:problem.components.map(c=>c.designator)});
        console.log(JSON.stringify({event:'native',block,count:problem.components.length,cached,ms:performance.now()-start}));
        return solution;
    };
    const start = performance.now();
    const result = withBlockCandidateCapture((label, pool, selected) => {
        pools.push({label, candidates:pool.map(c=>({stage:c.stage,hypothesis:c.hypothesis,quality:c.quality,primitives:c.primitives,
            placements:c.primitives.flatMap(p=>p.placements), selected:selected.includes(c)}))});
    }, () => solvePlacementSubtreeSync({ input, graph, node }));
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
    const data={block,variant,ms,sourceInputHash:createHash('sha256').update(raw).digest('hex'),
        inputHash:createHash('sha256').update(JSON.stringify(input)).digest('hex'),c9Role:input.components.find(c=>c.designator==='C9').pcb.role,sourceUnchanged:JSON.stringify(input)===JSON.stringify(sourceInput),
        nativeHash:createHash('sha256').update(readFileSync('native/pcb-board-packer/pcb-board-packer.win32-x64-msvc.node')).digest('hex'),inventory,orientation,validation,
        metrics:placementMetrics(input,placements),allMetrics:placementMetrics(allInput,placements),pairs,localHardPairs,hardPairsOk,
        placements,diagnostics:result.diagnostics,captures,pools,alternatives:result.root.layoutAlternatives?.map(p=>({placements:p.placements,quality:p.blockQuality}))};
    mkdirSync(`${out}/${block}`,{recursive:true});
    writeFileSync(`${out}/${block}/${tag}.json`,JSON.stringify(data,null,2));
    for(const all of [false,true])writeFileSync(`${out}/${block}/${tag}${all?'-with-ignored':''}.svg`,renderPlacementSubsetSvg(input,placements,{ratsnestTopology:'mst',signalPaths:false,includeIgnoredSignals:all,padding:2}).replace(/[ \t]+$/gm,''));
    console.log(JSON.stringify({block,tag,ms,inventory,orientation,ok:validation.ok,metrics:data.metrics,pairs,released:result.diagnostics.filter(d=>d.message.startsWith('Independent'))}));
} else if (process.argv[1]?.endsWith('experiment-telemetry-unified.mjs')) {
    const requested=process.argv.slice(2), chosen=process.env.PCB_EXPERIMENT_VARIANTS?.split(',');
    const queue=blocks.filter(b=>!requested.length||requested.includes(b)).flatMap(b=>variants.filter(v=>!chosen||chosen.includes(v.id)).map(v=>[b,v.id]));
    mkdirSync('.test-output/telemetry-unified',{recursive:true});
    mkdirSync(out,{recursive:true});
    const statuses=[];
    await Promise.all([0,1,2,3].map(async()=>{while(queue.length){const [block,tag]=queue.shift();
        const result=await new Promise(resolve=>{const log=createWriteStream(`.test-output/telemetry-unified/${block}-${tag}.log`);
            const child=spawn(process.execPath,['--import','tsx',import.meta.filename,'--worker',block,tag],{windowsHide:true,stdio:['ignore','pipe','pipe']});
            child.stdout.pipe(log);child.stderr.pipe(log);child.on('error',e=>resolve({block,tag,error:String(e)}));child.on('exit',code=>{log.end();resolve({block,tag,code});});});
        statuses.push(result);console.log(JSON.stringify(result));writeFileSync(`${out}/runs.json`,JSON.stringify(statuses,null,2));
    }}));
    if(statuses.some(s=>s.code!==0))process.exitCode=1;
}
