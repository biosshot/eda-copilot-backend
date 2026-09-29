import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import type { PlacementInput, PlacementTreeNode } from '../src/types/pcb/layout-model.ts';
import { preferredPlacementLayers } from '../src/pcb-layout/pcb-auto-place/fixed.ts';
import { buildPlacementGraph } from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import { solvePlacementSubtreeSync } from '../src/pcb-layout/pcb-auto-place-v2/tree-solver.ts';

test('default board layer is used for unrestricted components, while explicit and fixed layers survive', () => {
    assert.deepEqual(preferredPlacementLayers(['top', 'bottom'], 'bottom'), ['bottom', 'top']);
    assert.deepEqual(preferredPlacementLayers(['top'], 'bottom'), ['top']);

    const input = JSON.parse(readFileSync('docs/experimental/pcb/global-placement-2026-09-27/Telemetry/input.json', 'utf8')) as PlacementInput;
    input.board.allowedLayers = ['top', 'bottom'];
    input.board.defaultLayer = 'bottom';
    const unrestricted = input.components.find((item) => item.designator === 'R30')!;
    const explicitTop = input.components.find((item) => item.designator === 'R31')!;
    const fixedTop = input.components.find((item) => item.designator === 'J5')!;
    unrestricted.pcb.allowedLayers = ['top', 'bottom'];
    explicitTop.pcb.allowedLayers = ['top'];
    fixedTop.pcb.allowedLayers = ['top', 'bottom'];
    fixedTop.pcb.fixedPlacement = { x: 0, y: 0, rotate: 0, layer: 'top' };
    const graph = buildPlacementGraph(input);
    const find = (node: PlacementTreeNode, designator: string): PlacementTreeNode | undefined =>
        node.kind === 'component' && node.label === designator
            ? node
            : node.children.map((child) => find(child, designator)).find(Boolean);
    for (const [designator, expectedLayer] of [['R30', 'bottom'], ['R31', 'top'], ['J5', 'top']] as const) {
        const node = find(graph.root, designator)!;
        const result = solvePlacementSubtreeSync({ input, graph, node });
        assert.equal(result.root.placements[0]?.layer, expectedLayer, designator);
    }
    assert.deepEqual(unrestricted.pcb.allowedLayers, ['top', 'bottom'], 'solver must not mutate its input');
});
