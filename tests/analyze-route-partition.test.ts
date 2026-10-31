import test from 'node:test';
import assert from 'node:assert/strict';
import { area, clipHalfPlane, buildRouteGraph, type Point } from '../src/analyze/route-partition.ts';
const domain: Point[] = [[-2, -2], [12, -2], [12, 12], [-2, 12]];
for (const angle of [0, 45, 90])
    test(`route domains conserve physical area at ${angle} degrees`, () => {
        const theta = angle * Math.PI / 180, p = [10 * Math.cos(theta), 10 * Math.sin(theta)];
        const board = { copper: { fixed: { tracks: [{ layer: 'TOP', widthMm: .1, points: [{ x: 0, y: 0 }, { x: p[0], y: p[1] }] }] } } };
        const g: any = { regions: [{ id: 'r', source: { path: 'copper.fixed.tracks[0]' }, layer: 'TOP' }], contacts: [] };
        const graph = buildRouteGraph(board, g, new Set(['r']), .3), cells = [{ polygonMm: clipHalfPlane(domain, graph.runs[0].direction, 2) }, { polygonMm: clipHalfPlane(domain, graph.runs[0].direction, 2, false) }];
        assert.equal(graph.runs.length, 1);
        assert.ok(cells.length > 1);
        assert.ok(Math.abs(cells.reduce((s, c) => s + area(c.polygonMm), 0) - area(domain)) < 1e-9);
        for (let i = 0; i < cells.length; i++)
            for (let j = 0; j < i; j++) {
                let overlap = cells[i].polygonMm;
                for (let k = 0; k < cells[j].polygonMm.length; k++) {
                    const a = cells[j].polygonMm[k], b = cells[j].polygonMm[(k + 1) % cells[j].polygonMm.length], n: Point = [b[1] - a[1], a[0] - b[0]];
                    overlap = clipHalfPlane(overlap, n, a[0] * n[0] + a[1] * n[1]);
                }
                assert.ok(area(overlap) < 1e-9, 'disjoint physical domains');
            }
    });
test('near crossing and remote-order meander conductors retain coupling witnesses', () => {
    const tracks = [[[0, 0], [10, 0]], [[5, -1], [5, 1]], [[10, .2], [0, .2]]].map(points => ({ layer: 'TOP', widthMm: .1, points: points.map(([x, y]) => ({ x, y })) }));
    const g: any = { regions: tracks.map((_, i) => ({ id: String(i), source: { path: `copper.fixed.tracks[${i}]` }, layer: 'TOP' })), contacts: [{ a: '0', b: '1', reliable: true, kind: 'area' }] };
    const graph = buildRouteGraph({ copper: { fixed: { tracks } } }, g, new Set(['0', '1', '2']), .3);
    assert.equal(graph.runs.length, 3);
    assert.equal(graph.couplingPairs.length, 3);
    assert.equal(graph.contacts.length, 1);
});
