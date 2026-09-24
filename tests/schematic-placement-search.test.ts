import test from 'node:test';
import assert from 'node:assert/strict';
import type { ElkExtendedEdge } from 'elkjs';
import { effectiveLayoutArea } from '../src/circuit-layout/quality.ts';
import { acceptsFlagOrientation } from '../src/circuit-layout/refinement/flag-policy.ts';
import { turnNode } from '../src/circuit-layout/refinement/groups.ts';
import { shortSymbolsMap } from '../src/circuit-layout/short-symbol.ts';
import { removeNetCycles } from '../src/circuit-layout/refinement/net-cycles.ts';
import { connectedNetEdges, straightRuns } from '../src/circuit-layout/refinement/net-routes.ts';
import { type Placed, path, edgeSegments, routeLength } from '../src/circuit-layout/refinement/geometry.ts';
import { labelLongLinks } from '../src/circuit-layout/refinement/long-links.ts';
import { collapsePortRows } from '../src/circuit-layout/port-rows.ts';
import ELK, { type ElkNode } from 'elkjs';
import { pinPositions } from '../src/circuit-layout/refinement/geometry.ts';
import { RouteEnvironment, reconnect } from '../src/circuit-layout/refinement/router.ts';

const edge = (id: string, from: string, to: string, points: number[][]): ElkExtendedEdge => ({ id, sources: [from], targets: [to],
    sections: [{ id: `${id}:s`, startPoint: { x: points[0][0], y: points[0][1] },
        endPoint: { x: points.at(-1)![0], y: points.at(-1)![1] }, bendPoints: points.slice(1, -1).map(([x, y]) => ({ x, y })) }] });

test('area preference has a landscape plateau, mild width cost and bounded height cost', () => {
    assert.equal(effectiveLayoutArea(200, 100), 20000);
    assert.equal(effectiveLayoutArea(100, 100), 10000);
    assert(effectiveLayoutArea(100, 200) > effectiveLayoutArea(200, 100) * 2);
    assert(effectiveLayoutArea(400, 100) < effectiveLayoutArea(100, 400));
    assert.equal(effectiveLayoutArea(10, 1000), 100000);
});

test('a direct ground may turn with its owner without shortening its own lead', () => {
    const flag = shortSymbolsMap.GND.create('GND', 'block', 'ground');
    const before = { ...flag.node, x: 0, y: 0 } as Placed, after = turnNode(before, 180);
    const lead = edge('lead', 'C_pin_1', 'ground_pin_1', [[0, 0], [0, 40]]);
    assert(acceptsFlagOrientation(before, after, flag.component, [lead], [lead], true));
    assert(!acceptsFlagOrientation(before, after, flag.component, [lead], [lead], false));
});

test('moving both ends of a component-to-ground lead still produces a route', () => {
    const flag = shortSymbolsMap.GND.create('GND', 'block', 'ground');
    const nodes: Placed[] = [{ id: 'C', x: 100, y: 100, width: 60, height: 40,
        ports: [{ id: 'C_pin_1', x: 0, y: 20 }, { id: 'C_pin_2', x: 60, y: 20 }] },
        { ...flag.node, x: 70 - flag.node.ports![0].x!, y: 135 } as Placed];
    const lead = edge('lead', 'ground_pin_1', 'C_pin_1', [[70, 135], [70, 120], [100, 120]]);
    const nets = new Map([['ground_pin_1', 'GND'], ['C_pin_1', 'GND'], ['C_pin_2', 'SIGNAL']]);
    const route = reconnect(lead, nodes, new RouteEnvironment([], [], nets), []);
    assert(route);
    assert.deepEqual(path(route), path(lead));
});

test('an existing short IC escape permits a nearby ground in a dense pin corridor', () => {
    const flag = shortSymbolsMap.GND.create('GND', 'block', 'ground');
    const fixed: Placed[] = [{ id: 'U', x: 0, y: 100, width: 100, height: 100, ports: [{ id: 'U_pin', x: 100, y: 20 }] },
        { id: 'L', x: 120, y: 100, width: 60, height: 28, ports: [] }];
    const ground = { ...flag.node, x: 150 - flag.node.ports![0].x!, y: 160 } as Placed;
    const lead = edge('lead', 'U_pin', 'ground_pin_1', [[100, 120], [110, 120], [110, 145], [150, 145], [150, 300]]);
    const nets = new Map([['U_pin', 'GND'], ['ground_pin_1', 'GND']]);
    const route = reconnect(lead, [ground], new RouteEnvironment(fixed, [], nets), []);
    assert(route);
    assert(routeLength(path(route)) < routeLength(path(lead)) / 2);
    assert.deepEqual(path(route).at(-1), { x: 150, y: 160 });
});

