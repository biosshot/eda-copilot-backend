import type { ElkNode } from 'elkjs';
import { straightRuns } from './refinement/net-routes.ts';

type Point = { x: number; y: number };
type Box = { id: string; left: number; top: number; right: number; bottom: number };
type Segment = { edgeId: string; a: Point; b: Point };

type SchematicNode = ElkNode & {
    rotate?: number;
    center?: { x?: number; y?: number };
};

export type LayoutQuality = {
    valid: boolean;
    width: number;
    height: number;
    area: number;
    aspectRatio: number;
    nodeCount: number;
    edgeCount: number;
    routedEdgeCount: number;
    nodeArea: number;
    wireLength: number;
    bendCount: number;
    crossingCount: number;
    collinearOverlapCount: number;
    nodeOverlapCount: number;
    wireThroughNodeCount: number;
    score: number;
};

export function layoutCompatibilitySignature(root: ElkNode) {
    const nodes: string[] = [];
    const edges: string[] = [];

    const visit = (node: SchematicNode) => {
        if (node.children?.length) {
            for (const child of node.children as SchematicNode[]) visit(child);
        } else {
            nodes.push(JSON.stringify([
                node.id,
                node.width ?? null,
                node.height ?? null,
                node.rotate ?? 0,
                node.center?.x ?? null,
                node.center?.y ?? null,
            ]));
        }

        for (const edge of node.edges ?? []) {
            const sectionEndpoints = (edge.sections ?? []).map(section => JSON.stringify([
                section.incomingShape ?? null,
                section.outgoingShape ?? null,
            ])).sort();
            edges.push(JSON.stringify([
                edge.id,
                [...edge.sources].sort(),
                [...edge.targets].sort(),
                sectionEndpoints,
            ]));
        }
    };

    visit(root as SchematicNode);
    return JSON.stringify([nodes.sort(), edges.sort()]);
}

function finite(value: unknown): value is number {
    return typeof value === 'number' && Number.isFinite(value);
}

function leafBoxes(root: ElkNode) {
    const boxes: Box[] = [];
    const offsets = new Map<string, Point>();

    const visit = (node: ElkNode, parent: Point) => {
        const offset = {
            x: parent.x + (finite(node.x) ? node.x : 0),
            y: parent.y + (finite(node.y) ? node.y : 0),
        };
        offsets.set(node.id, offset);

        if (node.children?.length) {
            for (const child of node.children) visit(child, offset);
            return;
        }

        if (!finite(node.width) || !finite(node.height)) return;
        boxes.push({
            id: node.id,
            left: offset.x,
            top: offset.y,
            right: offset.x + node.width,
            bottom: offset.y + node.height,
        });
    };

    visit(root, { x: 0, y: 0 });
    return { boxes, offsets };
}

function edgeSegments(root: ElkNode, offsets: Map<string, Point>) {
    const segments: Segment[] = [];
    const parent = new Map<string, string>();
    const netOf = (id: string): string => {
        const p = parent.get(id);
        if (!p || p === id) { parent.set(id, id); return id; }
        const net = netOf(p); parent.set(id, net); return net;
    };
    let edgeCount = 0;
    let routedEdgeCount = 0;
    let bendCount = 0;
    let wireLength = 0;

    const collect = (node: ElkNode, inheritedOffset: Point) => {
        const nodeOffset = offsets.get(node.id) ?? inheritedOffset;
        for (const edge of node.edges ?? []) {
            for (const id of [...edge.sources, ...edge.targets]) parent.set(netOf(id), netOf(edge.sources[0]));
            edgeCount++;
            if (edge.sections?.length) routedEdgeCount++;
            const edgeOffset = edge.container
                ? offsets.get(edge.container) ?? nodeOffset
                : nodeOffset;

            for (const section of edge.sections ?? []) {
                const points = [section.startPoint, ...(section.bendPoints ?? []), section.endPoint]
                    .map(point => ({ x: point.x + edgeOffset.x, y: point.y + edgeOffset.y }));
                bendCount += section.bendPoints?.length ?? 0;

                for (let index = 1; index < points.length; index++) {
                    const a = points[index - 1];
                    const b = points[index];
                    wireLength += Math.abs(b.x - a.x) + Math.abs(b.y - a.y);
                    if (a.x !== b.x || a.y !== b.y) segments.push({ edgeId: edge.sources[0], a, b });
                }
            }
        }

        for (const child of node.children ?? []) collect(child, nodeOffset);
    };

    collect(root, { x: 0, y: 0 });
    // A shared supply trunk is drawn once, even when many logical edges use
    // it. Counting its copies as crossings unfairly rejects compact port rows.
    const byNet = new Map<string, Segment[]>();
    for (const segment of segments) { const net = netOf(segment.edgeId), list = byNet.get(net) ?? []; list.push(segment); byNet.set(net, list); }
    const physical = [...byNet].flatMap(([edgeId, parts]) => straightRuns(parts).map(s => ({ ...s, edgeId })));
    wireLength = physical.reduce((sum, s) => sum + Math.abs(s.a.x - s.b.x) + Math.abs(s.a.y - s.b.y), 0);
    return { segments: physical, edgeCount, routedEdgeCount, bendCount, wireLength };
}

