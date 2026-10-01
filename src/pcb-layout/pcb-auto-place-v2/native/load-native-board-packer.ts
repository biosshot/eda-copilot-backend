import { backendResource } from '#runtime/resources.ts';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';
import type { NativeBoardPackerAddon } from './contract.ts';
import env from '#utils/env.ts';
import { PCB_NUMERIC_CONTRACT } from '../../f32.ts';

let cachedAddon: NativeBoardPackerAddon | undefined;

export function loadNativeBoardPacker(): NativeBoardPackerAddon {
    if (cachedAddon) return cachedAddon;
    const require = createRequire(import.meta.url);
    const configuredPath = env.PCB_BOARD_PACKER_NATIVE_PATH;
    const addonPath = configuredPath
        ? resolve(configuredPath)
        : backendResource('native', 'pcb-board-packer', 'index.cjs');
    try {
        const addon = require(addonPath) as NativeBoardPackerAddon;
        const precision = addon.numericContract?.();
        if (precision !== PCB_NUMERIC_CONTRACT) {
            throw new Error(`PCB numeric contract mismatch: expected ${PCB_NUMERIC_CONTRACT}, got ${precision ?? 'legacy addon'}; rebuild the native addon`);
        }
        cachedAddon = addon;
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        throw new Error(`Rust PCB board packer addon is unavailable at ${addonPath}: ${message}`);
    }
    return cachedAddon;
}
