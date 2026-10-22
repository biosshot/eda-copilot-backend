import { readFile, writeFile } from 'node:fs/promises';
import { basename, join } from 'node:path';
import { performance } from 'node:perf_hooks';
import { runAnalysisProblem, terminateAnalysisPool, type AnalysisPolygon } from '../../src/analyze/run-analysis.ts';

const escape = (v: string) => v.replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&apos;' })[c]!);
function path(p: AnalysisPolygon) {
    return [p.outer, ...p.holes].map(ring => ring.map((p, i) => `${i ? 'L' : 'M'}${p[0]} ${p[1]}`).join(' ') + 'Z').join(' ');
}

export async function prepareSavedAnalysisGeometry() {
    try {
        const fixtureDir = process.env.PCB_ANALYZE_FIXTURE_DIR;
        const runDir = process.env.PCB_ANALYZE_RUN_DIR;
        if (!fixtureDir || !runDir) throw new Error('Run with npm run test:pcb-analyze -- <fixture> --name geometry');
        const boardJson = await readFile(join(fixtureDir, `${basename(fixtureDir)}.json`), 'utf8');
        const start = performance.now();
        const report = await runAnalysisProblem({ kind: 'prepare-geometry', boardJson });
        const elapsedMs = performance.now() - start;
        const serialized = JSON.stringify(report, null, 2) + '\n';
        await writeFile(join(runDir, 'report.json'), serialized);
        const { units, coordinates, outline, regions, reliable_regions, possible_regions, possible_contacts, possible_islands, holes, layers, findings, contacts, islands } = report;
        await writeFile(join(runDir, 'geometry.json'), JSON.stringify({ units, coordinates, outline, layers, regions, reliable_regions, possible_regions, possible_contacts, possible_islands, holes, findings, contacts, islands }) + '\n');
        const points = outline?.outer ?? [[0, 0], [10, 10]];
        const xs = points.map(p => p[0]), ys = points.map(p => p[1]);
        const x = Math.min(...xs) - 1, y = Math.min(...ys) - 1, w = Math.max(...xs) - x + 1, h = Math.max(...ys) - y + 1;
        const highlighted = new Set(findings.filter(f => f.code === 'physical_short').flatMap(f => f.region_ids));
        const separated = new Set(findings.filter(f => f.code === 'net_islands').flatMap(f => f.region_ids));
        for (const [index, layer] of layers.entries()) {
            const copper = regions.filter(r => r.layer === layer.layer).map(r => `<path data-region="${r.id}" data-source="${escape(r.source.source_id ?? r.source.path)}" d="${path(r.polygon)}" fill="${highlighted.has(r.id) ? '#ed5555' : '#c89f44'}"${separated.has(r.id) ? ' stroke="#53b9d9" stroke-width="0.025"' : ''} fill-rule="evenodd"><title>${escape(`${r.id} ${r.source.source_id ?? r.source.path} net=${r.source.net ?? 'unknown'}${separated.has(r.id) ? ' / net occurs in separate islands' : ''}`)}</title></path>`).join('\n');
            const envelope = possible_regions.filter(r => r.layer === layer.layer).map(r => `<path data-possible="${r.id}" d="${path(r.polygon)}" fill="#a06bcb" fill-opacity="0.5" fill-rule="evenodd"/>`).join('');
            const core = reliable_regions.filter(r => r.layer === layer.layer).map(r => `<path data-reliable="${r.id}" data-source="${escape(r.source.source_id ?? r.source.path)}" d="${path(r.polygon)}" fill="${highlighted.has(r.id) ? '#ed5555' : '#57b899'}" fill-opacity="0.6"${separated.has(r.id) ? ' stroke="#53b9d9" stroke-width="0.025"' : ''} fill-rule="evenodd"><title>${escape(`${r.id}: reliable core of ${r.source.path}`)}</title></path>`).join('');
            const drills = holes.filter(h => h.layers.includes(layer.layer)).flatMap(h => h.polygons.map(p => `<path data-drill="${escape(h.id)}" d="${path(p)}" fill="#101820" stroke="${h.plated ? '#64c5e8' : '#aaa'}" stroke-width="0.025" fill-rule="evenodd"/>`)).join('\n');
            const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="${x} ${y} ${w} ${h}" width="1200" height="900"><title>${escape(`${basename(fixtureDir)} ${layer.layer}: ${report.status}, mm / Y-down; gold=nominal, green=reliable, purple=possible`)}</title><rect x="${x}" y="${y}" width="${w}" height="${h}" fill="#101820"/>${outline ? `<path d="${path(outline)}" fill="#23313a" stroke="#ddd" stroke-width="0.06" fill-rule="evenodd"/>` : ''}${envelope}${copper}${core}${drills}</svg>\n`;
            await writeFile(join(runDir, `layer-${index}-${layer.layer.replace(/[^a-zA-Z0-9_-]/g, '_')}.svg`), svg);
        }
        const connectivity = { reliableContacts: contacts.filter(c => c.reliable).length, uncertainContacts: contacts.filter(c => !c.reliable).length,
            possibleContacts: possible_contacts.length, reliableIslands: islands.length, possibleIslands: possible_islands.length,
            uncertainComponents: report.statistics.uncertain_components, findings: Object.fromEntries([...new Set(findings.map(f => f.code))].map(code => [code, findings.filter(f => f.code === code).length])) };
        const measurements = { connectivity, status: report.status, elapsedMs, reportBytes: Buffer.byteLength(serialized), statistics: report.statistics,
            layers: layers.map(l => ({ layer: l.layer, areaMm2: l.union_area_mm2, regions: l.regions })),
            diagnosticCodes: Object.fromEntries([...new Set(report.diagnostics.map(d => d.code))].map(code => [code, report.diagnostics.filter(d => d.code === code).length])) };
        await writeFile(join(runDir, 'measurements.json'), JSON.stringify(measurements, null, 2) + '\n');
        console.log(JSON.stringify(measurements));
        if (report.status === 'invalid') process.exitCode = 1;
    } catch (error) { console.error(error); process.exitCode = 1; }
    finally { await terminateAnalysisPool(); }
}
