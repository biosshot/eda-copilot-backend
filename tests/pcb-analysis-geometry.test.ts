import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import test from 'node:test';
import { runAnalysisProblem, terminateAnalysisPool } from '../src/analyze/run-analysis.ts';
import type { AnalysisProblem } from '../src/analyze/types.ts';
const native = createRequire(import.meta.url)('../native/node-addon/index.cjs') as typeof import('../native/node-addon/index.cjs');
assert.equal(typeof native.prepareAnalysisGeometry, 'function', 'Rebuild shared addon: npm run native:build');
const saved = readFileSync('tests/pcb-analyze/NanoScope/NanoScope.json', 'utf8');
function coupon() {
    const b = JSON.parse(saved);
    b.pads = [{ id: 'a', number: '1', net: b.nets[0].name, at: { x: 0, y: 0 }, rotationDeg: 0, layers: [b.layers[0].name], shape: { kind: 'rect', widthMm: 2, heightMm: 2 } }];
    b.outline = [{ x: -10, y: -10 }, { x: 10, y: -10 }, { x: 10, y: 10 }, { x: -10, y: 10 }]; b.cutouts = []; b.holes = [];
    b.copper = { fixed: { tracks: [], vias: [], zones: [] }, editable: { tracks: [], vias: [], zones: [] } };
    return JSON.stringify(b);
}
test('real geometry addon and process runtime keep typed operations and report parity', { timeout: 30_000 }, async () => {
    try {
        const boardJson = coupon(); const phases: string[] = [];
        const report = await runAnalysisProblem({ kind: 'prepare-geometry', boardJson, geometry: { max_deviation_mm: 0.0001 } }, { onProgress: e => { phases.push(e.phase); } });
        assert.deepEqual(report, native.prepareAnalysisGeometry(boardJson, { max_deviation_mm: 0.0001 }));
        assert.equal(report.status, 'complete'); assert.equal(report.layers[0].union_area_mm2, 4);
        assert.deepEqual(phases, ['queued', 'started', 'completed']);
        assert.equal((await runAnalysisProblem({ kind: 'import-board', boardJson })).imported, true);
        assert.equal((await runAnalysisProblem({ kind: 'prepare-geometry', boardJson: '{' })).status, 'invalid');
        assert.equal((await runAnalysisProblem({ kind: 'prepare-geometry', boardJson: '{}' })).status, 'invalid');
        for (const geometry of [null, [], { extra: 1 }, { max_deviation_mm: NaN }, { max_vertices_per_curve: 1.5 }, { max_generated_vertices: 1 }]) {
            await assert.rejects(runAnalysisProblem({ kind: 'prepare-geometry', boardJson, geometry } as AnalysisProblem), { code: 'INVALID_REQUEST' });
        }
        assert.throws(() => native.prepareAnalysisGeometry(boardJson, { extra: 1 } as never));
        const curved = JSON.parse(boardJson); curved.pads[0].shape = { kind: 'circle', diameterMm: 4 };
        const limited = await runAnalysisProblem({ kind: 'prepare-geometry', boardJson: JSON.stringify(curved), geometry: { max_generated_vertices: 8 } });
        assert.equal(limited.status, 'invalid'); assert.ok(limited.excluded.length > 0);
        curved.pads[0].shape.diameterMm = 1e300;
        assert.equal((await runAnalysisProblem({ kind: 'prepare-geometry', boardJson: JSON.stringify(curved) })).status, 'invalid');
    } finally { await terminateAnalysisPool(); }
});
test('active real geometry operation cancels and the shared runtime recovers', { timeout: 30_000 }, async () => {
    try {
        const controller = new AbortController(); const phases: string[] = [];
        await assert.rejects(runAnalysisProblem({ kind: 'prepare-geometry', boardJson: saved }, {
            signal: controller.signal, onProgress: e => { phases.push(e.phase); if (e.phase === 'started') controller.abort(); },
        }), { code: 'ABORTED' });
        assert.deepEqual(phases, ['queued', 'started', 'cancelled']);
        assert.equal((await runAnalysisProblem({ kind: 'prepare-geometry', boardJson: coupon() })).status, 'complete');
        assert.equal(native.numericContract(), 'f32-rte-ftz-v1'); assert.equal(native.placementScoreContractVersion(), 13);
    } finally { await terminateAnalysisPool(); }
});

