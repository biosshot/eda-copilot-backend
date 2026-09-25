import test from 'node:test';
import assert from 'node:assert/strict';
import type { ElkExtendedEdge } from 'elkjs';
import { RouteEnvironment, clearPath } from '../src/circuit-layout/refinement/router.ts';
import type { Point } from '../src/circuit-layout/refinement/geometry.ts';

const nets = new Map([['signal-a', 'SIGNAL'], ['signal-b', 'SIGNAL']]);
const obstacle: ElkExtendedEdge = {
    id: 'signal', sources: ['signal-a'], targets: ['signal-b'],
    sections: [{ id: 'section', startPoint: { x: 0, y: 0 }, endPoint: { x: 100, y: 0 } }],
};

test('foreign wire clearance survives rigid movement, including near endpoints', () => {
    for (const vertical of [false, true]) for (const dynamic of [false, true]) for (const newGeometry of [false, true]) {
        const turn = (p: Point) => vertical ? { x: p.y, y: p.x } : p;
        const wire = { ...obstacle, sections: obstacle.sections!.map(s => ({ ...s,
            startPoint: turn(s.startPoint), endPoint: turn(s.endPoint) })) };
        const env = new RouteEnvironment([], dynamic ? [] : [wire], nets);
        const allows = (points: Point[], net = 'GND') =>
            clearPath(points.map(turn), net, env, [], dynamic ? [wire] : [], new Set(), newGeometry);
        for (const distance of [1, 5, 9, 10, 15]) {
            const expected = distance >= 10;
            assert.equal(allows([{ x: 20, y: distance }, { x: 80, y: distance }]), expected, 'parallel');
            assert.equal(allows([{ x: 50, y: distance }, { x: 50, y: 40 }]), expected, 'perpendicular near miss');
            assert.equal(allows([{ x: 100 + distance, y: 0 }, { x: 140, y: 0 }]), expected, 'collinear near miss');
        }
        assert.equal(allows([{ x: 50, y: -20 }, { x: 50, y: 20 }]), true, 'proper crossing');
        assert.equal(allows([{ x: 50, y: 0 }, { x: 50, y: 20 }]), false, 'foreign T junction');
        assert.equal(allows([{ x: 20, y: 0 }, { x: 80, y: 0 }]), false, 'foreign overlap');
        assert.equal(allows([{ x: 20, y: 1 }, { x: 80, y: 1 }], 'SIGNAL'), true, 'same-net branch');
    }
});
