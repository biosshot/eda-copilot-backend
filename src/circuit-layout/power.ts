import { isGroundSignal } from './ground.ts';

/** Shared power-net naming semantics used by schematic short symbols and PCB placement. */
export function isPowerSignal(signalName: string) {
    if (!signalName) return false;
    const s = signalName.toUpperCase();
    if (isGroundSignal(s)) return false;
    if (s === 'BATTERY') return true;
    if (/^USB_[V\d]/i.test(signalName)) return true;
    if (/^V(?:CC|DD|BAT|IN|OUT|REF|REG|PP|SS|EE|BUS|[0-9])/i.test(signalName)) return true;
    if (/^[AVDG]?V(?:DD|CC)/i.test(signalName)) return true;
    if (/^[+-]?V[+-]?$|^V$/i.test(signalName)) return true;
    if (/^[+-]?\d+(?:\.\d+)?V/i.test(signalName)) return true;
    if (/^\d+V\d+$/i.test(signalName)) return true;
    return false;
}
