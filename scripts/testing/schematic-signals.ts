import type { CircuitComponent } from '../../src/types/circuit.ts';

export type SignalMismatch = { signalName: string; designator: string; pinNumber: string; problem: string };

/** Keep every affected pin, without the pairwise connectivity error limit. */
export function signalCheck(differences: SignalMismatch[]) {
    const unique = [...new Map(differences.map(d => [JSON.stringify(d), d])).values()];
    return { valid: unique.length === 0,
        mismatchedSignals: new Set(unique.map(d => d.signalName)).size,
        mismatchedPins: new Set(unique.map(d => JSON.stringify([d.designator, d.pinNumber]))).size,
        differences: unique };
}

/** Compare the original assignments with the serialized ASM, including singleton
 * and NC pins. Only explicitly generated service symbols may be added. */
export function compareSignalAssignments(expected: CircuitComponent[], actual: CircuitComponent[], addedDesignators: ReadonlySet<string>) {
    const flatten = (components: CircuitComponent[]) => components.flatMap(c => c.pins.map(p => ({
        designator: c.designator, pinNumber: String(p.pin_number), signalName: p.signal_name,
    })));
    const pinKey = (p: ReturnType<typeof flatten>[number]) => JSON.stringify([p.designator, p.pinNumber]);
    const original = flatten(expected), originalById = new Map(original.map(p => [pinKey(p), p]));
    const originals = new Set(expected.map(c => c.designator));
    const remaining = new Set(originalById.keys()), differences: SignalMismatch[] = [];
    for (const p of flatten(actual)) {
        if (addedDesignators.has(p.designator) && !originals.has(p.designator)) continue;
        const id = pinKey(p), before = originalById.get(id);
        if (!before) differences.push({ ...p, problem: 'лишний вывод в ASM' });
        else {
            if (!remaining.delete(id)) differences.push({ ...before, problem: 'дублирующийся вывод в ASM' });
            if (before.signalName !== p.signalName) differences.push({ ...before, problem: `signal_name изменён на ${JSON.stringify(p.signalName)}` });
        }
    }
    for (const id of remaining) differences.push({ ...originalById.get(id)!, problem: 'вывод отсутствует в ASM' });
    return signalCheck(differences);
}

export function formatSignalCheck(check: ReturnType<typeof signalCheck>) {
    if (check.valid) return 'Сигналы и выводы соответствуют исходной схеме.';
    const groups = new Map<string, Map<string, Set<string>>>();
    for (const d of check.differences) {
        const pins = groups.get(d.signalName) ?? new Map<string, Set<string>>();
        const id = `${d.designator}.${d.pinNumber}`, problems = pins.get(id) ?? new Set<string>();
        problems.add(d.problem); pins.set(id, problems); groups.set(d.signalName, pins);
    }
    return [`Не соответствует: сигналов — ${check.mismatchedSignals}, выводов — ${check.mismatchedPins}.`,
        ...[...groups].map(([signal, pins]) => `${signal || '(без сигнала)'} — ${[...pins].map(([pin, problems]) => `${pin} (${[...problems].join('; ')})`).join(', ')}`)].join('\n');
}
