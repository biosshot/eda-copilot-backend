import { test } from 'node:test';
import assert from 'node:assert/strict';
import ELK from 'elkjs';
import type { ElkNode } from 'elkjs';
import { layoutIndependentBlocks } from '../src/circuit-layout/independent-blocks.ts';
import { placeNearbyFlags } from '../src/circuit-layout/refinement/nearby-flags.ts';
import { shortSymbolsMap } from '../src/circuit-layout/short-symbol.ts';
import { type Placed, path, routeLength, pinPositions, overlaps } from '../src/circuit-layout/refinement/geometry.ts';
import { refineSchematicScene } from '../src/circuit-layout/refinement/index.ts';
import type { CircuitComponent } from '../src/types/circuit.ts';

function fixture(): ElkNode {
    return { id: 'root', children: [{ id: 'block___v_root__', children: ['a', 'b'].map(id => ({
        id: `block_${id}`, children: [0, 1].map(i => ({ id: `${id}${i}`, width: 50, height: 70,
            ports: [{ id: `${id}${i}_pin_1`, x: i ? 0 : 50, y: 35 }], layoutOptions: { 'elk.portConstraints': 'FIXED_POS' } }))
    })) }], edges: ['a', 'b'].map(id => ({ id: `edge_${id}`, sources: [`${id}0_pin_1`], targets: [`${id}1_pin_1`] })) };
}

test('independent block geometry is unaffected by sibling size or input order', async () => {
    const graph = fixture(), before = structuredClone(graph), elk = new ELK();
    const first = await layoutIndependentBlocks(graph, elk);
    assert.deepEqual(graph, before);
    graph.children![0].children!.reverse();
    graph.children![0].children![0].children![0].height = 500;
    const second = await layoutIndependentBlocks(graph, elk);
    const local = (g: ElkNode) => g.children![0].children!.find(n => n.id === 'block_a')!.children!
        .map(n => ({ id: n.id, x: n.x, y: n.y, width: n.width, height: n.height,
            ports: n.ports?.map(p => ({ id: p.id, x: p.x, y: p.y })) }));
    assert.deepEqual(local(first!), local(second!));
    assert.equal(first!.edges!.length, 2);
    assert(first!.edges!.every(e => e.sections?.length));
    const [a, b] = first!.children![0].children!;
    assert(!overlaps(a as Placed, b as Placed, 0));
});

test('a direct wire crossing block boundaries keeps the hierarchical layout path', async () => {
    const graph = fixture();
    graph.edges!.push({ id: 'bridge', sources: ['a0_pin_1'], targets: ['b0_pin_1'] });
    assert.equal(await layoutIndependentBlocks(graph, new ELK()), undefined);
});

test('independently refined blocks are packed as rigid non-overlapping drawings', () => {
    const components: CircuitComponent[] = ['A', 'B'].map((block, i) => ({ designator: `U${i}`, block_name: block,
        value: '', search_query: '', part_uuid: null, pins: [{ pin_number: 1, name: 'NC', signal_name: '' }] }));
    const children = components.map((c, i) => ({ id: c.designator, x: i * 400, y: 0, width: 100, height: 100,
        ports: [{ id: `${c.designator}_pin_1`, x: 100, y: 50 }] }));
    const scene = { id: 'scene', children, edges: [], width: 600, height: 100 };
    const result = refineSchematicScene(scene, components, [], []);
    const nodes = result.scene.children as Placed[];
    assert.equal(nodes.length, 2);
    assert(!overlaps(nodes[0], nodes[1], 0));
    assert.deepEqual(scene.children, children);
    assert(nodes.every(n => n.x >= 0 && n.y >= 0 && n.x + n.width <= result.scene.width! && n.y + n.height <= result.scene.height!));
});

test('private signal flags move together near side pins without changing port representation', () => {
    const ic: Placed = { id: 'U1', x: 100, y: 100, width: 100, height: 150,
        ports: [{ id: 'U1_pin_1', x: 100, y: 30 }, { id: 'U1_pin_2', x: 100, y: 100 }] };
    const flags = ['A', 'B'].map((net, i) => shortSymbolsMap.NETPORT.create(net, 'block', `f${i}`));
    const nodes: Placed[] = [ic, ...flags.map((f, i) => ({ ...f.node, x: 500 + i * 100, y: 20 }) as Placed)];
    const pins = pinPositions(nodes), nets = new Map<string, string>();
    const edges = flags.map((f, i) => {
        const a = `U1_pin_${i + 1}`, b = f.node.ports![0].id, p = pins.get(a)!, q = pins.get(b)!;
        nets.set(a, ['A', 'B'][i]); nets.set(b, ['A', 'B'][i]);
        return { id: `e${i}`, sources: [a], targets: [b], sections: [{ id: `s${i}`, startPoint: p,
            endPoint: q, bendPoints: [{ x: q.x, y: p.y }] }] };
    });
    const result = placeNearbyFlags(nodes, edges, flags.map(f => f.component), nets);
    assert(result.moved > 0);
    assert(result.edges.reduce((sum, e) => sum + routeLength(path(e)), 0)
        < edges.reduce((sum, e) => sum + routeLength(path(e)), 0));
    assert.deepEqual(result.nodes.map(n => n.id), nodes.map(n => n.id));
    const positions = pinPositions(result.nodes);
    for (const e of result.edges) {
        assert.deepEqual(path(e)[0], positions.get(e.sources[0]));
        assert.deepEqual(path(e).at(-1), positions.get(e.targets[0]));
    }
    for (let i = 0; i < result.nodes.length; i++) for (const b of result.nodes.slice(i + 1)) assert(!overlaps(result.nodes[i], b, 0));
});

test('moving a branch flag preserves the shared wire between real components', () => {
    const flag = shortSymbolsMap.NETPORT.create('DATA', 'block', 'flag');
    const nodes: Placed[] = [
        { id: 'U1', x: 100, y: 100, width: 100, height: 100, ports: [{ id: 'U1_pin_1', x: 100, y: 30 }] },
        { id: 'R1', x: 350, y: 100, width: 60, height: 60, ports: [{ id: 'R1_pin_1', x: 0, y: 30 }] },
        { ...flag.node, x: 500, y: 0 } as Placed,
    ];
    const pins = pinPositions(nodes), nets = new Map([...pins.keys()].map(id => [id, 'DATA']));
    const trunk = { id: 'trunk', sources: ['U1_pin_1'], targets: ['R1_pin_1'], sections: [{ id: 'trunk:s',
        startPoint: pins.get('U1_pin_1')!, endPoint: pins.get('R1_pin_1')! }] };
    const target = pins.get('flag_pin_1')!;
    const branch = { id: 'branch', sources: ['U1_pin_1'], targets: ['flag_pin_1'], sections: [{ id: 'branch:s',
        startPoint: pins.get('U1_pin_1')!, endPoint: target,
        bendPoints: [{ x: 220, y: 130 }, { x: 220, y: target.y }] }] };
    const result = placeNearbyFlags(nodes, [trunk, branch], [flag.component], nets);
    assert(result.moved > 0);
    assert.deepEqual(result.edges.find(e => e.id === 'trunk'), trunk);
    assert(routeLength(path(result.edges.find(e => e.id === 'branch')!)) < routeLength(path(branch)));
});
