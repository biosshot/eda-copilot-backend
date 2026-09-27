import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import type { PlacementInput, PlacementTreeNode } from '../src/types/pcb/layout-model.ts';
import type { BlockSolveParams } from '../src/pcb-layout/pcb-auto-place-v2/block-solver.ts';
import { buildPlacementGraph } from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import { componentBox, getPadWorld } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { solvePlacementSubtreeSync } from '../src/pcb-layout/pcb-auto-place-v2/tree-solver.ts';
import { withBlockSolverCapture } from '../src/pcb-layout/pcb-auto-place-v2/block-solver-engine.ts';
import { relaxBlockGroups } from '../src/pcb-layout/pcb-auto-place-v2/relaxed-block-groups.ts';

function fixture(block: string) {
    const input: PlacementInput = JSON.parse(readFileSync(new URL('./fixtures/block-placement/Telemetry/input.json', import.meta.url), 'utf8'));
    const graph = buildPlacementGraph(input), nodes: PlacementTreeNode[] = [];
    const walk = (n: PlacementTreeNode) => { nodes.push(n); n.children.forEach(walk); }; walk(graph.root);
    const node = nodes.find(n => n.kind === 'block' && n.label === block)!;
    let params!: BlockSolveParams; const stop = new Error('capture');
    assert.throws(() => withBlockSolverCapture(p => { if (p.node.id === node.id) { params = p; throw stop; } },
        () => solvePlacementSubtreeSync({ input, graph, node })), e => e === stop);
    const make = (designator: string) => {
        const c = input.components.find(c => c.designator === designator)!;
        const p = { designator, x: 0, y: 0, rotate: 0, layer: 'top' as const, score: 0 };
        const bbox = componentBox(c, p);
        return { id: `primitive:tree:component:${designator}`, sourceNodeId: `tree:component:${designator}`,
            label: designator, kind: 'component' as const, bbox, width: bbox.right-bbox.left, height: bbox.bottom-bbox.top,
            placements: [p], children: [], connectionPoints: c.pins.flatMap(pin => {
                const q = getPadWorld(c, p, pin.pin_number); return q ? [{...q, ref:`${designator}.${pin.pin_number}`, net:pin.signal_name}] : [];
            }) };
    };
    return { input, nodes, params, make };
}

test('cap relaxation preserves core pairs, electrical targets and component inventory', () => {
    const {input,nodes,params,make} = fixture('lte_power'), before=JSON.stringify({input,params});
    const r=relaxBlockGroups(input,nodes,params.primitives,params.relations,'caps',make);
    assert.ok(r.released.includes('cap_cluster:50'));
    assert.ok(r.primitives.some(p=>p.label==='C34'));
    assert.ok(r.primitives.some(p=>p.label==='C35'));
    assert.equal(r.primitives.find(p=>p.placements.some(q=>q.designator==='L1')),params.primitives.find(p=>p.placements.some(q=>q.designator==='L1')));
    assert.deepEqual(r.primitives.flatMap(p=>p.placements.map(q=>q.designator)).sort(),params.primitives.flatMap(p=>p.placements.map(q=>q.designator)).sort());
    for(const name of ['C34','C35']) assert.ok(r.relations.some(x=>x.kind==='island_target'&&x.from===`component:${name}`&&x.to==='pad:U10.12'));
    assert.equal(JSON.stringify({input,params}),before);
});

test('satellite relaxation releases passive banks and carries their parent anchors', () => {
    const {input,nodes,params,make} = fixture('current_iso');
    const r=relaxBlockGroups(input,nodes,params.primitives,params.relations,'satellites',make);
    assert.ok(r.released.includes('current_hdc'));
    assert.equal(r.primitives.some(p=>p.kind==='block'),false);
    for(const name of ['C13','C14']) {
        assert.ok(r.primitives.some(p=>p.kind==='component'&&p.label===name));
        assert.ok(r.relations.some(x=>x.from===`component:${name}`&&x.to==='pad:U2.1'));
    }
    const pairs=params.relations.filter(r=>r.kind==='critical_pair');
    for(const pair of pairs) assert.ok(r.relations.includes(pair));
    assert.equal(new Set(r.primitives.flatMap(p=>p.placements.map(q=>q.designator))).size,12);
});

test('explicit row topology and fixed components prevent cap relaxation', () => {
    const {input,nodes,params,make} = fixture('lte_power');
    const cap=nodes.find(n=>n.label==='cap_cluster:50')!;
    cap.data!.maxRows=1;
    assert.equal(relaxBlockGroups(input,nodes,params.primitives,params.relations,'all',make).released.length,0);
    cap.data!.maxRows=null;
    input.components.find(c=>c.designator==='C34')!.pcb.fixedPlacement={x:0,y:0};
    assert.equal(relaxBlockGroups(input,nodes,params.primitives,params.relations,'all',make).released.length,0);
});

test('Telemetry C33 stays next to U10 despite its strong external LTE connection', () => {
    const {input,nodes} = fixture('lte_power');
    const graph=buildPlacementGraph(input),node=nodes.find(n=>n.kind==='block'&&n.label==='lte_power')!;
    const result=solvePlacementSubtreeSync({input,graph,node});
    const point=(ref:string,pin:string)=>getPadWorld(input.components.find(c=>c.designator===ref)!,result.root.placements.find(p=>p.designator===ref)!,pin)!;
    const a=point('C33','1'),b=point('U10','3');
    assert.ok(Math.hypot(a.x-b.x,a.y-b.y)<3,'external block envelope must not push C33 away from its local IC pin');
    for(const [pin,inductorPin] of [['11','1'],['9','2']]){
        const x=point('U10',pin),y=point('L1',inductorPin);
        assert.ok(Math.hypot(x.x-y.x,x.y-y.y)<=5);
    }
});
