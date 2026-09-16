import type { PlacementRules } from '#types/pcb/layout-rules.ts';
import type { PcbComponent, PlacementInput } from '#types/pcb/layout-model.ts';
import { componentBox } from '../pcb-auto-place/geometry.ts';
import {
    rotatePrimitive,
    translatePrimitive,
    unionPrimitive,
    type PlacementPrimitive,
} from './primitives.ts';

export type LocalLayoutSeed = {
    x: number;
    y: number;
    rotate?: number;
};

export type LocalLayout = Record<string, LocalLayoutSeed>;

type RuntimePcbMetadata = PcbComponent['pcb'] & {
    __localLayoutSeed?: LocalLayoutSeed;
};

type RawBlockWithLocalLayout = PlacementRules['blocks'][number] & {
    localLayout?: LocalLayout | null;
};

const LOCAL_LAYOUT_MODULE_PREFIX = '__eda_local_layout__:';
const MAX_LEGALIZE_ITERATIONS = 100;
const EPSILON = 1e-6;

/**
 * The current placement-rule schema intentionally remains unchanged. Instrument
 * block(...) calls before the existing DSL runner so sparse localLayout data can
 * travel through its already-supported module collection, then strip the
 * reserved carrier before normal rule validation. This keeps localLayout a
 * TypeScript-side preprocessing concern and does not change native contracts.
 */
export function instrumentLocalLayoutDsl(code: string) {
    return `
const __edaOriginalBlock = block;
block = (...__edaArgs) => {
    const __edaOptions = (__edaArgs[3] && typeof __edaArgs[3] === "object") ? __edaArgs[3] : __edaArgs[4];
    if (__edaOptions && __edaOptions.localLayout && typeof __edaOptions.localLayout === "object") {
        module("${LOCAL_LAYOUT_MODULE_PREFIX}" + encodeURIComponent(JSON.stringify({ block: __edaArgs[0], layout: __edaOptions.localLayout })), []);
    }
    return __edaOriginalBlock(...__edaArgs);
};
${code}`;
}

export function extractLocalLayouts(rules: PlacementRules): Map<string, LocalLayout> {
    const layouts = new Map<string, LocalLayout>();
    for (const block of rules.blocks as RawBlockWithLocalLayout[]) {
        if (!block.localLayout || Object.keys(block.localLayout).length === 0) continue;
        layouts.set(block.name, block.localLayout);
    }
    for (const module of rules.modules ?? []) {
        if (!module.name.startsWith(LOCAL_LAYOUT_MODULE_PREFIX)) continue;
        try {
            const payload = JSON.parse(decodeURIComponent(module.name.slice(LOCAL_LAYOUT_MODULE_PREFIX.length))) as {
                block?: unknown;
                layout?: unknown;
            };
            if (typeof payload.block !== 'string' || !payload.layout || typeof payload.layout !== 'object' || Array.isArray(payload.layout)) continue;
            layouts.set(payload.block, payload.layout as LocalLayout);
        } catch {
            throw new Error('Invalid PCB placement DSL: malformed localLayout metadata.');
        }
    }
    return layouts;
}

export function stripLocalLayoutCarriers(rules: PlacementRules): PlacementRules {
    return {
        ...rules,
        modules: (rules.modules ?? []).filter((module) => !module.name.startsWith(LOCAL_LAYOUT_MODULE_PREFIX)),
    };
}

export function validateLocalLayouts(
    circuit: { components: Array<{ designator: string }> },
    rules: PlacementRules,
    layouts: Map<string, LocalLayout>,
) {
    const circuitComponents = new Set(circuit.components.map((component) => component.designator));
    const rulesByDesignator = new Map(rules.component_rules.map((rule) => [rule.designator, rule]));
    const errors: string[] = [];

    for (const [blockName, layout] of layouts) {
        const block = rules.blocks.find((candidate) => candidate.name === blockName);
        if (!block) {
            errors.push(`localLayout references unknown block "${blockName}".`);
            continue;
        }
        const members = new Set(block.component_designators);
        for (const rule of rules.component_rules) {
            if (rule.block_name === blockName) members.add(rule.designator);
        }

        for (const [designator, seed] of Object.entries(layout)) {
            if (!seed || typeof seed !== 'object') {
                errors.push(`block("${blockName}").localLayout.${designator} must be a coordinate object.`);
                continue;
            }
            if (!circuitComponents.has(designator)) {
                errors.push(`block("${blockName}").localLayout references unknown component "${designator}".`);
                continue;
            }
            if (!members.has(designator)) {
                errors.push(`block("${blockName}").localLayout component "${designator}" is not a direct member of that block.`);
                continue;
            }
            if (!Number.isFinite(seed.x) || !Number.isFinite(seed.y) || (seed.rotate !== undefined && !Number.isFinite(seed.rotate))) {
                errors.push(`block("${blockName}").localLayout.${designator} requires finite x, y, and rotate values.`);
            }
            const rule = rulesByDesignator.get(designator);
            if (rule?.fixedPlacement || rule?.edgeMount || rule?.edgePlace) {
                errors.push(`block("${blockName}").localLayout component "${designator}" cannot also use fixed(), edgeMount(), or edgePlace().`);
            }
        }
    }

    if (errors.length > 0) {
        throw new Error([
            'Invalid PCB placement DSL:',
            'Invalid localLayout rules:',
            ...errors.map((error) => `- ${error}`),
        ].join('\n'));
    }
}

