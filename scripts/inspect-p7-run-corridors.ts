import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import { localRequest } from '../src/analyze/analysis-dsl/local-request.ts';
import { boardChannelPlan } from '../src/analyze/board-channel-plan.ts';
import {capturedBoardGeometry} from '../src/analyze/geometry-snapshot.ts';
import {buildRouteGraph,project,tangent,type Point} from '../src/analyze/route-partition.ts';
import {proposeCorridors} from '../src/analyze/run-corridors.ts';
const folder = 'tests/pcb-analyze/STM32G0_CAN_RS485/', board = JSON.parse(readFileSync(folder + 'STM32G0_CAN_RS485.json', 'utf8')), code = readFileSync(folder + 'local-extraction-complete-channel.js', 'utf8').split('a.plot(')[0];
const started = performance.now();
const result = await executeScenario({ board, code }, { filename: 'corridor-geometry-evidence.js', maxEntries: 1000, maxOutputBytes: 4 * 1024 * 1024 }, () => { }, async (task) => {
    let contacts: any;
    const base = localRequest(task as any, p => contacts = p.contacts);
    try {
        const plan = boardChannelPlan(board, base, contacts);
        mkdirSync('debugging/p7-channel-plan', { recursive: true });
        writeFileSync('debugging/p7-channel-plan/corridor-plan.json', JSON.stringify(plan));
        console.log(JSON.stringify({ status: 'planned', blocks: plan.blocks.map(b => ({ id: b.id, kind: b.kind })) }));
    }
    catch (error) {
        const e = error as any;
        if (!e.corridorAudit)
            throw error;
        const evidence = { status: 'refused', code: e.code, message: e.message, wallSeconds: (performance.now() - started) / 1000, ...e.corridorAudit };
        if(process.argv.includes('--full-return-support')){
            const g=capturedBoardGeometry(JSON.stringify(board)),signal=g.islands.find(i=>i.id===evidence.signalIsland)!;
            const returns=new Set(g.islands.filter(i=>evidence.returnIslands.includes(i.id)).flatMap(i=>i.region_ids));
            const points=g.regions.filter(r=>returns.has(r.id)).flatMap(r=>r.polygon.outer) as Point[];
            const graph=buildRouteGraph(board,g,new Set(signal.region_ids),0);
            const runs=graph.runs.map(run=>{const t=tangent(run.direction),centre=project(run.from,t),halfWidthMm=Math.max(...points.map(p=>Math.abs(project(p,t)-centre)))+Math.max(1e-6,4*g.boolean_error_bound_mm),c=proposeCorridors(g,[run],halfWidthMm)[0];return {id:run.id,halfWidthMm,events:c.events.length,uniformIntervalsMm:c.uniformIntervalsMm,admissibleIntervalsMm:c.admissibleIntervalsMm,lateralContacts:c.lateralContinuations.length};});
            const fullReturnAudit={scope:'Geometry only: complete captured finite return transverse support; no fields or accuracy claim',runs};
            mkdirSync('debugging/p7-channel-plan',{recursive:true});writeFileSync('debugging/p7-channel-plan/full-plane-corridor-audit.json',JSON.stringify(fullReturnAudit,null,2));console.log(JSON.stringify(fullReturnAudit));
        }
        mkdirSync('debugging/p7-channel-plan', { recursive: true });
        writeFileSync('debugging/p7-channel-plan/corridor-refusal.json', JSON.stringify(evidence, null, 2));
        console.log(JSON.stringify({ status: evidence.status, code: e.code, seconds: evidence.wallSeconds, runs: evidence.corridors.map((c: any) => ({ id: c.runId, lengthMm: c.longitudinalMm[1] - c.longitudinalMm[0], halfWidthMm: (c.transverseMm[1] - c.transverseMm[0]) / 2, uniformIntervals: c.uniformIntervalsMm.length, materialEvents: c.events.filter((e: any) => e.kind === 'material').length, lateralContacts: c.lateralContinuations.length, returnPlaneSources: [...new Set(c.lateralContinuations.filter((v: any) => v.sourcePath.includes('zones')).map((v: any) => v.sourcePath))] })) }));
    }
    return { id: 'geometry', status: 'complete', method: 'geometry evidence only; no physical extraction', diagnostics: [], assumptions: [], metrics: {} };
});
if (result.status !== 'complete')
    throw new Error(JSON.stringify(result.error));
