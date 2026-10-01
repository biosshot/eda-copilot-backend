import * as fp from '../f32.ts';
import { coordinateAdd, sourceCoordinateAdd, coordinateDifference, coordinateCenter, coordinateRound, coordinateOrigin } from '../coordinate-transport.ts';
import { rotatePoint, sinCosDegrees, roundPlacement as canonicalRoundPlacement } from '../f32.ts';
import type { BoardAnchor, BoardEdge, BoardHole, Box, CenteredRectBoard, FootprintPad, FootprintSpec, Layer, PcbComponent, Placement, Point } from '../../types/pcb/layout-model.ts';

export const GEOMETRY_EPSILON = 1e-6;

// Placement coordinates are rounded to the grid (typically 0.5 mm) and then to 3 decimals.
// Use a slightly larger epsilon for overlap/clearance checks so that tiny floating-point
// rounding differences do not create false positives when the gap equals the required clearance.
export const PLACEMENT_EPSILON = 0.005;

export function getPadWorld(component: PcbComponent, placement: Placement, pin: string | number) {
    const pad = component.footprint.pads.find((pad) => String(pad.pin_number) === String(pin));
    return pad ? getLocalPointWorld(placement, pad) : null;
}

export function getPadOffset(component: PcbComponent, pin: string | number, rotate: number, layer: Layer = 'top') {
    const pad = component.footprint.pads.find((pad) => String(pad.pin_number) === String(pin));
    return pad ? getLocalPointOffset(pad, rotate, layer) : null;
}

export function getLocalPointWorld(placement: Placement, point: Point) {
    const offset = getLocalPointOffset(point, placement.rotate, placement.layer);
    return {
        x: sourceCoordinateAdd(placement.x, offset.x),
        y: sourceCoordinateAdd(placement.y, offset.y),
    };
}

export function getLocalPointOffset(point: Point, rotate: number, layer: Layer = 'top') {
    const local = layer === 'bottom'
        ? { x: -point.x, y: point.y }
        : point;
    return rotatePoint(local, rotate);
}

export function getBox(component: PcbComponent, placement: Placement): Box {
    const footprint = component.footprint;
    const [rawSin, rawCos] = sinCosDegrees(placement.rotate);
    const cos = fp.abs(rawCos);
    const sin = fp.abs(rawSin);
    const halfWidth = fp.div((fp.add(fp.mul(footprint.width, cos), fp.mul(footprint.height, sin))), 2);
    const halfHeight = fp.div((fp.add(fp.mul(footprint.width, sin), fp.mul(footprint.height, cos))), 2);

    return {
        left: sourceCoordinateAdd(placement.x, -halfWidth),
        right: sourceCoordinateAdd(placement.x, halfWidth),
        top: sourceCoordinateAdd(placement.y, -halfHeight),
        bottom: sourceCoordinateAdd(placement.y, halfHeight),
    };
}

export function boardBox(board: CenteredRectBoard): Box {
    return {
        left: fp.div(-board.outline.width, 2),
        right: fp.div(board.outline.width, 2),
        top: fp.div(-board.outline.height, 2),
        bottom: fp.div(board.outline.height, 2),
    };
}

export function rectBoardPolygon(width: number, height: number): Point[] {
    return [
        { x: fp.div(-width, 2), y: fp.div(-height, 2) },
        { x: fp.div(width, 2), y: fp.div(-height, 2) },
        { x: fp.div(width, 2), y: fp.div(height, 2) },
        { x: fp.div(-width, 2), y: fp.div(height, 2) },
    ];
}

export function boardOutlinePolygon(board: CenteredRectBoard): Point[] {
    return board.outline.type === 'polygon'
        ? board.outline.points
        : rectBoardPolygon(board.outline.width, board.outline.height);
}

export function pointsBox(points: Point[]): Box {
    if (points.length === 0) return { left: 0, right: 0, top: 0, bottom: 0 };
    let left = points[0].x;
    let right = points[0].x;
    let top = points[0].y;
    let bottom = points[0].y;
    for (let index = 1; index < points.length; index += 1) {
        const point = points[index];
        if (point.x < left) left = point.x;
        if (point.x > right) right = point.x;
        if (point.y < top) top = point.y;
        if (point.y > bottom) bottom = point.y;
    }
    return { left, right, top, bottom };
}

