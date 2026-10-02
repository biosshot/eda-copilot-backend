import { getEasyEdaDataStr } from "#devices/easy-eda-datastr.ts";
import { getEasyEdaDevice } from "#devices/easy-eda.ts";
import type { FootprintGraphic, FootprintGraphicLayer, FootprintPad, FootprintSpec, Point } from "#types/pcb/layout-model.ts";
import { fetchWithRetry } from "#utils/fetch-with-retry.ts";
import { memoize } from "#utils/memoize.ts";
import { getPartLibraryUuid, type PartUuid } from "#types/lcsc.ts";
import * as fp from '#pcb-layout/f32.ts';
import { getEdaApiBase, type EdaEdition } from '../eda-api.ts';

const MIL_PER_MM = 39.37007874015748;
const PAD_ONLY_FOOTPRINT_MARGIN_MM = 0.254;

type EasyEdaFootprintDocument = {
    uuid: string;
    title?: string;
    display_title?: string;
    dataStr?: string;
    dataStrId?: string;
    key?: string;
    iv?: string;
};

type Box = {
    left: number;
    right: number;
    top: number;
    bottom: number;
};

export const getEasyEdaFootprintDocument = memoize(async (footprintUuid: string, libraryUuid: string = 'lcsc', edaEdition: EdaEdition = 'easyeda'): Promise<EasyEdaFootprintDocument> => {
    const response = await fetchWithRetry(`${getEdaApiBase(edaEdition)}/api/v2/components/${encodeURIComponent(footprintUuid)}?uuid=${encodeURIComponent(footprintUuid)}&path=${encodeURIComponent(libraryUuid)}`);
    if (!response.ok) {
        throw new Error(`Failed to fetch EasyEDA footprint ${footprintUuid}: ${response.status}`);
    }

    const json = await response.json() as { success?: boolean; msg?: string; result?: EasyEdaFootprintDocument };
    if (!json.success || !json.result) {
        throw new Error(`Failed to fetch EasyEDA footprint ${footprintUuid}: ${json.msg ?? "unknown error"}`);
    }

    return json.result;
});

export const resolveEasyEdaFootprintByUuid = memoize(async (footprintUuid: string, libraryUuid: string = 'lcsc', edaEdition: EdaEdition = 'easyeda'): Promise<FootprintSpec> => {
    const document = await getEasyEdaFootprintDocument(footprintUuid, libraryUuid, edaEdition);
    const dataStr = await getEasyEdaDataStr(document);
    if (!dataStr) {
        throw new Error(`EasyEDA footprint ${footprintUuid} has no dataStr`);
    }

    return parseEasyEdaFootprintDataStr(dataStr, document.display_title ?? document.title ?? footprintUuid);
});

export const resolveEasyEdaFootprintByPartUuid = memoize(async (partUuid: PartUuid, edaEdition: EdaEdition = 'easyeda'): Promise<FootprintSpec | null> => {
    const device = await getEasyEdaDevice(partUuid, edaEdition);
    const footprintUuid = device.footprint?.uuid;
    if (!footprintUuid) return null;

    return resolveEasyEdaFootprintByUuid(footprintUuid, getPartLibraryUuid(partUuid), edaEdition);
});

