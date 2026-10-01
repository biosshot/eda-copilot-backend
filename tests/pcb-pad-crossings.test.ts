import test from 'node:test';
import assert from 'node:assert/strict';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import type { NativePostPlaceScoreProblemV1, NativeRoutingObstacle } from '../src/pcb-layout/pcb-auto-place-v2/native/contract.ts';
import { minimumSpanningEdges } from '../src/pcb-layout/pcb-auto-place/ratsnest.ts';
import { f32ScoreTolerance } from './helpers/f32.ts';

const addon = loadNativeBoardPacker();
const pad = (ref: string, x: number, net = 'OTHER', layer: 'top' | 'bottom' | undefined = 'top'): NativeRoutingObstacle => ({
    ref, primitiveId: 'U1', net, layer, box: { left: x - .2, right: x + .2, top: -.2, bottom: .2 },
});
const problem = (pads: NativeRoutingObstacle[]): NativePostPlaceScoreProblemV1 => ({
    version: 1, padCrossingWeight: 180, routingObstacles: pads,
    nets: [{ name: 'SIG', weight: 1, points: [{ x: 0, y: 0 }, { x: 10, y: 0 }], layers: ['top', 'top'] }],
    distances: [], clearances: [], fixedPenalties: [], edges: [], paths: [],
});
const delta = (pads: NativeRoutingObstacle[]) => addon.scorePostPlace(problem(pads)) - addon.scorePostPlace(problem([]));

test('each distinct foreign pad adds its own penalty, including other pins of the endpoint IC', () => {
    assert.equal(delta([pad('U1.2', 2)]), 180);
    assert.equal(delta([pad('U1.2', 2), pad('U1.3', 4)]), 360);
    assert.equal(delta([pad('U1.2', 2), pad('U1.3', 4), pad('U1.4', 6)]), 540);
});

test('same-net pads and opposite-side SMD pads are free, through-hole obstacles are not', () => {
    assert.equal(delta([pad('U1.1', 0, 'SIG'), pad('R1.1', 10, 'SIG'), pad('R2.1', 5, 'SIG')]), 0);
    assert.equal(delta([pad('U1.2', 4, 'OTHER', 'bottom')]), 0);
    const through = pad('J1.1', 5); delete through.layer;
    assert.equal(delta([through]), 180);
});

test('duplicate obstacle records of one pad do not charge twice; disconnected pads still obstruct', () => {
    assert.equal(delta([pad('U1.2', 2), pad('U1.2', 2)]), 180);
    const unconnected = pad('U1.3', 4); delete unconnected.net;
    assert.equal(delta([unconnected]), 180);
});

test('moving a pad out of a straight connection removes its penalty and preserves legacy score when disabled', () => {
    const p = pad('U1.2', 2); p.box.top = 1; p.box.bottom = 2;
    assert.equal(delta([p]), 0);
    const old = problem([pad('U1.2', 2)]); old.padCrossingWeight = 0;
    assert.equal(addon.scorePostPlace(old), addon.scorePostPlace(problem([])));
});

test('comparison ratsnest uses the same spanning-tree length as the native objective', () => {
    const p = problem([]);
    p.nets[0].points = [{ x: 0, y: 0 }, { x: 10, y: 0 }, { x: 0, y: 1 }, { x: 10, y: 1 }];
    p.nets[0].layers = ['top', 'top', 'top', 'top'];
    const edges = minimumSpanningEdges(p.nets[0].points);
    assert.equal(edges.length, 3);
    const score = edges.reduce((sum, [a, b]) => {
        const points = p.nets[0].points, d = Math.hypot(points[a].x - points[b].x, points[a].y - points[b].y);
        return sum + d * 10 + d * d * .35;
    }, 0);
    const actual=addon.scorePostPlace(p);
    assert.ok(Math.abs(actual-score)<=f32ScoreTolerance(actual,score));
});
