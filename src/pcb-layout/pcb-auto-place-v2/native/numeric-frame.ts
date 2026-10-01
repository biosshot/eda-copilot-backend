import type { Point, Box } from '#types/pcb/layout-model.ts';
import type { PlacementPrimitive } from '../primitives.ts';
import { f32 } from '../../f32.ts';

export interface NativeNumericFrame {
    precision: 'f32-rte-ftz-v1';
    origin: Point;
    outputOrigin: Point;
}

/** Transport only: subtract before narrowing, or restore after F32 geometry. */
export function primitiveInFrame(primitive: PlacementPrimitive, origin: Point, restore = false): PlacementPrimitive {
    const coordinate = (value: number, offset: number) => restore ? value + offset : f32(value - offset);
    const point = <T extends Point>(value: T): T => ({ ...value,
        x: coordinate(value.x, origin.x), y: coordinate(value.y, origin.y) });
    const box = (value: Box): Box => ({ left: coordinate(value.left, origin.x), right: coordinate(value.right, origin.x),
        top: coordinate(value.top, origin.y), bottom: coordinate(value.bottom, origin.y) });
    return { ...primitive, bbox: box(primitive.bbox),
        collisionBoxes: primitive.collisionBoxes?.map(box), placements: primitive.placements.map(point),
        connectionPoints: primitive.connectionPoints.map(point), pathPorts: primitive.pathPorts?.map(point),
        children: primitive.children.map(child => primitiveInFrame(child, origin, restore)),
        layoutAlternatives: primitive.layoutAlternatives?.map(child => primitiveInFrame(child, origin, restore)) };
}

export function frameTranslation(value: number, inputOrigin: number, outputOrigin: number): number {
    // Centered blocks expose a displacement from their authored absolute source.
    return f32(value - (outputOrigin - inputOrigin));
}