export function parseEasyEdaFootprintDataStr(dataStr: string, fallbackName = "EASYEDA_FOOTPRINT"): FootprintSpec {
    const pads: Array<FootprintPad & { box: Box }> = [];
    const bodyBoxes: Box[] = [];
    const fallbackBodyBoxes: Box[] = [];
    const silkscreenBoxes: Box[] = [];
    const topSilkscreenBoxes: Box[] = [];
    const rawGraphics: Array<FootprintGraphic & { box: Box }> = [];
    let name = fallbackName;
    let mechanicalHoleIndex = 1;
    let footprintViaIndex = 1;

    for (const line of dataStr.split(/\r?\n/)) {
        const trimmed = line.trim();
        if (!trimmed) continue;

        let item: unknown[];
        try {
            item = JSON.parse(trimmed);
        } catch {
            continue;
        }

        if (item[0] === "DOCTYPE" && item[1] !== "FOOTPRINT") {
            throw new Error(`Expected EasyEDA FOOTPRINT dataStr, got ${String(item[1])}`);
        }

        if (item[0] === "ATTR" && item[3] === "Name" && typeof item[4] === "string") {
            name = item[4];
            continue;
        }

        if (item[0] === "PAD") {
            const pad = parsePad(item);
            if (pad) {
                pads.push(pad);
            }
            continue;
        }

        const footprintVia = parseFootprintVia(item, footprintViaIndex);
        if (footprintVia) {
            pads.push(footprintVia);
            footprintViaIndex += 1;
            continue;
        }

        const mechanicalHole = parseMechanicalHole(item, mechanicalHoleIndex);
        if (mechanicalHole) {
            pads.push(mechanicalHole);
            mechanicalHoleIndex += 1;
            continue;
        }

        const graphic = parseGraphic(item);
        if (graphic && !isIgnorableFootprintGraphic(graphic)) {
            rawGraphics.push(graphic);
            if (graphic.layer === "silk") {
                silkscreenBoxes.push(graphic.box);
                if (graphic.side === 'top') topSilkscreenBoxes.push(graphic.box);
            }
        }

        const box = bodyGeometryBox(item);
        if (box) bodyBoxes.push(box);
        const fallbackBox = fallbackBodyGeometryBox(item);
        if (fallbackBox) fallbackBodyBoxes.push(fallbackBox);
    }

    if (pads.length === 0) {
        throw new Error(`EasyEDA footprint ${name} has no PAD entries`);
    }

    const bodySourceBoxes = bodyBoxes.length > 0 ? bodyBoxes : fallbackBodyBoxes;
    const bodyBox = bodySourceBoxes.length > 0 ? mergeBoxes(bodySourceBoxes) : null;
    const padBoxes = pads.map((pad) => pad.box);
    const paddedPadBox = inflateBox(mergeBoxes(padBoxes), PAD_ONLY_FOOTPRINT_MARGIN_MM);
    const physicalBaseBox = bodyBox ? mergeBoxes([bodyBox, paddedPadBox]) : paddedPadBox;
    const physicalSilkscreenBoxes = bodyBox
        ? silkscreenBoxes.filter((box) => isPhysicalSilkscreenBox(box, physicalBaseBox))
        : silkscreenBoxes;
    const physicalTopSilkscreenBoxes = physicalSilkscreenBoxes.filter((box) => topSilkscreenBoxes.includes(box));
    const visualBoxes = [physicalBaseBox, ...physicalSilkscreenBoxes];
    const visualBox = visualBoxes.length > 0 ? mergeBoxes(visualBoxes) : null;
    const footprintBox = visualBox && boxContains(visualBox, paddedPadBox)
        ? visualBox
        : mergeBoxes([...(visualBox ? [visualBox] : []), paddedPadBox]);
    const center = {
        x: (footprintBox.left + footprintBox.right) / 2,
        y: (footprintBox.top + footprintBox.bottom) / 2,
    };
    const hasBottomSilk = physicalSilkscreenBoxes.some((box) => !topSilkscreenBoxes.includes(box));
    const occupiedTopBoxes = bodyBoxes.length > 0 && (physicalTopSilkscreenBoxes.length > 0 || !hasBottomSilk)
        ? [...bodyBoxes, ...physicalTopSilkscreenBoxes]
        : physicalTopSilkscreenBoxes;
    const topPadBoxes = pads.filter((pad) => pad.layer !== 'bottom').map((pad) => pad.box);
    const inferredBodyBox = occupiedTopBoxes.length > 0
        ? mergeBoxes([...occupiedTopBoxes, ...topPadBoxes])
        : null;

    return {
        name,
        width: roundMm(footprintBox.right - footprintBox.left),
        height: roundMm(footprintBox.bottom - footprintBox.top),
        pads: pads.map(({ box: _box, ...pad }) => ({
            ...pad,
            x: roundMm(center.x - pad.x),
            y: roundMm(pad.y - center.y),
        })),
        graphics: rawGraphics.map(({ box: _box, ...graphic }) => normalizeGraphic(graphic, center)),
        ...(inferredBodyBox ? { bodyBox: normalizeBox(inferredBodyBox, center) } : {}),
        ...(inferredBodyBox ? { bodyBoxSource: occupiedTopBoxes.length
            ? bodyBoxes.length > 0 && (physicalTopSilkscreenBoxes.length > 0 || !hasBottomSilk) ? 'body' as const : 'silk' as const
            : 'pads' as const } : {}),
        sourceOriginOffset: {
            x: roundMm(center.x),
            y: roundMm(-center.y),
        },
    };
}

