import { rotatePrimitive, translatePrimitive, type PlacementPrimitive } from '../primitives.ts';
import { roundPlacement } from '../../pcb-auto-place/geometry.ts';
import { NATIVE_BOARD_PACK_CONTRACT_VERSION, type NativePrimitivePackSolution } from './contract.ts';
import { primitiveInFrame, frameTranslation } from './numeric-frame.ts';

export function applyNativeBoardPackSolution(
    primitives: PlacementPrimitive[],
    solution: NativePrimitivePackSolution,
    expectedVersion: number = NATIVE_BOARD_PACK_CONTRACT_VERSION,
) {
    if (solution.version !== expectedVersion) {
        throw new Error(`Unsupported native board pack solution version ${solution.version}`);
    }
    const states = new Map(solution.states.map((state) => [state.primitiveId, state]));
    const primitivesById = new Map(primitives.map((primitive) => [primitive.id, primitive]));
    if (states.size !== primitives.length) {
        throw new Error(`Native board packer returned ${states.size} primitive states for ${primitives.length} primitives`);
    }
    return solution.states.map((state) => {
        const primitive = primitivesById.get(state.primitiveId);
        if (!primitive) throw new Error(`Native board packer returned unknown primitive ${state.primitiveId}`);
        // Mechanical anchors may carry more precision than the placement grid.
        if (primitive.locked) {
            if (state.rotation !== 0 || state.translationX !== 0 || state.translationY !== 0)
                throw new Error(`Native solver moved locked primitive ${primitive.id}`);
            return primitive;
        }
        const frame = solution.numericFrame;
        const source = frame ? primitiveInFrame(primitive, frame.origin) : primitive;
        const rotated = rotatePrimitive(source, state.rotation);
        const dx = frame ? frameTranslation(state.translationX, frame.origin.x, frame.outputOrigin.x) : state.translationX;
        const dy = frame ? frameTranslation(state.translationY, frame.origin.y, frame.outputOrigin.y) : state.translationY;
        const local = translatePrimitive(rotated, dx, dy);
        const transformed = frame ? primitiveInFrame(local, frame.outputOrigin, true) : local;
        if (state.placements) {
            const canonical = ({ designator, x, y, rotate, layer }: typeof state.placements[number]) => ({
                designator,
                x: roundPlacement(x),
                y: roundPlacement(y),
                rotate,
                layer,
            });
            // Compare ticks in the local output frame; narrowing restored
            // absolute coordinates would collapse adjacent ticks at 1e9 mm.
            const toLocal = (p: typeof state.placements[number]) => frame
                ? { ...p, x: p.x-frame.outputOrigin.x, y: p.y-frame.outputOrigin.y } : p;
            const expected = JSON.stringify(state.placements.map(toLocal).map(canonical));
            const actual = JSON.stringify(local.placements.map(canonical));
            if (expected !== actual) throw new Error(`Native primitive transform mismatch for ${primitive.id}: expected=${expected} actual=${actual}`);
        }
        return transformed;
    });
}
