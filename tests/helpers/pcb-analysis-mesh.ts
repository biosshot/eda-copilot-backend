import { readFile, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { runAnalysisProblem, terminateAnalysisPool } from '../../src/analyze/run-analysis.ts';
const escape = (s: string) => s.replace(/[&<>"']/g, c => ({'&':'&amp;', '<':'&lt;', '>':'&gt;', '"':'&quot;', "'":'&apos;'})[c]!);
export async function prepareSavedAnalysisMesh() {
    try {
        const fixture = process.env.PCB_ANALYZE_FIXTURE_DIR, output = process.env.PCB_ANALYZE_RUN_DIR;
        if (!fixture || !output) throw Error('Use test:pcb-analyze -- <fixture> --name mesh');
        const boardJson = await readFile(join(fixture, `${basename(fixture)}.json`), 'utf8');
        const start = performance.now();
        // Diagnostic mesh: no assumed physical profile. Size is a scenario
        // parameter; zero angle disables quality refinement, not area checks.
        const report = await runAnalysisProblem({kind:'prepare-conduction', boardJson, mesh:{min_angle_deg:0}});
        const elapsedMs = performance.now() - start;
        const serialized = JSON.stringify(report);
        await writeFile(join(output,'mesh.json'), serialized + '\n');
        const {domains, ...summary} = report;
        await writeFile(join(output,'report.json'),JSON.stringify({...summary, domains:domains.map(({nodes,triangles,boundaries,terminals,...d}) => ({...d,nodes:nodes.length,triangles:triangles.length,boundaries:boundaries.length,terminals:terminals.map(({triangles,boundary_edges,...t})=>({...t,triangles:triangles.length,boundary_edges:boundary_edges.length}))})),elapsedMs,reportBytes:Buffer.byteLength(serialized),physicalProfile:'none; missing parameters retained'},null,2)+'\n');
        const board = JSON.parse(boardJson), outline: Array<{x:number;y:number}> = board.outline;
        const x=Math.min(...outline.map(p=>p.x))-1,y=Math.min(...outline.map(p=>p.y))-1,w=Math.max(...outline.map(p=>p.x))-x+1,h=Math.max(...outline.map(p=>p.y))-y+1;
        for (const [i,layer] of (board.layers as Array<{name:string}>).entries()) {
            const paths=domains.filter(d=>d.layer===layer.name).map(d=>{
                const triangles=d.triangles.map(t=>`M${d.nodes[t[0]]!.join(' ')}L${d.nodes[t[1]]!.join(' ')}L${d.nodes[t[2]]!.join(' ')}Z`).join('');
                const bounds=d.boundaries.map(b=>`M${d.nodes[b.nodes[0]]!.join(' ')}L${d.nodes[b.nodes[1]]!.join(' ')}`).join('');
                const terminals=new Set(d.terminals.flatMap(t=>t.boundary_edges));
                const terminal=d.boundaries.filter((_,i)=>terminals.has(i)).map(b=>`M${d.nodes[b.nodes[0]]!.join(' ')}L${d.nodes[b.nodes[1]]!.join(' ')}`).join('');
                return `<g data-domain="${d.id}"><title>${escape(d.readiness.reasons.join(', '))}</title><path d="${triangles}" fill="#493f29" stroke="#ae954d" stroke-width="0.012"/><path d="${bounds}" fill="none" stroke="#62ceeb" stroke-width="0.025"/><path d="${terminal}" fill="none" stroke="#ec749a" stroke-width="0.035"/></g>`;
            }).join('\n');
            await writeFile(join(output,`layer-${i}-${layer.name.replace(/[^a-zA-Z0-9_-]/g,'_')}.svg`),`<svg xmlns="http://www.w3.org/2000/svg" viewBox="${x} ${y} ${w} ${h}" width="1400" height="1000"><title>${escape(`${basename(fixture)} ${layer.name}: diagnostic mesh, no assumed materials`)}</title><rect x="${x}" y="${y}" width="${w}" height="${h}" fill="#101820"/>${paths}</svg>\n`);
        }
        console.log(JSON.stringify({status:report.status,counts:report.counts,elapsedMs,bytes:Buffer.byteLength(serialized),diagnostics:report.diagnostics.reduce<Record<string,number>>((c,d)=>(c[d.code]=(c[d.code]??0)+1,c),{})}));
        if(report.status==='invalid')process.exitCode=1;
    } finally { await terminateAnalysisPool(); }
}