export function pointInBoard(board: CenteredRectBoard, point: Point, edgeClearance = 0) {
    const box = boardBox(board);
    if (
        point.x < fp.sub(fp.add(box.left, edgeClearance), GEOMETRY_EPSILON)
        || point.x > fp.add(fp.sub(box.right, edgeClearance), GEOMETRY_EPSILON)
        || point.y < fp.sub(fp.add(box.top, edgeClearance), GEOMETRY_EPSILON)
        || point.y > fp.add(fp.sub(box.bottom, edgeClearance), GEOMETRY_EPSILON)
    ) {
        return false;
    }

    if (board.outline.type !== 'polygon') return true;
    if (!pointInPolygon(point, board.outline.points)) return false;
    return edgeClearance <= 0 || fp.add(pointToPolygonDistance(point, board.outline.points), GEOMETRY_EPSILON) >= edgeClearance;
}

export function boxInsideBoard(board: CenteredRectBoard, box: Box, edgeClearance = 0) {
    const corners = [
        { x: box.left, y: box.top },
        { x: box.right, y: box.top },
        { x: box.right, y: box.bottom },
        { x: box.left, y: box.bottom },
    ];
    if (!corners.every((corner) => pointInBoard(board, corner, edgeClearance))) return false;
    if (board.outline.type !== 'polygon') return true;

    const boxEdges = corners.map((corner, index) => [corner, corners[(index + 1) % corners.length]] as const);
    const outline = board.outline.points;
    return boxEdges.every(([a, b]) => outline.every((point, index) =>
        segmentIntersection(a, b, point, outline[(index + 1) % outline.length]) === null));
}

export function pointInPolygon(point: Point, polygon: Point[]) {
    const ox = coordinateOrigin(point.x), oy = coordinateOrigin(point.y);
    if (ox !== 0 || oy !== 0) {
        const local = (p: Point) => ({ x: fp.f32(p.x - ox), y: fp.f32(p.y - oy) });
        return pointInPolygon(local(point), polygon.map(local));
    }
    if (polygon.length < 3) return false;
    let inside = false;
    for (let i = 0, j = polygon.length - 1; i < polygon.length; j = i++) {
        const a = polygon[i];
        const b = polygon[j];
        if (pointOnSegment(point, a, b)) return true;
        const intersects = ((a.y > point.y) !== (b.y > point.y))
            && point.x < fp.add(fp.div((fp.mul((fp.sub(b.x, a.x)), (fp.sub(point.y, a.y)))), (fp.sub(b.y, a.y))), a.x);
        if (intersects) inside = !inside;
    }
    return inside;
}

export function pointToPolygonDistance(point: Point, polygon: Point[]) {
    if (polygon.length === 0) return Infinity;
    let min = Infinity;
    for (let index = 0; index < polygon.length; index += 1) {
        const a = polygon[index];
        const b = polygon[(index + 1) % polygon.length];
        const distance = pointToSegmentDistance(point, a, b);
        if (distance < min) min = distance;
    }
    return min;
}

export function pointToSegmentDistance(point: Point, a: Point, b: Point) {
    const ox = coordinateOrigin(point.x), oy = coordinateOrigin(point.y);
    if (ox !== 0 || oy !== 0) {
        const local = (p: Point) => ({ x: fp.f32(p.x - ox), y: fp.f32(p.y - oy) });
        return pointToSegmentDistance(local(point), local(a), local(b));
    }
    const dx = fp.sub(b.x, a.x);
    const dy = fp.sub(b.y, a.y);
    const lengthSquared = fp.add(fp.mul(dx, dx), fp.mul(dy, dy));
    if (lengthSquared < GEOMETRY_EPSILON) return dist(point, a);
    const t = fp.max(0, fp.min(1, fp.div((fp.add(fp.mul((fp.sub(point.x, a.x)), dx), fp.mul((fp.sub(point.y, a.y)), dy))), lengthSquared)));
    const projectionX = fp.add(a.x, fp.mul(t, dx));
    const projectionY = fp.add(a.y, fp.mul(t, dy));
    const projectionDx = fp.sub(point.x, projectionX);
    const projectionDy = fp.sub(point.y, projectionY);
    return fp.sqrt(fp.add(fp.mul(projectionDx, projectionDx), fp.mul(projectionDy, projectionDy)));
}