function strictlyOverlaps(a1: number, a2: number, b1: number, b2: number) {
    return Math.min(a2, b2) - Math.max(a1, b1) > 1e-6;
}

function nodeOverlaps(boxes: Box[]) {
    let count = 0;
    for (let left = 0; left < boxes.length; left++) {
        for (let right = left + 1; right < boxes.length; right++) {
            const a = boxes[left];
            const b = boxes[right];
            if (strictlyOverlaps(a.left, a.right, b.left, b.right)
                && strictlyOverlaps(a.top, a.bottom, b.top, b.bottom)) count++;
        }
    }
    return count;
}

function segmentInteractions(segments: Segment[]) {
    let crossingCount = 0;
    let collinearOverlapCount = 0;

    for (let left = 0; left < segments.length; left++) {
        for (let right = left + 1; right < segments.length; right++) {
            const a = segments[left];
            const b = segments[right];
            if (a.edgeId === b.edgeId) continue;

            const aVertical = a.a.x === a.b.x;
            const bVertical = b.a.x === b.b.x;
            if (aVertical === bVertical) {
                if (aVertical && a.a.x === b.a.x
                    && strictlyOverlaps(a.a.y, a.b.y, b.a.y, b.b.y)) collinearOverlapCount++;
                if (!aVertical && a.a.y === b.a.y
                    && strictlyOverlaps(a.a.x, a.b.x, b.a.x, b.b.x)) collinearOverlapCount++;
                continue;
            }

            const vertical = aVertical ? a : b;
            const horizontal = aVertical ? b : a;
            const x = vertical.a.x;
            const y = horizontal.a.y;
            const withinVertical = x > Math.min(horizontal.a.x, horizontal.b.x) + 1e-6
                && x < Math.max(horizontal.a.x, horizontal.b.x) - 1e-6;
            const withinHorizontal = y > Math.min(vertical.a.y, vertical.b.y) + 1e-6
                && y < Math.max(vertical.a.y, vertical.b.y) - 1e-6;
            if (withinVertical && withinHorizontal) crossingCount++;
        }
    }

    return { crossingCount, collinearOverlapCount };
}

function wireThroughNodes(segments: Segment[], boxes: Box[]) {
    let count = 0;
    for (const segment of segments) {
        const vertical = segment.a.x === segment.b.x;
        for (const box of boxes) {
            if (vertical) {
                if (segment.a.x <= box.left + 1e-6 || segment.a.x >= box.right - 1e-6) continue;
                if (strictlyOverlaps(segment.a.y, segment.b.y, box.top, box.bottom)) count++;
            } else {
                if (segment.a.y <= box.top + 1e-6 || segment.a.y >= box.bottom - 1e-6) continue;
                if (strictlyOverlaps(segment.a.x, segment.b.x, box.left, box.right)) count++;
            }
        }
    }
    return count;
}

/** Sheet-like landscape drawings are preferred, with extra height more costly
 * than extra width. Real area remains the base cost, so blank width never wins. */
