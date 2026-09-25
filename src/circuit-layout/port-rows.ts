import type { ElkNode, ElkExtendedEdge } from 'elkjs';
import type { CircuitComponent } from '#types/circuit.ts';
import { getPartUuid } from '#types/lcsc.ts';

/** Collapse generated leaf flags into horizontal placement objects. Original
 * port IDs and symbol geometry survive expansion, so this is not a new label
 * representation or an electrical graph transformation. */
export function collapsePortRows(block: ElkNode, edges: ElkExtendedEdge[], flags: ReadonlyMap<string, CircuitComponent>) {
    const nodes = block.children ?? [];
    const owner = new Map(nodes.flatMap(n => (n.ports ?? []).map(p => [p.id, n])));
    const groups = new Map<string, { node: ElkNode; coordinate: number }[]>();
    for (const node of nodes) {
        const flag = flags.get(node.id);
        if (!flag || node.ports?.length !== 1) continue;
        const pin = node.ports[0], incident = edges.filter(e => [...e.sources, ...e.targets].includes(pin.id));
        if (incident.length !== 1) continue;
        const anchor = [...incident[0].sources, ...incident[0].targets].find(p => p !== pin.id)!, parent = owner.get(anchor);
        if (!parent || flags.has(parent.id) || parent.children?.length) continue;
        const p = parent.ports!.find(p => p.id === anchor)!;
        const face = [['W', p.x ?? 0], ['E', parent.width! - (p.x ?? 0)], ['N', p.y ?? 0], ['S', parent.height! - (p.y ?? 0)]] as const;
        const side = [...face].sort((a, b) => a[1] - b[1])[0][0];
        const key = `${parent.id}:${side}:${flag.part_uuid ? getPartUuid(flag.part_uuid) : ''}:${pin.y === 0 ? 'up' : 'down'}`;
        const list = groups.get(key) ?? [];
        list.push({ node, coordinate: side === 'W' || side === 'E' ? p.y! : p.x! }); groups.set(key, list);
    }
    const rows = new Map<string, ElkNode[]>(), removed = new Set<string>(), replacements: ElkNode[] = [];
    for (const [key, group] of [...groups].sort(([a], [b]) => a.localeCompare(b))) {
        group.sort((a, b) => a.coordinate - b.coordinate || a.node.id.localeCompare(b.node.id));
        for (let offset = 0; offset < group.length; offset += 8) {
            const chunk = group.slice(offset, offset + 8);
            if (chunk.length < 2) continue;
            const id = `port-row:${key}:${offset}`, height = Math.max(...chunk.map(c => c.node.height!));
            let x = 0;
            const children = chunk.map(({ node }) => {
                const result = { ...node, x, y: 0 }; x += node.width! + 15; removed.add(node.id); return result;
            });
            rows.set(id, children);
            replacements.push({ id, width: x - 15, height, layoutOptions: { 'elk.portConstraints': 'FIXED_POS' },
                ports: children.flatMap(n => n.ports!.map(p => ({ ...p, x: n.x! + p.x!, y: n.y! + p.y! }))) });
        }
    }
    block.children = [...nodes.filter(n => !removed.has(n.id)), ...replacements];
    return (result: ElkNode) => {
        result.children = result.children?.flatMap(n => rows.has(n.id)
            ? rows.get(n.id)!.map(child => ({ ...child, x: n.x! + child.x!, y: n.y! + child.y! })) : [n]);
    };
}