export function outlineInsetPointFromCorner(board: CenteredRectBoard, corner: Exclude<BoardAnchor, 'board.center' | 'board.left' | 'board.right' | 'board.top' | 'board.bottom'>, inset: number) {
    const boxCorner = boardAnchorPoint(board, corner);
    if (board.outline.type !== 'polygon') {
        return {
            x: fp.add(boxCorner.x, (corner.includes('left') ? inset : -inset)),
            y: fp.add(boxCorner.y, (corner.includes('top') ? inset : -inset)),
        };
    }
    const directionLength = fp.hypot(boxCorner.x, boxCorner.y);
    if (directionLength < GEOMETRY_EPSILON) return boxCorner;
    const direction = { x: fp.div(-boxCorner.x, directionLength), y: fp.div(-boxCorner.y, directionLength) };
    const center = { x: 0, y: 0 };
    const polygon = boardOutlinePolygon(board);
    const intersections = polygon
        .map((point, index) => segmentIntersection(boxCorner, center, point, polygon[(index + 1) % polygon.length]))
        .filter((point): point is Point => point !== null)
        .sort((a, b) => fp.sub(dist(boxCorner, a), dist(boxCorner, b)));
    const edgePoint = intersections[0] ?? boxCorner;
    return {
        x: fp.add(edgePoint.x, fp.mul(direction.x, inset)),
        y: fp.add(edgePoint.y, fp.mul(direction.y, inset)),
    };
}

export function boardAnchorPoint(board: CenteredRectBoard, anchorValue: BoardAnchor): Point {
    const box = boardBox(board);
    const center = { x: 0, y: 0 };
    const map: Record<BoardAnchor, Point> = {
        'board.center': center,
        'board.left': { x: box.left, y: 0 },
        'board.right': { x: box.right, y: 0 },
        'board.top': { x: 0, y: box.top },
        'board.bottom': { x: 0, y: box.bottom },
        'board.top_left': { x: box.left, y: box.top },
        'board.top_right': { x: box.right, y: box.top },
        'board.bottom_left': { x: box.left, y: box.bottom },
        'board.bottom_right': { x: box.right, y: box.bottom },
    };
    return map[anchorValue];
}

export function distanceToEdge(board: CenteredRectBoard, component: PcbComponent, placement: Placement, edge: BoardEdge) {
    const box = boardBox(board);
    const componentBox = getBox(component, placement);
    if (edge === 'left') return fp.abs(fp.sub(componentBox.left, box.left));
    if (edge === 'right') return fp.abs(fp.sub(box.right, componentBox.right));
    if (edge === 'top') return fp.abs(fp.sub(componentBox.top, box.top));
    return fp.abs(fp.sub(box.bottom, componentBox.bottom));
}

export function rotatedSize(footprint: FootprintSpec, rotate: number) {
    return Math.abs(rotate % 180) === 90
        ? { width: footprint.height, height: footprint.width }
        : { width: footprint.width, height: footprint.height };
}

export function overlaps(a: Box, b: Box, clearance: number) {
    return boxClearanceGap(a, b) <= fp.sub(clearance, PLACEMENT_EPSILON);
}

export function boxGap(a: Box, b: Box) {
    const xGap = fp.max(0, fp.max(coordinateDifference(b.left, a.right), coordinateDifference(a.left, b.right)));
    const yGap = fp.max(0, fp.max(coordinateDifference(b.top, a.bottom), coordinateDifference(a.top, b.bottom)));
    return fp.sqrt(fp.add(fp.mul(xGap, xGap), fp.mul(yGap, yGap)));
}

/**
 * Separation distance for axis-aligned clearance checks.
 * Two boxes are safely separated iff the result is >= required clearance.
 * A negative value means they overlap (or are closer than touching).
 */
export function boxClearanceGap(a: Box, b: Box): number {
    const xSep = fp.max(coordinateDifference(b.left, a.right), coordinateDifference(a.left, b.right));
    const ySep = fp.max(coordinateDifference(b.top, a.bottom), coordinateDifference(a.top, b.bottom));
    return fp.max(xSep, ySep);
}

