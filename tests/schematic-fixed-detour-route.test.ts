import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { ElkExtendedEdge } from 'elkjs';
import { type Placed, edgeSegments, path, routeLength, segmentThroughBox } from '../src/circuit-layout/refinement/geometry.ts';
import { rerouteFixedDetours } from '../src/circuit-layout/refinement/detour-route.ts';

test('a long wire around fixed ICs moves closer without changing terminal identity or moving bodies', () => {
    const nodes: Placed[] = [
        { id: 'U1', x: 360, y: 100, width: 100, height: 100, ports: [{ id: 'U1_pin_1', x: 100, y: 30 }] },
        { id: 'U2', x: 40, y: 260, width: 100, height: 100, ports: [{ id: 'U2_pin_1', x: 0, y: 30 }] },
    ];
    const edge: ElkExtendedEdge = { id: 'signal', sources: ['U1_pin_1'], targets: ['U2_pin_1'], sections: [{ id: 'signal_s0',
        startPoint: { x: 460, y: 130 }, bendPoints: [{ x: 500, y: 130 }, { x: 500, y: 520 },
            { x: -30, y: 520 }, { x: -30, y: 290 }], endPoint: { x: 40, y: 290 } }] };
    const before = structuredClone(nodes), oldLength = routeLength(path(edge));
    const result = rerouteFixedDetours(nodes, [edge], new Map([['U1_pin_1', 'SIG'], ['U2_pin_1', 'SIG']]));
    assert.equal(result.changed, 1);
    assert.deepEqual(nodes, before);
    assert.deepEqual(result.edges[0].sources, edge.sources);
    assert.deepEqual(result.edges[0].targets, edge.targets);
    assert(routeLength(path(result.edges[0])) < oldLength - 200);
    assert(path(result.edges[0]).every(p => p.y < 520), 'the distant bottom corridor is no longer used');
    assert(edgeSegments(result.edges[0]).every(segment => nodes.every(node => !segmentThroughBox(segment, node))));
});