function parseFootprintVia(item: unknown[], index: number): (FootprintPad & { box: Box }) | null {
    if (item[0] !== "VIA") return null;

    const candidates = [
        viaCandidate(item, 4, 5, 6, 7),
        viaCandidate(item, 5, 6, 7, 8),
        viaCandidate(item, 5, 6, 8, 7),
        viaCandidate(item, 2, 3, 4, 5),
    ].filter((candidate): candidate is { x: number; y: number; diameter: number; drill: number } => candidate !== null);
    const via = candidates[0];
    if (!via) return null;

    const center = { x: milToMm(via.x), y: milToMm(via.y) };
    const diameter = roundMm(milToMm(via.diameter));
    const drill = roundMm(milToMm(via.drill));
    const radius = diameter / 2;

    return {
        pin_number: `FV${index}`,
        name: `FV${index}`,
        x: center.x,
        y: center.y,
        width: diameter,
        height: diameter,
        mount: "through_hole",
        layer: 'multi',
        drillDiameter: drill,
        box: {
            left: center.x - radius,
            right: center.x + radius,
            top: center.y - radius,
            bottom: center.y + radius,
        },
    };
}

function viaCandidate(item: unknown[], xIndex: number, yIndex: number, diameterIndex: number, drillIndex: number) {
    const x = numberAt(item, xIndex);
    const y = numberAt(item, yIndex);
    const diameter = numberAt(item, diameterIndex);
    const drill = numberAt(item, drillIndex);
    if (x === null || y === null || diameter === null || drill === null) return null;
    if (diameter <= 0 || drill <= 0 || diameter < drill) return null;
    return { x, y, diameter, drill };
}

function parseMechanicalHole(item: unknown[], index: number): (FootprintPad & { box: Box }) | null {
    if (item[0] !== "FILL" && item[0] !== "POLY") return null;
    const layer = numberAt(item, 4);
    if (layer !== 12) return null;

    const geometry = item[0] === "FILL" ? item[7] : item[6];
    const circle = singleCircle(geometry);
    if (!circle) return null;

    const center = { x: milToMm(circle.x), y: milToMm(circle.y) };
    const radius = milToMm(circle.radius);
    const diameter = roundMm(radius * 2);
    const box = {
        left: center.x - radius,
        right: center.x + radius,
        top: center.y - radius,
        bottom: center.y + radius,
    };

    return {
        pin_number: `MH${index}`,
        name: `MH${index}`,
        x: center.x,
        y: center.y,
        width: diameter,
        height: diameter,
        mount: "through_hole",
        layer: 'multi',
        drillDiameter: diameter,
        box,
    };
}

