// Read-only acceptance evidence. Run after Rust reports and the geometry launcher.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, readdirSync, writeFileSync, statSync, existsSync } from 'node:fs';
import { createRequire } from 'node:module';
import os from 'node:os';
import { join, resolve } from 'node:path';
import { isDeepStrictEqual } from 'node:util';
const root = resolve('debugging/copper-geometry');
const read = p => JSON.parse(readFileSync(p, 'utf8'));
const names = ['BandAmpV5', 'ESPower', 'NanoScope', 'NanoSpec', 'STM32G0_CAN_RS485', 'TPS63060DSCR', 'ThunderF722', 'USB_PD_Trigger_5A'];
const hash = p => createHash('sha256').update(readFileSync(p)).digest('hex');
const results = [];
const reports = new Map();
for (const name of names) {
    const runs = join('debugging/pcb-analyze', name, 'runs');
    const last = readdirSync(runs).filter(r => r.includes('-geometry-')).sort().at(-1);
    assert.ok(last, `${name}: run geometry launcher first`);
    const dir = join(runs, last), report = read(join(dir, 'report.json'));
    assert.ok(isDeepStrictEqual(report, read(join(root, 'rust', name + '.json'))), `${name}: Node process and standalone Rust full-report parity`);
    assert.equal(report.status, 'partial', `${name}: captured freshness remains unknown`);
    assert.equal(report.excluded.length, 0, `${name}: no excluded geometry in captured corpus`);
    assert.equal(readdirSync(dir).filter(f => f.endsWith('.svg')).length, report.layers.length);
    const geometry = read(join(dir, 'geometry.json'));
    assert.deepEqual(geometry.regions, report.regions);
    for (const key of ['reliable_regions', 'possible_regions', 'possible_contacts', 'possible_islands']) assert.deepEqual(geometry[key], report[key]);
    const reliableIds = new Set(report.reliable_regions.map(r => r.id));
    const possibleIds = new Set(report.possible_regions.map(r => r.id));
    assert.ok(report.islands.every(i => i.region_ids.every(id => reliableIds.has(id))));
    assert.ok(report.possible_islands.every(i => i.region_ids.every(id => possibleIds.has(id))));
    assert.ok(report.possible_contacts.every(c => !c.reliable));
    reports.set(name, report);
    const measurements = read(join(dir, 'measurements.json'));
    results.push({ name, run: dir, ...measurements, reportSha256: hash(join(dir, 'report.json')), geometryBytes: statSync(join(dir, 'geometry.json')).size,
        svgBytes: readdirSync(dir).filter(f => f.endsWith('.svg')).reduce((n, f) => n + statSync(join(dir, f)).size, 0) });
}
// Independent ray casting and shoelace checks, not the production Rust predicates.
function inside(point, ring) {
    let yes = false;
    for (let i = 0, j = ring.length - 1; i < ring.length; j = i++) {
        const a = ring[i], b = ring[j];
        if ((a[1] > point[1]) !== (b[1] > point[1]) && point[0] < (b[0] - a[0]) * (point[1] - a[1]) / (b[1] - a[1]) + a[0]) yes = !yes;
    }
    return yes;
}
const area = ring => Math.abs(ring.reduce((sum, a, i) => { const b = ring[(i + 1) % ring.length]; return sum + a[0] * b[1] - a[1] * b[0]; }, 0) / 2);
const near = (a, b, e = 1e-8) => assert.ok(Math.abs(a - b) <= e, `${a} != ${b}, bound ${e}`);
const checks = [];
for (const name of ['BandAmpV5', 'ESPower']) {
    const dir = join('tests/pcb-analyze', name), input = read(join(dir, name + '.json'));
    const raw = read(join(dir, 'export-evidence.json')).capture.pcb;
    const source = readFileSync(join(dir, 'source-pcb.txt'), 'utf8');
    const records = source.trim().split(/\r?\n/).map(line => { const [head, body] = line.split('||'); return { head: JSON.parse(head), body: JSON.parse(body.replace(/\|$/, '')) }; });
    const report = reports.get(name);
    const pair = name === 'BandAmpV5' ? ['1ef61450f2f63c6ee14', '73d6db5f0c9d2233'] : ['e536e19', '581f781a134fbbbf'];
    const pad = input.pads.find(p => p.id === pair[0]), track = input.copper.fixed.tracks.find(t => t.id === pair[1]);
    const nativePad = raw.pads.find(p => p.id === pad.id), nativeTrack = raw.tracks.find(t => t.id === track.id);
    const padNet = records.find(({head}) => { if (head.type !== 'PAD_NET') return false; const id = JSON.parse(head.id); return id[1] + id[3] === pad.id && id[2] === pad.number; });
    assert.ok(padNet && source.includes(track.id));
    assert.equal(padNet.body.padNet, nativePad.net);
    near(pad.at.x, nativePad.x); near(pad.at.y, -nativePad.y); near(pad.rotationDeg, -nativePad.rotation * 180 / Math.PI);
    assert.equal(nativePad.shape[0], 'RECT'); near(pad.shape.widthMm, nativePad.shape[1]); near(pad.shape.heightMm, nativePad.shape[2]);
    near(track.widthMm, nativeTrack.width); near(track.points[0].x, nativeTrack.x1); near(track.points[0].y, -nativeTrack.y1);
    const endpoint = track.points.find(q => {
        const a = -pad.rotationDeg * Math.PI / 180, dx = q.x - pad.at.x, dy = q.y - pad.at.y;
        return Math.abs(dx * Math.cos(a) - dy * Math.sin(a)) < pad.shape.widthMm / 2 - 0.01
            && Math.abs(dx * Math.sin(a) + dy * Math.cos(a)) < pad.shape.heightMm / 2 - 0.01;
    });
    assert.ok(endpoint, 'Native track endpoint is strictly within the native rectangular pad');
    const padIds = report.reliable_regions.filter(r => r.source.source_id === pad.id).map(r => r.id);
    const trackIds = report.reliable_regions.filter(r => r.source.source_id === track.id).map(r => r.id);
    assert.ok(report.islands.some(i => padIds.some(id => i.region_ids.includes(id)) && trackIds.some(id => i.region_ids.includes(id))));
    const via = raw.vias[0], exportedVia = input.copper.fixed.vias.find(v => v.id === via.id);
    const nativeVia = records.find(r => r.head.type === 'VIA' && r.head.id === via.id).body;
    const nativeDimensionsMm = { diameter: nativeVia.viaDiameter * 0.0254, drill: nativeVia.holeDiameter * 0.0254 };
    const captureDeltasMm = { diameter: via.diameter - nativeDimensionsMm.diameter, drill: via.drill - nativeDimensionsMm.drill };
    near(exportedVia.at.x, via.x); near(exportedVia.at.y, -via.y); near(exportedVia.drillMm, via.drill); near(exportedVia.diameterMm, via.diameter);
    const lands = report.regions.filter(r => r.source.kind === 'via' && r.source.source_id === via.id);
    assert.equal(lands.length, 2);
    const reliableLands = report.reliable_regions.filter(r => r.source.kind === 'via' && r.source.source_id === via.id);
    assert.equal(reliableLands.length, 2); assert.ok(report.islands.some(i => reliableLands.every(r => i.region_ids.includes(r.id))));
    const ringExpected = Math.PI / 4 * (via.diameter ** 2 - via.drill ** 2);
    for (const land of lands) near(land.area_mm2, ringExpected, Math.PI * (via.diameter + via.drill) * report.options.max_deviation_mm + 1e-5);
    // These native hole contours consist of exactly two 180-degree arcs. Their
    // circle follows directly from the native endpoints, without the exporter.
    const nativeFill = raw.polygons.find(p => p.sources.some(s => s.length === 10 && s[2] === 'ARC' && s[6] === 'ARC' && s[3] === 180 && s[7] === 180));
    const circle = nativeFill.sources.find(s => s.length === 10 && s[2] === 'ARC' && s[6] === 'ARC' && s[3] === 180 && s[7] === 180);
    const center = [(circle[0] + circle[4]) / 2, -(circle[1] + circle[5]) / 2];
    const radius = Math.hypot(circle[0] - circle[4], circle[1] - circle[5]) / 2;
    const fills = report.regions.filter(r => r.source.native_fill_ids.includes(nativeFill.id) || r.source.source_id === nativeFill.pourId);
    const hole = fills.flatMap(r => r.polygon.holes).find(h => inside(center, h) && h.every(p => Math.abs(Math.hypot(p[0] - center[0], p[1] - center[1]) - radius) < 0.00101));
    assert.ok(hole, `${name}: native circular fill void preserved`);
    assert.ok(fills.every(r => !inside(center, r.polygon.outer) || r.polygon.holes.some(h => inside(center, h))), 'Native fill hole centre is not copper');
    near(area(hole), Math.PI * radius ** 2, 2 * Math.PI * radius * 0.001 + 1e-4);
    checks.push({ name, evidence: join(dir, 'export-evidence.json'), evidenceSha256: hash(join(dir, 'export-evidence.json')),
        pad: { id: pad.id, at: pad.at, shape: pad.shape }, track: { id: track.id, width: track.widthMm, endpoint },
        via: { id: via.id, at: exportedVia.at, diameter: via.diameter, drill: via.drill, expectedRingArea: ringExpected, layers: lands.map(r => r.layer), nativeDimensionsMm, captureDeltasMm,
            limitation: 'V2 exactly preserves capture API dimensions; selected source-pcb VIA record dimensions differ. CAD equivalence is not asserted or silently repaired.' },
        nativeFill: { id: nativeFill.id, pourId: nativeFill.pourId, transportNativeIds: [...new Set(fills.flatMap(f => f.source.native_fill_ids))], sourceCircle: circle, centerYDown: center, radius, expectedHoleArea: Math.PI * radius ** 2, preparedHoleArea: area(hole),
            limitation: 'Selected V2 nativePrimitiveIds retain the pour/poured-group ID rather than the raw fragment ID; fragments are matched independently by captured pourId. No missing fragment ID is invented.' },
        conclusions: ['native pad/track overlap is a physical island', 'through via lands share one physical island', 'native fill circular hole is absent material'] });
}
let resourceMeasurement = null;
if (existsSync(join(root, 'process-resources.json'))) {
    resourceMeasurement = read(join(root, 'process-resources.json'));
    const pids = readFileSync(join(root, 'worker-pids.jsonl'), 'utf8').trim().split(/\r?\n/).filter(Boolean).map(s => JSON.parse(s).pid);
    const log = readFileSync(join(root, 'launcher.log'), 'utf8');
    assert.equal(pids.length, results.length, 'One measured native worker per fixture');
    for (const [i, result] of results.entries()) {
        assert.ok(log.includes(result.run.split(/[\\/]/).at(-1)), 'Resource measurements must belong to these exact runs');
        const worker = resourceMeasurement.workers.find(w => w.pid === pids[i]);
        assert.ok(worker, 'Measured worker PID');
        result.worker = { ...worker, observedPeakMiB: worker.peakWorkingSetBytes / 1048576 };
    }
}
function groupBy(items, key) {
    const groups = new Map();
    for (const item of items) { const k = key(item); if (!groups.has(k)) groups.set(k, []); groups.get(k).push(item); }
    return groups;
}
// Preserve and compare the pre-review corpus; never replace that baseline.
let comparison = null;
const baseline = join(root, 'corrections-baseline');
if (existsSync(join(baseline, 'evidence.json'))) {
    const previous = read(join(baseline, 'evidence.json'));
    const shapes = report => report.regions.map(r => JSON.stringify([r.layer, r.source.path, r.polygon.outer, r.polygon.holes, r.area_mm2])).sort();
    const codes = findings => Object.fromEntries([...new Set(findings.map(f => f.code))].sort().map(c => [c, findings.filter(f => f.code === c).length]));
    comparison = results.map(result => {
        const name = result.name, before = read(join(baseline, 'rust', name + '.json')), after = reports.get(name);
        const old = previous.results.find(r => r.name === name);
        assert.deepEqual(after.statistics.source_counts, before.statistics.source_counts, name + ': source inventory unchanged');
        assert.deepEqual(after.layers.map(l => l.union_area_mm2), before.layers.map(l => l.union_area_mm2), name + ': nominal layer areas unchanged');
        assert.deepEqual(shapes(after), shapes(before), name + ': nominal source polygons unchanged');
        const key = r => r.layer + ':' + r.source.path;
        const nominal = groupBy(before.regions, key), reliable = groupBy(after.reliable_regions, key), possible = groupBy(after.possible_regions, key);
        const changedSources = [...nominal].filter(([k, rs]) => rs.length !== (reliable.get(k)?.length ?? 0)).map(([k, rs]) => ({
            key: k, sourceId: rs[0].source.source_id, beforeRegions: rs.length, reliableRegions: reliable.get(k)?.length ?? 0,
            possibleRegions: possible.get(k)?.length ?? 0, nominalAreasMm2: rs.map(r => r.area_mm2).sort((a,b) => a-b),
        }));
        return { name, nominalShapesEqual: true, sourceCountsEqual: true,
            before: { status: before.status, regions: before.regions.length, islands: before.islands.length,
                reliableContacts: before.contacts.filter(c => c.reliable).length, uncertainContacts: before.contacts.filter(c => !c.reliable).length,
                findings: codes(before.findings), elapsedMs: old.elapsedMs, reportBytes: old.reportBytes },
            after: { status: after.status, regions: after.regions.length, ...result.connectivity,
                reliableRegions: after.reliable_regions.length, possibleRegions: after.possible_regions.length,
                elapsedMs: result.elapsedMs, reportBytes: result.reportBytes }, changedSources };
    });
    writeFileSync(join(root, 'comparison.json'), JSON.stringify(comparison, null, 2) + '\n');
}
const binary = createRequire(import.meta.url)('../native/node-addon/platform.cjs').nativeFilename();
const environment = { platform: process.platform, arch: process.arch, node: process.version, cpu: os.cpus()[0].model,
    logicalCores: os.availableParallelism(), nativeSha256: hash(join('native/node-addon', binary)) };
writeFileSync(join(root, 'evidence.json'), JSON.stringify({ environment, results, independentChecks: checks, resourceMeasurement, comparison }, null, 2) + '\n');
console.log(JSON.stringify({ fullRustNodeParity: results.length, independentBoards: checks.length, statuses: results.map(r => [r.name, r.status]), evidence: join(root, 'evidence.json') }, null, 2));
