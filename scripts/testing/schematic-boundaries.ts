import type { Circuit } from '../../src/types/circuit.ts';
import { hasConnection } from '../../src/circuit-layout/signals.ts';
import { isGroundSignal } from '../../src/circuit-layout/ground.ts';
import { isPowerSignal } from '../../src/circuit-layout/power.ts';

/** Give an isolated page block the same cross-block net-port requests it has
 * when placed with its neighbours. Null means the fixtures do not match. */
export function pageBoundarySignals(isolated: Circuit, page: Circuit): string[] | null {
    const blocks = new Set(isolated.components.map(c => c.block_name));
    const selected = page.components.filter(c => blocks.has(c.block_name));
    if (!blocks.size || selected.length !== isolated.components.length) return null;
    const byId = new Map(selected.map(c => [c.designator, c]));
    if (byId.size !== isolated.components.length || isolated.components.some(c => {
        const original = byId.get(c.designator);
        const pins = new Map(original?.pins.map(p => [String(p.pin_number), p.signal_name]));
        return !original || original.block_name !== c.block_name || pins.size !== c.pins.length
            || c.pins.some(p => pins.get(String(p.pin_number)) !== p.signal_name);
    })) return null;
    const localSignals = new Set(isolated.components.flatMap(c => c.pins.map(p => p.signal_name))
        .filter(signal => hasConnection(signal) && !isGroundSignal(signal) && !isPowerSignal(signal)));
    return [...new Set(page.components.filter(c => !blocks.has(c.block_name))
        .flatMap(c => c.pins.map(p => p.signal_name)).filter(signal => localSignals.has(signal)))].sort();
}