function parsePad(item: unknown[]): (FootprintPad & { box: Box }) | null {
    const pinNumber = item[5];
    const x = numberAt(item, 6);
    const y = numberAt(item, 7);
    const rotation = numberAt(item, 8) ?? 0;
    const layer = numberAt(item, 4);
    const drillShape = Array.isArray(item[9]) ? item[9] : null;
    const shape = Array.isArray(item[10]) ? item[10] : null;
    if ((typeof pinNumber !== "string" && typeof pinNumber !== "number") || x === null || y === null || !shape) {
        return null;
    }

    const center = { x: milToMm(x), y: milToMm(y) };
    const localBox = padShapeLocalBox(shape);
    if (!localBox) return null;

    // POLY pad shapes encode polygon points in absolute footprint coordinates,
    // not relative to the pad center. Convert to a local box before rotating.
    const shapeType = String(shape[0] ?? "").toUpperCase();
    const adjustedLocalBox = shapeType === "POLY"
        ? {
            left: localBox.left - center.x,
            right: localBox.right - center.x,
            top: localBox.top - center.y,
            bottom: localBox.bottom - center.y,
        }
        : localBox;

    const box = rotateLocalBox(center, adjustedLocalBox, rotation);
    const pad: FootprintPad = {
        pin_number: pinNumber,
        name: String(pinNumber),
        x: (box.left + box.right) / 2,
        y: (box.top + box.bottom) / 2,
        width: roundMm(box.right - box.left),
        height: roundMm(box.bottom - box.top),
    };
    const drillDiameter = drillShapeDiameter(drillShape);
    const isThroughHole = layer === 12 || drillDiameter !== null;

    return {
        ...pad,
        mount: isThroughHole ? "through_hole" : "smd",
        ...(layer === 1 || layer === 2 || isThroughHole ? { layer: isThroughHole ? 'multi' as const : layer === 2 ? 'bottom' as const : 'top' as const } : {}),
        ...(drillDiameter !== null ? { drillDiameter: roundMm(drillDiameter) } : {}),
        box,
    };
}

function drillShapeDiameter(shape: unknown[] | null): number | null {
    if (!shape) return null;
    const type = String(shape[0] ?? "").toUpperCase();
    if (type !== "ROUND" && type !== "CIRCLE" && type !== "OVAL" && type !== "ELLIPSE") return null;

    const width = numberAt(shape, 1);
    const height = numberAt(shape, 2) ?? width;
    if (width === null || height === null) return null;

    return Math.max(milToMm(width), milToMm(height));
}

function rotateLocalBox(center: Point, localBox: Box, rotation: number): Box {
    const radians = rotation * Math.PI / 180;
    const cos = Math.cos(radians);
    const sin = Math.sin(radians);
    const corners = [
        { x: localBox.left, y: localBox.top },
        { x: localBox.right, y: localBox.top },
        { x: localBox.right, y: localBox.bottom },
        { x: localBox.left, y: localBox.bottom },
    ].map((corner) => ({
        x: center.x + corner.x * cos - corner.y * sin,
        y: center.y + corner.x * sin + corner.y * cos,
    }));

    return pointsToBox(corners);
}

function padShapeLocalBox(shape: unknown[]): Box | null {
    const type = String(shape[0] ?? "").toUpperCase();
    if (type === "RECT" || type === "OVAL" || type === "ELLIPSE" || type === "ROUND") {
        const width = numberAt(shape, 1);
        const height = numberAt(shape, 2);
        return width !== null && height !== null ? centeredMilBox(width, height) : null;
    }

    if (type === "CIRCLE") {
        const diameter = numberAt(shape, 1) ?? numberAt(shape, 3);
        return diameter !== null ? centeredMilBox(diameter, diameter) : null;
    }

    if (type === "NGON") {
        const diameter = numberAt(shape, 1);
        const sides = numberAt(shape, 2);
        if (diameter === null || sides === null || !Number.isInteger(sides) || sides < 3) return null;
        // The placement model stores rectangular pad envelopes.
        return centeredMilBox(diameter, diameter);
    }

    if (type === "POLY") {
        const geometry = Array.isArray(shape[1]) ? shape[1] : shape;
        const points = collectGeometryPoints(geometry);
        return points.length > 0 ? pointsToBox(points.map((point) => ({ x: milToMm(point.x), y: milToMm(point.y) }))) : null;
    }

    return null;
}

function centeredMilBox(width: number, height: number): Box {
    const halfWidth = milToMm(width) / 2;
    const halfHeight = milToMm(height) / 2;
    return {
        left: -halfWidth,
        right: halfWidth,
        top: -halfHeight,
        bottom: halfHeight,
    };
}

