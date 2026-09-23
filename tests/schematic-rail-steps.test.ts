import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { coalesceNetRoutes, connectedNetEdges, straightRuns, routeSegments } from '../src/circuit-layout/refinement/net-routes.ts';
import type { ElkExtendedEdge } from 'elkjs';

// Captured backend output for the oscilloscope FPGA power section, including
// nearby foreign nets and component obstacles. No editor coordinates involved.
const fixture = JSON.parse(readFileSync(new URL('./fixtures/fpga-power-rails.json', import.meta.url), 'utf8')) as {
    edges: ElkExtendedEdge[]; nets: [string, string][];
    boxes: { x: number; y: number; width: number; height: number }[]; pins: { x: number; y: number }[];
};
const nets = new Map(fixture.nets);
const routes = (edges: ElkExtendedEdge[], net: string) => straightRuns(edges.filter(e => nets.get(e.sources[0]) === net)
    .flatMap(e => (e.sections ?? []).flatMap(s => routeSegments([s.startPoint, ...s.bendPoints ?? [], s.endPoint]))));

test('FPGA power rails lose their 5-unit steps without changing terminals or islands', () => {
    const result = coalesceNetRoutes(fixture.edges, nets, fixture.boxes, fixture.pins, { maxBridgeDistance: 25 });
    for (const net of ['FPGA_1V1', 'SYS_3V3']) {
        assert.ok(routes(result.edges, net).length < routes(fixture.edges, net).length, `${net} must become simpler`);
    }
    for (const edge of result.edges) {
        const before = fixture.edges.find(e => e.id === edge.id)!;
        assert.deepEqual(edge.sources, before.sources);
        assert.deepEqual(edge.targets, before.targets);
        assert.deepEqual(edge.sections![0].startPoint, before.sections![0].startPoint);
        assert.deepEqual(edge.sections![0].endPoint, before.sections![0].endPoint);
        for (const s of routeSegments([edge.sections![0].startPoint, ...edge.sections![0].bendPoints ?? [], edge.sections![0].endPoint])) {
            assert.ok(s.a.x === s.b.x || s.a.y === s.b.y);
        }
    }
    assert.equal(connectedNetEdges(result.edges, nets).length, connectedNetEdges(fixture.edges, nets).length);
});

test('a foreign wire blocks extending the FPGA rail into its space', () => {
    const foreign: ElkExtendedEdge = { id: 'foreign', sources: ['X1'], targets: ['X2'], sections: [{
        id: 'foreign_s', startPoint: { x: 779.5, y: 190 }, endPoint: { x: 779.5, y: 240 },
    }] };
    const result = coalesceNetRoutes([...fixture.edges, foreign], new Map([...nets, ['X1', 'OTHER'], ['X2', 'OTHER']]),
        fixture.boxes, fixture.pins, { maxBridgeDistance: 25 });
    assert.deepEqual(result.edges.find(e => e.id === 'foreign'), foreign);
    assert.ok(!routes(result.edges, 'FPGA_1V1').some(s => s.a.x === 779.5 && s.b.x === 779.5
        && Math.min(s.a.y, s.b.y) < 240 && Math.max(s.a.y, s.b.y) > 190));
});
