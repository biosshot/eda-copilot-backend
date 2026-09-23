import { writeFile } from 'node:fs/promises';
import type { ElkNode } from 'elkjs';
import { createCanvas } from 'canvas';

function flattenNodes(node: ElkNode, parentX = 0, parentY = 0): ElkNode[] {
    const x = parentX + (node.x ?? 0);
    const y = parentY + (node.y ?? 0);
    const flat: ElkNode[] = [{ ...node, x, y }];

    if (node.children) {
        for (const child of node.children) {
            flat.push(...flattenNodes(child, x, y));
        }
    }

    return flat;
}

function findAllNodes(graph: ElkNode): ElkNode[] {
    const nodes: ElkNode[] = [];
    for (const child of graph.children ?? []) {
        nodes.push(...flattenNodes(child));
    }
    return nodes;
}

export function renderElkGraphToCanvas(
    graph: ElkNode,
    outputPath: string | undefined = undefined,
    padding: number = 0
): Promise<void> {
    const canvasWidth = Math.max((graph.width ?? 100) + 2 * padding, 800);
    const canvasHeight = Math.max((graph.height ?? 100) + 2 * padding, 600);

    const canvas = createCanvas(canvasWidth, canvasHeight);
    const ctx = canvas.getContext('2d');

    // Clear background
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(0, 0, canvasWidth, canvasHeight);

    const offsetX = padding;
    const offsetY = padding;
    const scale = 1;

    // Draw edges
    ctx.strokeStyle = '#000000';
    ctx.lineWidth = 2;

    for (const edge of graph.edges ?? []) {
        ctx.beginPath();

        if (!edge.sections) continue;

        for (const section of edge.sections) {
            const startX = section.startPoint.x * scale + offsetX;
            const startY = section.startPoint.y * scale + offsetY;
            const endX = section.endPoint.x * scale + offsetX;
            const endY = section.endPoint.y * scale + offsetY;

            ctx.moveTo(startX, startY);

            if (section.bendPoints && section.bendPoints.length > 0) {
                for (const bp of section.bendPoints) {
                    ctx.lineTo(bp.x * scale + offsetX, bp.y * scale + offsetY);
                }
            }

            ctx.lineTo(endX, endY);
        }
        ctx.stroke();
    }

    // Gather all leaf nodes
    const allNodes = findAllNodes(graph);

    // Draw nodes
    ctx.strokeStyle = '#000000';
    ctx.fillStyle = '#000000';
    ctx.lineWidth = 1;
    ctx.font = '12px Arial';

    for (const node of allNodes) {
        // Skip container nodes without explicit width/height (assumed to be groups)
        if (!node.width || !node.height) continue;

        const x = (node.x ?? 0) * scale + offsetX;
        const y = (node.y ?? 0) * scale + offsetY;
        const w = node.width;
        const h = node.height;

        // Draw rectangle
        ctx.strokeRect(x, y, w, h);

        // Draw label
        ctx.fillText(node.id, x + 5, y + 15);

        // Draw ports (as small red circles)
        if (node.ports && node.ports.length > 0) {
            ctx.fillStyle = '#ff0000';
            for (const port of node.ports) {
                const px = ((node.x ?? 0) + (port.x ?? 0)) * scale + offsetX;
                const py = ((node.y ?? 0) + (port.y ?? 0)) * scale + offsetY;
                ctx.beginPath();
                ctx.arc(px, py, 3, 0, Math.PI * 2);
                ctx.fill();
            }
            ctx.fillStyle = '#000000';
        }
    }

    // Save image
    const buffer = canvas.toBuffer('image/png');

    if (!outputPath) return Promise.resolve();

    return writeFile(outputPath, buffer);
}
