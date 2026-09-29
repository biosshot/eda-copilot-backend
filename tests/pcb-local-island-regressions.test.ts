import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import type { PlacementInput, PlacementTreeNode } from '../src/types/pcb/layout-model.ts';
import { buildPlacementGraph } from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import { solvePlacementIslands } from '../src/pcb-layout/pcb-auto-place-v2/island-solver.ts';
import { canSolvePassiveNetIsland } from '../src/pcb-layout/pcb-auto-place-v2/passive-net-island.ts';
import { renderPlacementSubsetSvg } from '../src/pcb-layout/pcb-auto-place/render.ts';
import { componentPadBox, getPadWorld } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { segmentIntersectsBox } from '../src/pcb-layout/pcb-auto-place/utils.ts';
import { solvePlacementTreeBottomUp } from '../src/pcb-layout/pcb-auto-place-v2/tree-solver.ts';
import { withBlockSolverCapture } from '../src/pcb-layout/pcb-auto-place-v2/block-solver-engine.ts';

const fixture = (): PlacementInput => JSON.parse(readFileSync(new URL('./fixtures/block-placement/Telemetry/input.json', import.meta.url), 'utf8'));

test('both Telemetry switching links avoid the other inductor pad before the pair becomes rigid', () => {
    const input = fixture(), graph = buildPlacementGraph(input), results = solvePlacementIslands(input, graph);
    for (const [name, refs] of [
        ['lte_switch', [['U10','11','L1','1'], ['U10','9','L1','2']]],
        ['logic_switch', [['U12','4','L2','1'], ['U12','2','L2','2']]],
    ] as const) {
        const result = results.find(r => r.label === `core_pairs:${name}`)!;
        const lengths: number[] = [];
        for (const [a, ap, b, bp] of refs) {
            const first = input.components.find(c => c.designator === a)!, second = input.components.find(c => c.designator === b)!;
            const pa = result.placements.find(p => p.designator === a)!, pb = result.placements.find(p => p.designator === b)!;
            const from = getPadWorld(first, pa, ap)!, to = getPadWorld(second, pb, bp)!;
            lengths.push(Math.hypot(from.x - to.x, from.y - to.y));
            const net = first.pins.find(p => String(p.pin_number) === ap)!.signal_name;
            for (const component of [first, second]) for (const pad of component.footprint.pads) {
                if (component.pins.some(pin => String(pin.pin_number) === String(pad.pin_number) && pin.signal_name === net)) continue;
                const placement = result.placements.find(p => p.designator === component.designator)!;
                assert.equal(segmentIntersectsBox(from, to, componentPadBox(placement, pad)), false, `${name}: ${a}.${ap}->${b}.${bp} crosses ${component.designator}.${pad.pin_number}`);
            }
        }
        assert.ok(Math.max(...lengths) <= 5, `${name}: explicit maximum distance`);
        assert.ok(Math.max(...lengths) - Math.min(...lengths) < .1, `${name}: both links must be balanced`);
    }
});

test('an unrelated diode is not frozen with a capacitor bank', () => {
    const input = fixture(), select = (names: string[]) => input.components.filter(c => names.includes(c.designator));
    assert.equal(canSolvePassiveNetIsland(select(['C56', 'C57'])), true);
    assert.equal(canSolvePassiveNetIsland(select(['C56', 'C57', 'D2'])), false);
});

test('R6.2 is present and explicitly ignored; diagnostic rendering restores its ratsnest', () => {
    const input = fixture(), graph = buildPlacementGraph(input);
    const diagnostics = graph.report.diagnostics.filter(d => d.code === 'ignored_local_connections');
    assert.ok(diagnostics.some(d => d.message.includes('I_OUT') && d.message.includes('R6.2')));
    const all = JSON.parse(readFileSync(new URL('../docs/experimental/pcb/global-placement-2026-09-27/Telemetry/pads-placement.json', import.meta.url), 'utf8')).placements;
    const names = new Set(input.blocks.find(b => b.name === 'current_iso')!.component_designators);
    const placements = all.filter((p: {designator: string}) => names.has(p.designator));
    const source = JSON.stringify(input);
    const options = { signalPaths: false, ratsnestTopology: 'mst' as const };
    assert.ok(!renderPlacementSubsetSvg(input, placements, options).includes('<title>I_OUT</title>'));
    assert.ok(renderPlacementSubsetSvg(input, placements, { ...options, includeIgnoredSignals: true }).includes('<title>I_OUT</title>'));
    assert.equal(JSON.stringify(input), source);
});

test('connected schematic pins without footprint pads produce an explicit diagnostic', () => {
    const input = fixture(); input.components[0].pins.push({pin_number:'missing', name:'missing', signal_name:'VISIBLE_NET'});
    assert.ok(buildPlacementGraph(input).report.diagnostics.some(d => d.code === 'connected_pin_without_pad' && d.message.includes('missing (VISIBLE_NET)')));
});

test('a parent block with satellite children receives net candidates and beam search', () => {
    const input = fixture(), graph = buildPlacementGraph(input);
    const find = (n: PlacementTreeNode): PlacementTreeNode | undefined => n.kind === 'block' && n.label === 'current_iso' ? n : n.children.map(find).find(Boolean);
    const node = find(graph.root)!;
    const stop = new Error('captured parent solve');
    let reached = false;
    assert.throws(() => withBlockSolverCapture(params => {
        if (params.node.label !== 'current_iso') return;
        reached = true;
        assert.ok(params.primitives.some(p => p.kind === 'block'));
        assert.ok(params.primitives.some(p => p.kind === 'component'));
        assert.equal(params.options.searchWidth, 4);
        assert.equal(params.options.experiments?.netCandidates, true);
        assert.equal(params.options.experiments?.padCrossings, true);
        throw stop;
    }, () => solvePlacementTreeBottomUp(input, { ...graph, root: node })), e => e === stop);
    assert.ok(reached);
});