export function boxPointGap(box: Box, point: Point) {
    const xGap = fp.max(coordinateDifference(box.left, point.x), 0, coordinateDifference(point.x, box.right));
    const yGap = fp.max(coordinateDifference(box.top, point.y), 0, coordinateDifference(point.y, box.bottom));
    return fp.sqrt(fp.add(fp.mul(xGap, xGap), fp.mul(yGap, yGap)));
}

export function boardHoleKeepoutRadius(hole: BoardHole) {
    return fp.max(hole.keepout, fp.div(hole.diameter, 2), fp.div(hole.drill, 2));
}

export function overlapsBoardHole(box: Box, hole: BoardHole, clearance = 0) {
    return fp.add(boxPointGap(box, hole), GEOMETRY_EPSILON) < fp.add(boardHoleKeepoutRadius(hole), clearance);
}

export function polar(radius: number, angle: number) {
    const [sin, cos] = sinCosDegrees(angle);
    return { x: fp.mul(cos, radius), y: fp.mul(sin, radius) };
}

function pointOnSegment(point: Point, a: Point, b: Point) {
    return fp.abs(fp.sub(fp.mul((fp.sub(b.x, a.x)), (fp.sub(point.y, a.y))), fp.mul((fp.sub(b.y, a.y)), (fp.sub(point.x, a.x))))) < GEOMETRY_EPSILON
        && point.x <= fp.add(fp.max(a.x, b.x), GEOMETRY_EPSILON)
        && point.x >= fp.sub(fp.min(a.x, b.x), GEOMETRY_EPSILON)
        && point.y <= fp.add(fp.max(a.y, b.y), GEOMETRY_EPSILON)
        && point.y >= fp.sub(fp.min(a.y, b.y), GEOMETRY_EPSILON);
}

function segmentIntersection(a: Point, b: Point, c: Point, d: Point): Point | null {
    const ox = coordinateOrigin(a.x), oy = coordinateOrigin(a.y);
    if (ox !== 0 || oy !== 0) {
        const local = (p: Point) => ({ x: fp.f32(p.x - ox), y: fp.f32(p.y - oy) });
        const p = segmentIntersection(local(a), local(b), local(c), local(d));
        return p ? { x: p.x + ox, y: p.y + oy } : null;
    }
    const denominator = fp.sub(fp.mul((fp.sub(a.x, b.x)), (fp.sub(c.y, d.y))), fp.mul((fp.sub(a.y, b.y)), (fp.sub(c.x, d.x))));
    if (fp.abs(denominator) < GEOMETRY_EPSILON) return null;
    const t = fp.div((fp.sub(fp.mul((fp.sub(a.x, c.x)), (fp.sub(c.y, d.y))), fp.mul((fp.sub(a.y, c.y)), (fp.sub(c.x, d.x))))), denominator);
    const u = fp.div(-(fp.sub(fp.mul((fp.sub(a.x, b.x)), (fp.sub(a.y, c.y))), fp.mul((fp.sub(a.y, b.y)), (fp.sub(a.x, c.x))))), denominator);
    if (t < -GEOMETRY_EPSILON || t > fp.add(1, GEOMETRY_EPSILON) || u < -GEOMETRY_EPSILON || u > fp.add(1, GEOMETRY_EPSILON)) return null;
    return {
        x: fp.add(a.x, fp.mul(t, (fp.sub(b.x, a.x)))),
        y: fp.add(a.y, fp.mul(t, (fp.sub(b.y, a.y)))),
    };
}

export function dist(a: Point, b: Point) {
    const dx = coordinateDifference(a.x, b.x);
    const dy = coordinateDifference(a.y, b.y);
    return fp.sqrt(fp.add(fp.mul(dx, dx), fp.mul(dy, dy)));
}

export function round(value: number) {
    return fp.div(Math.round(fp.mul(value, 100)), 100);
}

export function roundPlacement(value: number) {
    return canonicalRoundPlacement(value);
}

export function componentBox(component: PcbComponent, placement: Placement): Box {
    const size = rotatedSize(component.footprint, placement.rotate);
    return {
        left: sourceCoordinateAdd(placement.x, -fp.div(size.width, 2)),
        right: sourceCoordinateAdd(placement.x, fp.div(size.width, 2)),
        top: sourceCoordinateAdd(placement.y, -fp.div(size.height, 2)),
        bottom: sourceCoordinateAdd(placement.y, fp.div(size.height, 2)),
    };
}