function geometryBox(item: unknown[]): Box | null {
    const type = item[0];
    if (type !== "FILL" && type !== "POLY" && type !== "RECT" && type !== "CIRCLE" && type !== "ARC") {
        return null;
    }

    const geometry = type === "FILL" ? item[7] : type === "POLY" ? item[6] : item;
    const points = collectGeometryPoints(geometry);
    const boxes = collectShapeBoxes(geometry);
    if (points.length === 0 && boxes.length === 0) return null;

    return mergeBoxes([
        ...(points.length > 0 ? [pointsToBox(points.map((point) => ({ x: milToMm(point.x), y: milToMm(point.y) })))] : []),
        ...boxes,
    ]);
}

function bodyGeometryBox(item: unknown[]): Box | null {
    if (item[0] !== "FILL" && item[0] !== "POLY") return null;
    const layer = numberAt(item, 4);
    if (layer !== 48) return null;

    return geometryBox(item);
}

function fallbackBodyGeometryBox(item: unknown[]): Box | null {
    if (item[0] !== "FILL" && item[0] !== "POLY") return null;
    const layer = numberAt(item, 4);
    if (layer !== 3) return null;
    const graphic = parseGraphic(item);
    if (graphic && isIgnorableFootprintGraphic(graphic)) return null;

    return geometryBox(item);
}

function isIgnorableFootprintGraphic(graphic: FootprintGraphic & { box: Box }) {
    if (graphic.kind !== "path") return false;
    if (!graphic.closed || graphic.strokeWidth > 0.01 || graphic.points.length > 8) return false;
    return uniquePointCount(graphic.points) <= 3 && polygonArea(graphic.points) > 1;
}

function parseGraphic(item: unknown[]): (FootprintGraphic & { box: Box }) | null {
    if (item[0] !== "FILL" && item[0] !== "POLY") return null;

    const layerNumber = numberAt(item, 4);
    const layer = graphicLayer(layerNumber);
    if (!layer) return null;

    const geometry = item[0] === "FILL" ? item[7] : item[6];
    const strokeWidth = roundMm(milToMm(numberAt(item, 5) ?? 1));
    const circle = singleCircle(geometry);
    if (circle) {
        const box = {
            left: milToMm(circle.x - circle.radius),
            right: milToMm(circle.x + circle.radius),
            top: milToMm(circle.y - circle.radius),
            bottom: milToMm(circle.y + circle.radius),
        };
        return {
            kind: "circle",
            layer,
            ...(graphicSide(layerNumber) ? { side: graphicSide(layerNumber) } : {}),
            x: milToMm(circle.x),
            y: milToMm(circle.y),
            radius: milToMm(circle.radius),
            strokeWidth,
            box,
        };
    }

    const points = collectGeometryPoints(geometry).map((point) => ({ x: milToMm(point.x), y: milToMm(point.y) }));
    if (points.length < 2) return null;

    return {
        kind: "path",
        layer,
        ...(graphicSide(layerNumber) ? { side: graphicSide(layerNumber) } : {}),
        points,
        closed: isClosedPath(points),
        strokeWidth,
        box: pointsToBox(points),
    };
}

function graphicLayer(layer: number | null): FootprintGraphicLayer | null {
    if (layer === 3 || layer === 4) return "silk";
    if (layer === 48) return "body";
    if (layer === 49) return "marking";
    if (layer === 13) return "document";
    return null;
}

function graphicSide(layer: number | null): 'top' | 'bottom' | undefined {
    return layer === 3 ? 'top' : layer === 4 ? 'bottom' : undefined;
}

function normalizeBox(box: Box, center: Point): Box {
    return {
        left: roundMm(center.x - box.right), right: roundMm(center.x - box.left),
        top: roundMm(box.top - center.y), bottom: roundMm(box.bottom - center.y),
    };
}