test('the clock supply loop loses redundant ink while preserving all terminal connections', () => {
    // Narrow cycle from the captured CLK_2V5 fan-out, translated to the origin.
    const edges = [edge('resistor', 'R', 'U', [[-35, -70], [-25, -70], [-25, -30], [0, -30], [0, 0], [-10, 0]]),
        edge('flag', 'F', 'U', [[0, -40], [0, -25], [5, -25], [5, 0], [-10, 0]]),
        edge('cap', 'C', 'U', [[22, 60], [22, 0], [-10, 0]])];
    const nets = new Map(['R', 'U', 'F', 'C'].map(id => [id, 'CLOCK_SUPPLY']));
    const result = removeNetCycles(edges, nets);
    assert.equal(result.removed, 1);
    assert.equal(connectedNetEdges(result.edges, nets).length, 1);
    const ink = (es: ElkExtendedEdge[]) => straightRuns(es.flatMap(edgeSegments)).reduce((n, s) => n + routeLength([s.a, s.b]), 0);
    assert(ink(result.edges) < ink(edges));
    result.edges.forEach((e, i) => {
        assert.deepEqual(e.sources, edges[i].sources); assert.deepEqual(e.targets, edges[i].targets);
        assert.deepEqual(path(e)[0], path(edges[i])[0]); assert.deepEqual(path(e).at(-1), path(edges[i]).at(-1));
    });
    assert.equal(removeNetCycles(result.edges, nets).removed, 0);
    const shifted = edges.map(e => ({ ...e, sections: e.sections!.map(s => ({ ...s,
        startPoint: { x: s.startPoint.x + 123, y: s.startPoint.y + 456 },
        endPoint: { x: s.endPoint.x + 123, y: s.endPoint.y + 456 },
        bendPoints: s.bendPoints!.map(p => ({ x: p.x + 123, y: p.y + 456 })) })) }));
    const moved = removeNetCycles(shifted, nets);
    assert.equal(moved.removed, 1);
    moved.edges.forEach((e, i) => assert.deepEqual(path(e).map(p => ({ x: p.x - 123, y: p.y - 456 })), path(result.edges[i])));
});

test('port rows survive ELK expansion with original symbol and terminal identities', async () => {
    const flags = Array.from({ length: 6 }, (_, i) => shortSymbolsMap.NETPORT.create(`IO_${i}`, 'block', `flag${i}`));
    const ic: ElkNode = { id: 'IC', width: 100, height: 160, ports: flags.map((_, i) => ({ id: `IC_${i}`, x: 100, y: 30 + i * 20 })),
        layoutOptions: { 'elk.portConstraints': 'FIXED_POS' } };
    const block: ElkNode = { id: 'block', children: [ic, ...flags.map(f => f.node)], layoutOptions: { 'elk.algorithm': 'layered' } };
    const edges: ElkExtendedEdge[] = flags.map((f, i) => ({ id: `wire${i}`, sources: [`IC_${i}`], targets: [f.node.ports![0].id] }));
    const expand = collapsePortRows(block, edges, new Map(flags.map(f => [f.component.designator, f.component])));
    assert.equal(block.children!.length, 2);
    const result = await new ELK().layout({ id: 'root', children: [block], edges });
    expand(result.children![0]);
    const nodes = result.children![0].children!;
    assert.equal(nodes.length, 7);
    const row = nodes.filter(n => n.id !== 'IC');
    assert.equal(new Set(row.map(n => n.y)).size, 1);
    assert.equal(new Set(row.map(n => n.x)).size, 6);
    const positions = pinPositions(nodes as Placed[]);
    for (const e of result.edges!) {
        assert.deepEqual(path(e)[0], positions.get(e.sources[0]));
        assert.deepEqual(path(e).at(-1), positions.get(e.targets[0]));
    }
});

test('long inter-IC connections are eligible even in a small block', () => {
    const nodes: Placed[] = [0, 1].map(i => ({ id: `U${i}`, x: i * 1000, y: 100, width: 100, height: 100,
        ports: [0, 1, 2].map(p => ({ id: `U${i}_pin_${p}`, x: i ? 0 : 100, y: 20 + p * 20 })) }));
    const nets = new Map(nodes.flatMap(n => n.ports!.map((p, i) => [p.id, i ? p.id : 'CONTROL'] as const)));
    const edges = [edge('long', 'U0_pin_0', 'U1_pin_0', [[100, 120], [1000, 120]])];
    const result = labelLongLinks(nodes, edges, nets, new Map(nodes.map(n => [n.id, 'block'])), new Set(nodes.map(n => n.id)));
    assert.equal(result.links, 1);
    assert.equal(result.added.length, 2);
    assert(result.added.every(c => c.pins[0].signal_name === 'CONTROL'));
    assert(!result.edges.some(e => e.id === 'long'));
});