export function componentCollisionBoxes(component: PcbComponent, placement: Placement, layer: Layer): Box[] {
    const sameSide = placement.layer === layer;
    const override = component.pcb.occupiedAreas?.[sameSide ? 'top' : 'bottom'];
    const body = override !== undefined
        ? override.map((box) => localBoxWorld(placement, box))
        : sameSide ? [component.footprint.bodyBox
            ? localBoxWorld(placement, component.footprint.bodyBox) : componentBox(component, placement)] : [];
    const pads = component.footprint.pads
        .filter((pad) => isThroughHolePad(pad)
            || (sameSide ? pad.layer !== 'bottom' : pad.layer === 'bottom'))
        .map((pad) => componentPadBox(placement, pad));
    const reverseSilk = sameSide || override !== undefined ? [] : (component.footprint.graphics ?? [])
        .filter((graphic) => graphic.layer === 'silk' && graphic.side === 'bottom')
        .map((graphic) => graphicBoxWorld(placement, graphic));
    const oppositeLayerPolygons = sameSide ? [] : (component.pcb.generatedGeometry ?? [])
        .flatMap((geometry) => geometry.polygons)
        .filter((polygon) => polygon.layer === 'opposite')
        .map((polygon) => pointsBox(polygon.points.map((point) => getLocalPointWorld(placement, point))));
    return [...body, ...pads, ...reverseSilk, ...oppositeLayerPolygons];
}

export function componentBodyBox(component: PcbComponent, placement: Placement): Box {
    const override = component.pcb.occupiedAreas?.top;
    if (override !== undefined) {
        const ownPads = component.footprint.pads.filter((pad) => isThroughHolePad(pad) || pad.layer !== 'bottom')
            .map((pad) => componentPadBox(placement, pad));
        const boxes = [...override.map((box) => localBoxWorld(placement, box)), ...ownPads];
        return boxes.length ? unionBoxes(boxes)
            : { left: placement.x, right: placement.x, top: placement.y, bottom: placement.y };
    }
    return component.footprint.bodyBox
        ? localBoxWorld(placement, component.footprint.bodyBox)
        : componentBox(component, placement);
}

function localBoxWorld(placement: Placement, box: Box): Box {
    return pointsBox([
        { x: box.left, y: box.top }, { x: box.right, y: box.top },
        { x: box.right, y: box.bottom }, { x: box.left, y: box.bottom },
    ].map((point) => getLocalPointWorld(placement, point)));
}

function graphicBoxWorld(placement: Placement, graphic: NonNullable<PcbComponent['footprint']['graphics']>[number]): Box {
    if (graphic.kind === 'circle') {
        return localBoxWorld(placement, { left: fp.sub(graphic.x, graphic.radius), right: fp.add(graphic.x, graphic.radius),
            top: fp.sub(graphic.y, graphic.radius), bottom: fp.add(graphic.y, graphic.radius) });
    }
    return pointsBox(graphic.points.map((point) => getLocalPointWorld(placement, point)));
}

export function componentPairCollisionBoxPairs(
    a: PcbComponent,
    aPlacement: Placement,
    b: PcbComponent,
    bPlacement: Placement,
): Array<{ a: Box; b: Box }> {
    const layers: Layer[] = ['top', 'bottom'];
    const pairs: Array<{ a: Box; b: Box }> = [];
    for (const layer of layers) {
        const aBoxes = componentCollisionBoxes(a, aPlacement, layer);
        if (aBoxes.length === 0) continue;
        const bBoxes = componentCollisionBoxes(b, bPlacement, layer);
        if (bBoxes.length === 0) continue;
        for (const aBox of aBoxes) {
            for (const bBox of bBoxes) pairs.push({ a: aBox, b: bBox });
        }
    }
    return pairs;
}

