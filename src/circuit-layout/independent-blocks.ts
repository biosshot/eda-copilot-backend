import ELK from 'elkjs';
import type { ElkNode } from 'elkjs';
import { BASELINE_LAYOUT_PROFILE, LOCAL_LAYOUT_PROFILES } from './profiles.ts';
import { evaluateLayoutQuality, safelyImprovesLayout, safelyImprovesExtremeAspect } from './quality.ts';
import { canonicalGraph } from './graph-order.ts';
import { packSchematicRectangles, SCHEMATIC_SHEET } from '#utils/schematic-packing.ts';
import { collapsePortRows } from './port-rows.ts';
import type { CircuitComponent } from '#types/circuit.ts';

/** Solve disconnected functional blocks in their own coordinate systems.
 * Real cross-block wires require the hierarchical solver and are never cut. */
export async function layoutIndependentBlocks(graph: ElkNode, elk: InstanceType<typeof ELK>, flags: readonly CircuitComponent[] = []): Promise<ElkNode | undefined> {
    const root = graph.children?.[0];
    if (graph.children?.length !== 1 || !root?.children?.length) return undefined;
    const separated = root.children.every(b => b.children?.length);
    const blocks = separated ? root.children : [root];
    const owner = new Map<string, string>();
    const visit = (node: ElkNode, block: string) => {
        owner.set(node.id, block);
        for (const p of node.ports ?? []) owner.set(p.id, block);
        for (const child of node.children ?? []) visit(child, block);
    };
    blocks.forEach(b => visit(b, b.id));
    if ((graph.edges ?? []).some(e => {
        const scopes = new Set([...e.sources, ...e.targets].map(p => owner.get(p)));
        return scopes.size !== 1 || scopes.has(undefined);
    })) return undefined;

    const solved: ElkNode[] = [], edges = [] as NonNullable<ElkNode['edges']>;
    for (const block of [...blocks].sort((a, b) => a.id.localeCompare(b.id))) {
        const localEdges = (graph.edges ?? []).filter(e => owner.get(e.sources[0]) === block.id);
        let best: ElkNode | undefined;
        let quality: ReturnType<typeof evaluateLayoutQuality> | undefined;
        const directions = [block.layoutOptions?.['org.eclipse.elk.direction'] ?? 'RIGHT', 'DOWN'];
        const candidates = (separated ? [BASELINE_LAYOUT_PROFILE, ...LOCAL_LAYOUT_PROFILES] : [BASELINE_LAYOUT_PROFILE])
            .map(profile => ({ profile, direction: directions[0], rows: false }));
        if (flags.length) for (const direction of new Set(separated ? directions : directions.slice(0, 1))) candidates.push({ profile: LOCAL_LAYOUT_PROFILES[0], direction, rows: true });
        for (const { profile, direction, rows } of candidates) {
            const child = structuredClone(block);
            const expand = rows ? collapsePortRows(child, localEdges, new Map(flags.map(c => [c.designator, c]))) : undefined;
            child.layoutOptions = { ...child.layoutOptions, ...profile.options,
                'org.eclipse.elk.direction': direction };
            const local: ElkNode = { id: 'root', children: [child], edges: structuredClone(localEdges),
                layoutOptions: { 'org.eclipse.elk.randomSeed': '1' } };
            canonicalGraph(local);
            const result = await elk.layout(local, { layoutOptions: BASELINE_LAYOUT_PROFILE.options });
            expand?.(result.children![0]);
            const next = evaluateLayoutQuality(result);
            // Composition candidates trade area against route complexity using
            // the complete score. A per-metric crossing veto would make a new
            // row impossible even when it halves the occupied area. Electrical
            // coverage, body clearance and foreign collinear overlaps stay hard.
            const betterComposition = rows && quality && next.valid && next.nodeCount === quality.nodeCount
                && next.edgeCount === quality.edgeCount && next.routedEdgeCount === quality.routedEdgeCount
                && next.wireThroughNodeCount <= quality.wireThroughNodeCount
                && next.collinearOverlapCount <= quality.collinearOverlapCount && next.score < quality.score * 0.95;
            if (!best || !quality || safelyImprovesLayout(next, quality) || safelyImprovesExtremeAspect(next, quality) || betterComposition) {
                best = result; quality = next;
            }
        }
        const child = best!.children![0];
        // ELK normally stores routes relative to their common ancestor. Root
        // routes need rebasing before the child is moved into the page.
        for (const edge of best!.edges ?? []) {
            if (!edge.container || edge.container === best!.id) {
                for (const section of edge.sections ?? []) for (const p of [section.startPoint, ...(section.bendPoints ?? []), section.endPoint]) {
                    p.x -= child.x ?? 0; p.y -= child.y ?? 0;
                }
                edge.container = child.id;
            }
            edges.push(edge);
        }
        solved.push(child);
    }
    const packed = packSchematicRectangles(solved.map(b => ({ id: b.id, width: b.width!, height: b.height! })),
        SCHEMATIC_SHEET.blockPadding * 2 + SCHEMATIC_SHEET.extraBlockGap);
    for (const block of solved) Object.assign(block, packed.positions.get(block.id));
    if (!separated) return { ...graph, x: 0, y: 0, width: packed.width, height: packed.height, edges, children: solved };
    return { ...graph, x: 0, y: 0, width: packed.width, height: packed.height, edges,
        children: [{ ...root, x: 0, y: 0, width: packed.width, height: packed.height, children: solved }] };
}