function singleCircle(value: unknown): { x: number; y: number; radius: number } | null {
    if (!Array.isArray(value)) return null;
    if (Array.isArray(value[0]) && value.length !== 1) return null;
    const circle = Array.isArray(value[0]) ? value[0] : value;
    if (!Array.isArray(circle) || String(circle[0]).toUpperCase() !== "CIRCLE") return null;

    const x = numberAt(circle, 1);
    const y = numberAt(circle, 2);
    const radius = numberAt(circle, 3);
    return x !== null && y !== null && radius !== null ? { x, y, radius } : null;
}

function normalizeGraphic(graphic: FootprintGraphic, center: Point): FootprintGraphic {
    if (graphic.kind === "circle") {
        return {
            ...graphic,
            x: roundMm(center.x - graphic.x),
            y: roundMm(graphic.y - center.y),
            radius: roundMm(graphic.radius),
        };
    }

    return {
        ...graphic,
        points: graphic.points.map((point) => ({
            x: roundMm(center.x - point.x),
            y: roundMm(point.y - center.y),
        })),
    };
}

function isClosedPath(points: Point[]) {
    if (points.length < 3) return false;
    const first = points[0];
    const last = points[points.length - 1];
    return Math.abs(first.x - last.x) < 0.001 && Math.abs(first.y - last.y) < 0.001;
}

function polygonArea(points: Point[]) {
    if (points.length < 3) return 0;
    let area = 0;
    for (let index = 0; index < points.length; index++) {
        const current = points[index];
        const next = points[(index + 1) % points.length];
        area += current.x * next.y - next.x * current.y;
    }
    return Math.abs(area) / 2;
}

function uniquePointCount(points: Point[]) {
    const unique = new Set(points.map((point) => `${point.x.toFixed(4)},${point.y.toFixed(4)}`));
    return unique.size;
}

function collectGeometryPoints(value: unknown): Point[] {
    if (!Array.isArray(value)) return [];
    const shape = typeof value[0] === 'string' ? value[0].toUpperCase() : '';
    if (shape === 'R') return rectanglePoints(value);
    if (shape === 'CIRCLE') {
        const circle = singleCircle(value);
        return circle ? Array.from({ length: 17 }, (_, i) => ({
            x: circle.x + fp.mul(circle.radius, fp.cos(fp.mul(i, Math.PI / 8))),
            y: circle.y + fp.mul(circle.radius, fp.sin(fp.mul(i, Math.PI / 8))),
        })) : [];
    }
    // Named shapes have dimensions/angles mixed with coordinates. Their bounds
    // are handled by collectShapeBoxes or a dedicated decoder, never by pairs.
    if (['CIRCLE', 'ELLIPSE', 'ROUND', 'OVAL', 'RECT', 'ARC'].includes(shape)) return [];
    if (isEasyEdaPath(value)) {
        return collectPathPoints(value);
    }
    if (value.every((item) => typeof item === 'number' && Number.isFinite(item))) {
        const points: Point[] = [];
        for (let index = 0; index + 1 < value.length; index += 2) {
            points.push({ x: value[index] as number, y: value[index + 1] as number });
        }
        return points;
    }
    return value.flatMap(collectGeometryPoints);
}

function rectanglePoints(value: unknown[]): Point[] {
    const [x, y, width, height, rotation] = [1, 2, 3, 4, 5].map((index) => numberAt(value, index));
    if (x === null || y === null || width === null || height === null) return [];
    const cx = x + width / 2;
    const cy = y - height / 2;
    const radians = (rotation ?? 0) * Math.PI / 180;
    const cos = Math.cos(radians), sin = Math.sin(radians);
    return [[x, y], [x + width, y], [x + width, y - height], [x, y - height], [x, y]]
        .map(([px, py]) => ({ x: cx + (px - cx) * cos - (py - cy) * sin,
            y: cy + (px - cx) * sin + (py - cy) * cos }));
}

function isEasyEdaPath(value: unknown): value is unknown[] {
    return Array.isArray(value) && value.some((item) => ['L', 'ARC', 'CARC', 'C', 'Q'].includes(String(item)));
}