// Same dimensions/angle as the independent Rust review counterexample.
function drillNeck(width = 1.999) {
    const b = JSON.parse(coupon()); b.nets = [{ name: 'A' }, { name: 'B' }]; b.rules.nets = []; b.rules.differentialPairs = [];
    const angle = -87.5, a = angle * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
    const pad = (id: string, x: number, y: number, w: number, h: number, net?: string) => ({ id, number: '1', net,
        at: { x, y }, rotationDeg: angle, layers: [b.layers[0].name], shape: { kind: 'rect', widthMm: w, heightMm: h } });
    b.pads = [pad('bridge', 0, 0, 6, width), pad('left', -2*c, -2*s, 1, 1, 'A'), pad('right', 2*c, 2*s, 1, 1, 'B')];
    b.holes = [{ id: 'cut', at: { x: 0, y: 0 }, hole: { shape: 'round', diameterMm: 2, plated: false } }];
    return JSON.stringify(b);
}
test('real addon and worker distinguish the nominal drill neck from reliable and possible copper', { timeout: 30_000 }, async () => {
    const ab = (islands: Array<{ nets: string[] }>) => islands.some(i => i.nets.includes('A') && i.nets.includes('B'));
    try {
        const boardJson = drillNeck();
        for (const deviation of [0.001, 0.0001]) {
            const geometry = { max_deviation_mm: deviation };
            const report = await runAnalysisProblem({ kind: 'prepare-geometry', boardJson, geometry });
            assert.deepEqual(report, native.prepareAnalysisGeometry(boardJson, geometry));
            assert.equal(ab(report.islands), false); assert.equal(report.statistics.shorts, 0);
            assert.equal(report.findings.some(f => f.code === 'physical_short'), false);
            if (deviation === 0.001) {
                assert.equal(report.status, 'partial'); assert.equal(ab(report.possible_islands), true);
                assert.ok(report.diagnostics.some(d => d.code === 'topology_uncertain' && d.source_id === 'bridge' && d.path === 'pads[0]'));
            } else assert.equal(ab(report.possible_islands), false);
            mkdirSync('debugging/copper-geometry/regression', { recursive: true });
            writeFileSync(`debugging/copper-geometry/regression/cut-${deviation}.json`, JSON.stringify(report, null, 2));
        }
        const narrowJson = drillNeck(2.001);
        const unresolved = native.prepareAnalysisGeometry(narrowJson);
        assert.equal(ab(unresolved.islands), false); assert.equal(ab(unresolved.possible_islands), true);
        const resolved = await runAnalysisProblem({ kind: 'prepare-geometry', boardJson: narrowJson, geometry: { max_deviation_mm: 0.00001 } });
        assert.deepEqual(resolved, native.prepareAnalysisGeometry(narrowJson, { max_deviation_mm: 0.00001 }));
        assert.equal(ab(resolved.islands), true); assert.equal(resolved.statistics.shorts, 1);
        writeFileSync('debugging/copper-geometry/regression/narrow-input.json', narrowJson);
        writeFileSync('debugging/copper-geometry/regression/narrow-fine.json', JSON.stringify(resolved, null, 2));
        writeFileSync('debugging/copper-geometry/regression/narrow-coarse.json', JSON.stringify(unresolved, null, 2));
        const limited = native.prepareAnalysisGeometry(narrowJson, { max_generated_vertices: 100 });
        assert.equal(limited.status, 'invalid'); assert.equal(ab(limited.islands), false);
        assert.ok(limited.diagnostics.some(d => d.code === 'envelope_budget'));
        writeFileSync('debugging/copper-geometry/regression/budget.json', JSON.stringify(limited, null, 2));
    } finally { await terminateAnalysisPool(); }
});