export function attachLocalLayoutSeeds(input: PlacementInput, layouts: Map<string, LocalLayout>) {
    if (layouts.size === 0) return input;
    for (const component of input.components) {
        const seed = layouts.get(component.block_name)?.[component.designator];
        if (!seed) continue;
        (component.pcb as RuntimePcbMetadata).__localLayoutSeed = {
            x: seed.x,
            y: seed.y,
            ...(seed.rotate !== undefined ? { rotate: normalizeRotation(seed.rotate) } : {}),
        };
    }
    return input;
}

export function prepareLocalLayoutPrimitives(
    blockName: string,
    primitives: PlacementPrimitive[],
    componentByDesignator: Map<string, PcbComponent> | undefined,
    clearance: number,
    clearanceResolver?: (a: string, b: string) => number,
): PlacementPrimitive[] {
    if (!componentByDesignator) return primitives;
    const seeds = new Map<string, LocalLayoutSeed>();
    for (const component of componentByDesignator.values()) {
        if (component.block_name !== blockName) continue;
        const seed = (component.pcb as RuntimePcbMetadata).__localLayoutSeed;
        if (seed) seeds.set(component.designator, seed);
    }
    if (seeds.size === 0) return primitives;

    const expanded = primitives.flatMap((primitive) => splitPrimitiveWhenNeeded(primitive, seeds, componentByDesignator));
    const seeded: PlacementPrimitive[] = [];
    const free: PlacementPrimitive[] = [];

    for (const primitive of expanded) {
        const placement = primitive.placements.length === 1 ? primitive.placements[0] : undefined;
        const seed = placement ? seeds.get(placement.designator) : undefined;
        if (!placement || !seed) {
            free.push(primitive);
            continue;
        }
        seeded.push(applySeedPose(primitive, seed));
    }

    if (seeded.length === 0) return primitives;
    const legalized = legalizeLocalLayout(seeded, clearance, clearanceResolver);
    const macro = unionPrimitive(
        `local-layout:${blockName}`,
        'island',
        `${blockName}:localLayout`,
        `local-layout:${blockName}`,
        legalized,
    );
    // The seed topology is rigid internally, but the existing block solver may
    // translate/rotate it as one primitive while placing omitted/free members.
    macro.locked = false;
    return [macro, ...free];
}

export function legalizeLocalLayout(
    seeded: PlacementPrimitive[],
    defaultClearance: number,
    clearanceResolver?: (a: string, b: string) => number,
): PlacementPrimitive[] {
    const current = seeded.map(clonePrimitive);
    const seedCenters = new Map(current.map((primitive, index) => [primitive.id, primitiveCenter(seeded[index])]));

    for (let iteration = 0; iteration < MAX_LEGALIZE_ITERATIONS; iteration += 1) {
        let changed = false;
        for (let aIndex = 0; aIndex < current.length; aIndex += 1) {
            for (let bIndex = aIndex + 1; bIndex < current.length; bIndex += 1) {
                const a = current[aIndex];
                const b = current[bIndex];
                const aDesignator = a.placements[0]?.designator ?? a.id;
                const bDesignator = b.placements[0]?.designator ?? b.id;
                const clearance = clearanceResolver?.(aDesignator, bDesignator) ?? defaultClearance;
                const correction = separationCorrection(a, b, clearance, seedCenters.get(a.id)!, seedCenters.get(b.id)!);
                if (!correction) continue;
                current[aIndex] = translatePrimitive(a, -correction.x / 2, -correction.y / 2);
                current[bIndex] = translatePrimitive(b, correction.x / 2, correction.y / 2);
                changed = true;
            }
        }
        if (!changed) return current;
    }

    if (hasLocalViolations(current, defaultClearance, clearanceResolver)) {
        throw new Error('LOCAL_LAYOUT_LEGALIZATION_FAILED: could not resolve local component collisions within 100 iterations.');
    }
    return current;
}