function collectPathPoints(path: unknown[]): Point[] {
    const points: Point[] = [];
    let mode = 'L';

    for (let index = 0; index < path.length;) {
        const token = path[index];
        if (token === "L" || token === "C" || token === "Q") {
            mode = token;
            index++;
            continue;
        }

        if (token === "ARC" || token === "CARC") {
            const angle = numberAt(path, index + 1);
            const x = numberAt(path, index + 2);
            const y = numberAt(path, index + 3);
            if (x !== null && y !== null) {
                const end = { x, y };
                const start = points.at(-1);
                if (start && angle !== null) points.push(...arcPoints(start, end, angle));
                else points.push(end);
            }
            index += 4;
            mode = 'L';
            continue;
        }

        if ((mode === 'C' || mode === 'Q') && points.length) {
            const count = mode === 'C' ? 3 : 2;
            const controls: Point[] = [points.at(-1)!];
            for (let offset = 0; offset < count; offset++) {
                const x = numberAt(path, index + offset * 2), y = numberAt(path, index + offset * 2 + 1);
                if (x === null || y === null) break;
                controls.push({ x, y });
            }
            if (controls.length === count + 1) {
                points.push(...bezierPoints(controls));
                index += count * 2;
                continue;
            }
        }

        const x = numberAt(path, index);
        const y = numberAt(path, index + 1);
        if (x !== null && y !== null) {
            points.push({ x, y });
            index += 2;
            continue;
        }

        index++;
    }

    return points;
}

function arcPoints(start: Point, end: Point, angle: number): Point[] {
    if (angle === 0 || (start.x === end.x && start.y === end.y)) return [end];
    // Decode in a local F32 frame, restoring source coordinates only at output.
    const sweep = fp.mul(angle, Math.PI / 180);
    const dx = fp.f32(end.x - start.x), dy = fp.f32(end.y - start.y);
    const half = fp.div(sweep, 2);
    const offset = fp.div(fp.cos(half), fp.mul(2, fp.sin(half)));
    const cx = fp.sub(fp.div(dx, 2), fp.mul(dy, offset));
    const cy = fp.add(fp.div(dy, 2), fp.mul(dx, offset));
    const radius = fp.hypot(cx, cy);
    const initial = fp.atan2(-cy, -cx);
    const times = new Set<number>();
    // Intermediate points render the curve; cardinal extrema make its bbox exact.
    const steps = Math.ceil(Math.abs(angle) / 15);
    for (let step = 1; step < steps; step++) times.add(fp.div(step, steps));
    for (let axis = 0; axis < 4; axis++) {
        const delta = fp.mod(fp.add(fp.mod(fp.mul(Math.sign(sweep), fp.sub(fp.mul(axis, Math.PI / 2), initial)), 2 * Math.PI), 2 * Math.PI), 2 * Math.PI);
        const t = fp.div(delta, fp.abs(sweep));
        if (t > 0 && t < 1) times.add(t);
    }
    return [...[...times].sort((a, b) => a - b).map(t => ({
        x: start.x + fp.add(cx, fp.mul(radius, fp.cos(fp.add(initial, fp.mul(sweep, t))))),
        y: start.y + fp.add(cy, fp.mul(radius, fp.sin(fp.add(initial, fp.mul(sweep, t))))),
    })), end];
}