export function effectiveLayoutArea(width: number, height: number, verticalWeight = 1.5) {
    if (width <= 0 || height <= 0) return 0;
    const target = Math.SQRT2;
    const penalty = verticalWeight * Math.max(0, target * height / width - 1) ** 2
        + 0.15 * Math.max(0, width / (target * height) - 1) ** 2;
    return width * height * (1 + Math.min(9, penalty));
}

export function evaluateLayoutQuality(graph: ElkNode): LayoutQuality {
    const width = finite(graph.width) ? graph.width : 0;
    const height = finite(graph.height) ? graph.height : 0;
    const area = width * height;
    const aspectRatio = width > 0 && height > 0 ? Math.max(width / height, height / width) : Infinity;
    const { boxes, offsets } = leafBoxes(graph);
    const { segments, edgeCount, routedEdgeCount, bendCount, wireLength } = edgeSegments(graph, offsets);
    const nodeArea = boxes.reduce((sum, box) => sum + (box.right - box.left) * (box.bottom - box.top), 0);
    const nodeOverlapCount = nodeOverlaps(boxes);
    const { crossingCount, collinearOverlapCount } = segmentInteractions(segments);
    const wireThroughNodeCount = wireThroughNodes(segments, boxes);
    const nodeCount = boxes.length;
    const typicalNodeSize = Math.sqrt(nodeArea / Math.max(nodeCount, 1)) || 1;
    const normalizedArea = effectiveLayoutArea(width, height) / Math.max(nodeArea, 1);
    const normalizedWire = wireLength / (Math.max(edgeCount, 1) * typicalNodeSize);
    const normalizedBends = bendCount / Math.max(edgeCount, 1);
    const score = normalizedArea
        + normalizedWire * 1.2
        + normalizedBends * 0.8
        + crossingCount / Math.max(edgeCount, 1) * 12
        + collinearOverlapCount / Math.max(edgeCount, 1) * 6
        + wireThroughNodeCount / Math.max(edgeCount, 1) * 12;
    const valid = width > 0
        && height > 0
        && Number.isFinite(score)
        && boxes.length > 0
        && nodeOverlapCount === 0;

    return {
        valid,
        width,
        height,
        area,
        aspectRatio,
        nodeCount,
        edgeCount,
        routedEdgeCount,
        nodeArea,
        wireLength,
        bendCount,
        crossingCount,
        collinearOverlapCount,
        nodeOverlapCount,
        wireThroughNodeCount,
        score,
    };
}

export function safelyImprovesLayout(candidate: LayoutQuality, baseline: LayoutQuality) {
    if (!candidate.valid) return false;
    if (candidate.nodeCount !== baseline.nodeCount || candidate.edgeCount !== baseline.edgeCount) return false;
    if (candidate.routedEdgeCount !== baseline.routedEdgeCount) return false;
    if (candidate.nodeOverlapCount > baseline.nodeOverlapCount) return false;
    if (candidate.wireThroughNodeCount > baseline.wireThroughNodeCount) return false;
    if (candidate.crossingCount > baseline.crossingCount) return false;
    if (candidate.collinearOverlapCount > baseline.collinearOverlapCount) return false;
    return candidate.score < baseline.score * 0.985;
}

export function safelyImprovesExtremeAspect(candidate: LayoutQuality, baseline: LayoutQuality) {
    if (!candidate.valid || baseline.aspectRatio < 4) return false;
    if (candidate.nodeCount !== baseline.nodeCount || candidate.edgeCount !== baseline.edgeCount) return false;
    if (candidate.routedEdgeCount !== baseline.routedEdgeCount) return false;
    if (candidate.nodeOverlapCount > baseline.nodeOverlapCount) return false;
    if (candidate.wireThroughNodeCount > baseline.wireThroughNodeCount) return false;
    if (candidate.aspectRatio >= baseline.aspectRatio * 0.85) return false;
    if (candidate.score >= baseline.score * 0.8) return false;

    const baselineRoutingConflicts = baseline.crossingCount + baseline.collinearOverlapCount;
    const candidateRoutingConflicts = candidate.crossingCount + candidate.collinearOverlapCount;
    return candidateRoutingConflicts <= baselineRoutingConflicts * 1.05;
}