export function componentPadBox(placement: Placement, pad: FootprintPad): Box {
    const width = fp.max(pad.width, pad.drillDiameter ?? 0);
    const height = fp.max(pad.height, pad.drillDiameter ?? 0);
    const corners = [
        { x: fp.sub(pad.x, fp.div(width, 2)), y: fp.sub(pad.y, fp.div(height, 2)) },
        { x: fp.add(pad.x, fp.div(width, 2)), y: fp.sub(pad.y, fp.div(height, 2)) },
        { x: fp.add(pad.x, fp.div(width, 2)), y: fp.add(pad.y, fp.div(height, 2)) },
        { x: fp.sub(pad.x, fp.div(width, 2)), y: fp.add(pad.y, fp.div(height, 2)) },
    ].map((corner) => getLocalPointWorld(placement, corner));
    return pointsBox(corners);
}

export function isThroughHolePad(pad: FootprintPad) {
    return pad.mount === 'through_hole' || (pad.drillDiameter ?? 0) > 0;
}

export function translateBox(box: Box, dx: number, dy: number): Box {
    return {
        left: coordinateRound(coordinateAdd(box.left, dx)),
        right: coordinateRound(coordinateAdd(box.right, dx)),
        top: coordinateRound(coordinateAdd(box.top, dy)),
        bottom: coordinateRound(coordinateAdd(box.bottom, dy)),
    };
}

export function rotatePointAround(point: Point, origin: Point, angle: number): Point {
    const [sin, cos] = sinCosDegrees(angle);
    const dx = coordinateDifference(point.x, origin.x);
    const dy = coordinateDifference(point.y, origin.y);
    switch (((angle % 360) + 360) % 360) {
        case 0: return {x:coordinateRound(point.x),y:coordinateRound(point.y)};
        case 90: return {x:coordinateRound(coordinateAdd(origin.x,-dy)),y:coordinateRound(coordinateAdd(origin.y,dx))};
        case 180: return {x:coordinateRound(coordinateAdd(origin.x,-dx)),y:coordinateRound(coordinateAdd(origin.y,-dy))};
        case 270: return {x:coordinateRound(coordinateAdd(origin.x,dy)),y:coordinateRound(coordinateAdd(origin.y,-dx))};
    }
    return {
        x: coordinateRound(coordinateAdd(coordinateAdd(origin.x, fp.mul(dx, cos)), -fp.mul(dy, sin))),
        y: coordinateRound(coordinateAdd(coordinateAdd(origin.y, fp.mul(dx, sin)), fp.mul(dy, cos))),
    };
}

export function rotateBox(box: Box, origin: Point, angle: number): Box {
    let left = Infinity;
    let right = -Infinity;
    let top = Infinity;
    let bottom = -Infinity;
    const visit = (x: number, y: number) => {
        const rotated=rotatePointAround({x,y},origin,angle);
        const rotatedX=rotated.x;
        const rotatedY=rotated.y;
        if (rotatedX < left) left = rotatedX;
        if (rotatedX > right) right = rotatedX;
        if (rotatedY < top) top = rotatedY;
        if (rotatedY > bottom) bottom = rotatedY;
    };
    visit(box.left, box.top);
    visit(box.right, box.top);
    visit(box.right, box.bottom);
    visit(box.left, box.bottom);
    return { left, right, top, bottom };
}

export function boxCenter(box: Box): Point {
    return { x: coordinateRound(coordinateCenter(box.left, box.right)), y: coordinateRound(coordinateCenter(box.top, box.bottom)) };
}

export function unionBoxes(boxes: Box[]): Box {
    if (boxes.length === 0) return { left: 0, right: 0, top: 0, bottom: 0 };
    let left = boxes[0].left;
    let right = boxes[0].right;
    let top = boxes[0].top;
    let bottom = boxes[0].bottom;
    for (let index = 1; index < boxes.length; index += 1) {
        const box = boxes[index];
        if (box.left < left) left = box.left;
        if (box.right > right) right = box.right;
        if (box.top < top) top = box.top;
        if (box.bottom > bottom) bottom = box.bottom;
    }
    return { left, right, top, bottom };
}

export function normalizeRotation(value: number) {
    return ((Math.round(value) % 360) + 360) % 360;
}

/** Absolute copper side; undefined means a pad occupies both sides. */
export function padPlacementLayer(pad: FootprintPad, placement: Placement): Layer | undefined {
    return isThroughHolePad(pad) ? undefined : pad.layer === 'bottom'
        ? (placement.layer === 'top' ? 'bottom' : 'top') : placement.layer;
}