function bezierPoints(controls: Point[]): Point[] {
    const origin = controls[0];
    const end = controls.at(-1)!;
    controls = controls.map(point => ({ x: fp.f32(point.x - origin.x), y: fp.f32(point.y - origin.y) }));
    const times = new Set<number>();
    for (let step = 1; step < 16; step++) times.add(fp.div(step, 16));
    for (const axis of ['x', 'y'] as const) {
        const [p0, p1, p2, p3] = controls.map(point => point[axis]);
        const a = controls.length === 4 ? fp.add(fp.sub(fp.add(-p0, fp.mul(3, p1)), fp.mul(3, p2)), p3) : 0;
        const b = fp.mul(controls.length === 4 ? 2 : 1, fp.add(fp.sub(p0, fp.mul(2, p1)), p2));
        const c = fp.sub(p1, p0);
        const discriminant = fp.sub(fp.mul(b, b), fp.mul(fp.mul(4, a), c));
        const roots = a === 0 ? (b === 0 ? [] : [fp.div(-c, b)])
            : discriminant < 0 ? [] : [fp.div(fp.add(-b, fp.sqrt(discriminant)), fp.mul(2, a)), fp.div(fp.sub(-b, fp.sqrt(discriminant)), fp.mul(2, a))];
        for (const t of roots) if (t > 0 && t < 1) times.add(t);
    }
    return [...[...times].sort((a, b) => a - b).map(t => {
        let level = controls;
        while (level.length > 1) level = level.slice(1).map((point, i) => ({
            x: fp.add(fp.mul(fp.sub(1, t), level[i].x), fp.mul(t, point.x)),
            y: fp.add(fp.mul(fp.sub(1, t), level[i].y), fp.mul(t, point.y)),
        }));
        return { x: origin.x + level[0].x, y: origin.y + level[0].y };
    }), end];
}

function collectShapeBoxes(value: unknown): Box[] {
    if (!Array.isArray(value)) return [];

    const type = String(value[0] ?? "").toUpperCase();
    if (type === "CIRCLE") {
        const x = numberAt(value, 1);
        const y = numberAt(value, 2);
        const radius = numberAt(value, 3);
        if (x !== null && y !== null && radius !== null) {
            return [{
                left: milToMm(x - radius),
                right: milToMm(x + radius),
                top: milToMm(y - radius),
                bottom: milToMm(y + radius),
            }];
        }
    }

    return value.flatMap((item) => collectShapeBoxes(item));
}

function pointsToBox(points: Point[]): Box {
    return {
        left: Math.min(...points.map((point) => point.x)),
        right: Math.max(...points.map((point) => point.x)),
        top: Math.min(...points.map((point) => point.y)),
        bottom: Math.max(...points.map((point) => point.y)),
    };
}

function mergeBoxes(boxes: Box[]): Box {
    return {
        left: Math.min(...boxes.map((box) => box.left)),
        right: Math.max(...boxes.map((box) => box.right)),
        top: Math.min(...boxes.map((box) => box.top)),
        bottom: Math.max(...boxes.map((box) => box.bottom)),
    };
}

function inflateBox(box: Box, margin: number): Box {
    return {
        left: box.left - margin,
        right: box.right + margin,
        top: box.top - margin,
        bottom: box.bottom + margin,
    };
}

function boxContains(outer: Box, inner: Box) {
    return inner.left >= outer.left
        && inner.right <= outer.right
        && inner.top >= outer.top
        && inner.bottom <= outer.bottom;
}

function isPhysicalSilkscreenBox(box: Box, physicalBaseBox: Box) {
    const nearBody = boxesOverlapOrTouch(inflateBox(physicalBaseBox, 0.8), box);
    if (!nearBody) return false;

    const merged = mergeBoxes([physicalBaseBox, box]);
    const baseWidth = physicalBaseBox.right - physicalBaseBox.left;
    const baseHeight = physicalBaseBox.bottom - physicalBaseBox.top;
    const mergedWidth = merged.right - merged.left;
    const mergedHeight = merged.bottom - merged.top;

    return mergedWidth <= baseWidth + 1.6
        && mergedHeight <= baseHeight + 1.6;
}

function boxesOverlapOrTouch(a: Box, b: Box) {
    return a.left <= b.right
        && a.right >= b.left
        && a.top <= b.bottom
        && a.bottom >= b.top;
}

function numberAt(items: unknown[], index: number) {
    const value = items[index];
    return typeof value === "number" && Number.isFinite(value) ? value : null;
}

function milToMm(value: number) {
    return value / MIL_PER_MM;
}

function roundMm(value: number) {
    return Number(value.toFixed(4));
}
