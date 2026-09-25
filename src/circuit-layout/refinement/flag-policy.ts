import type { CircuitComponent } from '#types/circuit.ts';
import type { ElkExtendedEdge } from 'elkjs';
import { type Placed, normal, path, routeLength, pinPositions } from './geometry.ts';
import { getPartUuid } from '#types/lcsc.ts';

export const FLAG_ROTATION_POLICY = Object.freeze({ minimumFraction: 0.3 });
const preferredNormal = (flag: CircuitComponent) => flag.part_uuid && getPartUuid(flag.part_uuid) === 'GND' ? -1 : 1;

/** A private flag travels with its owner. Its orientation is a soft preference,
 * never a veto on improving the complete attachment. Turning a flag alone
 * against its preferred direction requires a substantial lead reduction. */
export function acceptsFlagOrientation(before: Placed, after: Placed, flag: CircuitComponent,
    oldEdges: readonly ElkExtendedEdge[], newEdges: readonly ElkExtendedEdge[], privateLead: boolean, movesWithOwner = false) {
    const id = before.ports![0].id;
    if (privateLead && movesWithOwner) return true;
    if (normal(after, id).y === preferredNormal(flag) || normal(before, id).y === normal(after, id).y) return true;
    const leadLength = (edges: readonly ElkExtendedEdge[]) => edges
        .filter(e => [...e.sources, ...e.targets].includes(id)).reduce((sum, e) => sum + routeLength(path(e)), 0);
    return privateLead && leadLength(newEdges) < leadLength(oldEdges) * (1 - FLAG_ROTATION_POLICY.minimumFraction);
}

/** Ground below its neighbours, supply/signal flags above them. These remain
 * preferences: an exceptional, much shorter private lead may outweigh them. */
export function flagReadabilityCost(nodes: readonly Placed[], edges: readonly ElkExtendedEdge[], flags: ReadonlyMap<string, CircuitComponent>) {
    const pins = pinPositions([...nodes]);
    let cost = 0;
    for (const node of nodes) {
        const flag = flags.get(node.id); if (!flag || node.ports?.length !== 1) continue;
        const id = node.ports[0].id, preferred = preferredNormal(flag);
        const leads = edges.filter(e => [...e.sources, ...e.targets].includes(id));
        const neighbours = leads
            .flatMap(e => [...e.sources, ...e.targets]).filter(p => p !== id).map(p => pins.get(p)).filter(p => !!p);
        const y = pins.get(id)!.y;
        if (normal(node, id).y !== preferred || (neighbours.length
            && (preferred < 0 ? y < Math.max(...neighbours.map(p => p.y)) : y > Math.min(...neighbours.map(p => p.y))))) {
            // Scale the preference with this lead instead of imposing another
            // hidden absolute threshold on small drawings.
            cost += leads.reduce((sum, e) => sum + routeLength(path(e)), 0)
                * FLAG_ROTATION_POLICY.minimumFraction / (1 - FLAG_ROTATION_POLICY.minimumFraction);
        }
    }
    return cost;
}