function splitPrimitiveWhenNeeded(
    primitive: PlacementPrimitive,
    seeds: Map<string, LocalLayoutSeed>,
    componentByDesignator: Map<string, PcbComponent>,
): PlacementPrimitive[] {
    const touched = primitive.placements.some((placement) => seeds.has(placement.designator));
    if (!touched || primitive.placements.length <= 1) return [primitive];

    return primitive.placements.flatMap((placement) => {
        const component = componentByDesignator.get(placement.designator);
        if (!component) return [];
        const bbox = componentBox(component, placement);
        const refPrefix = `${placement.designator}.`;
        const allowedOrientations = component.pcb.allowedRotations
            .map((rotation) => normalizeRotation(rotation - placement.rotate));
        return [{
            id: `${primitive.id}:component:${placement.designator}`,
            kind: 'component' as const,
            label: placement.designator,
            sourceNodeId: `${primitive.sourceNodeId}:component:${placement.designator}`,
            locked: Boolean(component.pcb.fixedPlacement),
            canRotate: !component.pcb.fixedPlacement && allowedOrientations.length > 1,
            allowedOrientations: component.pcb.fixedPlacement ? [0] : allowedOrientations,
            bbox,
            collisionBoxes: [bbox],
            width: bbox.right - bbox.left,
            height: bbox.bottom - bbox.top,
            placements: [{ ...placement }],
            connectionPoints: primitive.connectionPoints.filter((point) => point.ref.startsWith(refPrefix)).map((point) => ({ ...point })),
            pathPorts: primitive.pathPorts?.filter((port) => port.ref.startsWith(refPrefix)).map((port) => ({ ...port, normal: { ...port.normal } })),
            children: [],
        }];
    });
}

function applySeedPose(primitive: PlacementPrimitive, seed: LocalLayoutSeed) {
    const placement = primitive.placements[0];
    if (!placement) return primitive;
    const requestedRotation = normalizeRotation(seed.rotate ?? placement.rotate);
    const rotated = rotatePrimitive(primitive, requestedRotation - placement.rotate);
    const afterRotation = rotated.placements[0];
    const positioned = translatePrimitive(rotated, seed.x - afterRotation.x, seed.y - afterRotation.y);
    return {
        ...positioned,
        locked: false,
    };
}

function separationCorrection(
    a: PlacementPrimitive,
    b: PlacementPrimitive,
    clearance: number,
    seedA: { x: number; y: number },
    seedB: { x: number; y: number },
) {
    const boxesA = a.collisionBoxes?.length ? a.collisionBoxes : [a.bbox];
    const boxesB = b.collisionBoxes?.length ? b.collisionBoxes : [b.bbox];
    let requiredX = 0;
    let requiredY = 0;
    let overlaps = false;

    for (const boxA of boxesA) {
        for (const boxB of boxesB) {
            const x = Math.min(boxA.right, boxB.right) - Math.max(boxA.left, boxB.left) + clearance;
            const y = Math.min(boxA.bottom, boxB.bottom) - Math.max(boxA.top, boxB.top) + clearance;
            if (x <= EPSILON || y <= EPSILON) continue;
            overlaps = true;
            requiredX = Math.max(requiredX, x);
            requiredY = Math.max(requiredY, y);
        }
    }
    if (!overlaps) return null;

    if (requiredX <= requiredY) {
        const sign = orderedSign(seedA.x, seedB.x, a.id, b.id);
        return { x: requiredX * sign, y: 0 };
    }
    const sign = orderedSign(seedA.y, seedB.y, a.id, b.id);
    return { x: 0, y: requiredY * sign };
}

function hasLocalViolations(
    primitives: PlacementPrimitive[],
    defaultClearance: number,
    clearanceResolver?: (a: string, b: string) => number,
) {
    for (let a = 0; a < primitives.length; a += 1) {
        for (let b = a + 1; b < primitives.length; b += 1) {
            const left = primitives[a];
            const right = primitives[b];
            const clearance = clearanceResolver?.(
                left.placements[0]?.designator ?? left.id,
                right.placements[0]?.designator ?? right.id,
            ) ?? defaultClearance;
            if (separationCorrection(left, right, clearance, primitiveCenter(left), primitiveCenter(right))) return true;
        }
    }
    return false;
}

function orderedSign(a: number, b: number, aId: string, bId: string) {
    if (b > a + EPSILON) return 1;
    if (b < a - EPSILON) return -1;
    return aId.localeCompare(bId) <= 0 ? 1 : -1;
}

function primitiveCenter(primitive: PlacementPrimitive) {
    const placement = primitive.placements[0];
    if (placement) return { x: placement.x, y: placement.y };
    return {
        x: (primitive.bbox.left + primitive.bbox.right) / 2,
        y: (primitive.bbox.top + primitive.bbox.bottom) / 2,
    };
}

function clonePrimitive(primitive: PlacementPrimitive): PlacementPrimitive {
    return {
        ...primitive,
        bbox: { ...primitive.bbox },
        collisionBoxes: primitive.collisionBoxes?.map((box) => ({ ...box })),
        placements: primitive.placements.map((placement) => ({ ...placement })),
        connectionPoints: primitive.connectionPoints.map((point) => ({ ...point })),
        pathPorts: primitive.pathPorts?.map((port) => ({ ...port, normal: { ...port.normal } })),
        children: primitive.children.map(clonePrimitive),
    };
}

function normalizeRotation(value: number) {
    return ((Math.round(value) % 360) + 360) % 360;
}
