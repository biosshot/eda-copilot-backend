export type FrequencyInterval = readonly [startHz: number, stopHz: number];

type FrequencyDemand = {
    frequencyHz?: number;
    frequency?: { startHz: number; stopHz: number };
    spectralBandHz?: FrequencyInterval;
};

export const p7FrequencyPlanningVersion = 1;
export const p7FrequencyBoundariesHz = [0, 1e3, 1e5, 1e6, 1e9, 1e10] as const;

function checkInterval([start, stop]: FrequencyInterval): void {
    if (!Number.isFinite(start) || !Number.isFinite(stop) || start < 0 || start > stop) {
        throw new Error('FREQUENCY_DEMAND: expected a finite ordered nonnegative interval');
    }
}

/** A spectral band must already account for the source edge, tail and DC requirements. */
export function requestedFrequencyInterval(demand: FrequencyDemand): FrequencyInterval {
    const { frequencyHz, frequency, spectralBandHz } = demand;
    const supplied = [frequencyHz, frequency, spectralBandHz].filter(value => value !== undefined);
    if (supplied.length !== 1) throw new Error('FREQUENCY_DEMAND: expected one explicit frequency or spectral band');
    const interval: FrequencyInterval = frequencyHz !== undefined ? [frequencyHz, frequencyHz]
        : frequency ? [frequency.startHz, frequency.stopHz] : spectralBandHz!;
    checkInterval(interval);
    return interval;
}

type Gate = { readonly absoluteFloor: number; readonly relativeTolerance: number };
type ComplexValue = readonly [real: number, imaginary: number];
type Sample = Readonly<Record<string, ComplexValue>>;
export type PhysicalFrequencyEvaluator = (frequencyHz: number) => Sample;
type Policy = {
    readonly maxSamples: number;
    readonly maxStepHz: number;
    readonly gates: Readonly<Record<string, Gate>>;
    readonly modelVersion: string;
};
type Holdout = {
    intervalHz: FrequencyInterval;
    frequencyHz: number;
    normalizedErrors: Record<string, number>;
    accepted: boolean;
    promoted: boolean;
};
type Gap = {
    intervalHz: FrequencyInterval;
    reason: 'unsupported' | 'sample-budget' | 'deadline' | 'cancelled' | 'floating-point-resolution' | 'physical-evaluation';
};

function split(interval: FrequencyInterval, controls: readonly number[]): FrequencyInterval[] {
    const [start, stop] = interval;
    if (start === stop) return [interval];
    const boundaries = [start, ...new Set(controls.filter(f => f > start && f < stop).sort((a, b) => a - b)), stop];
    return boundaries.slice(1).map((boundary, i) => [boundaries[i], boundary]);
}

/**
 * Audit interpolation against independent physical solves with CPU binary64 arithmetic.
 * Each gate names one scalar or complex observable (e.g. an individual S entry, delay or loss).
 * Supported intervals must already describe the common applicability of every physical block.
 * The caller sets the maximum step from relevant time or resonance scales and enforces
 * its deadline within each synchronous physical evaluation.
 */
export function auditFrequencyCoverage(options: {
    demand: FrequencyDemand;
    supportedIntervalsHz: readonly FrequencyInterval[];
    transitionFrequenciesHz?: readonly number[];
    policy: Policy;
    evaluatePhysical: PhysicalFrequencyEvaluator;
    deadlineMs?: number;
    nowMs?: () => number;
    signal?: AbortSignal;
}) {
    const { demand, supportedIntervalsHz, transitionFrequenciesHz = [], policy, evaluatePhysical } = options;
    const { deadlineMs, nowMs = () => performance.now(), signal } = options;
    if (deadlineMs !== undefined && !Number.isFinite(deadlineMs)) {
        throw new Error('FREQUENCY_POLICY: expected a finite monotonic deadline');
    }
    const requestedIntervalHz = requestedFrequencyInterval(demand);
    const gates = Object.entries(policy.gates);
    if (!Number.isSafeInteger(policy.maxSamples) || policy.maxSamples < 1 ||
        !Number.isFinite(policy.maxStepHz) || policy.maxStepHz <= 0 ||
        !policy.modelVersion || gates.length === 0 ||
        gates.some(([, gate]) => !Number.isFinite(gate.absoluteFloor) || gate.absoluteFloor <= 0 ||
            !Number.isFinite(gate.relativeTolerance) || gate.relativeTolerance <= 0)) {
        throw new Error('FREQUENCY_POLICY: expected a version, finite positive budgets and observable gates');
    }
    supportedIntervalsHz.forEach(checkInterval);
    transitionFrequenciesHz.forEach(f => checkInterval([f, f]));
    const requestedIntervalsHz = split(requestedIntervalHz, p7FrequencyBoundariesHz);
    const segments = requestedIntervalsHz.flatMap(interval => split(interval, [
        ...transitionFrequenciesHz, ...supportedIntervalsHz.flat(),
    ]));
    const training = new Set<number>();
    const holdouts: Holdout[] = [];
    const validatedIntervalsHz: FrequencyInterval[] = [];
    const unverified: Gap[] = [];
    const evaluationFailures: { frequencyHz: number; message: string }[] = [];
    const sampled = new Map<number, Sample | null>();

    function exhausted(): 'sample-budget' | 'deadline' | 'cancelled' | undefined {
        if (signal?.aborted) return 'cancelled';
        if (deadlineMs !== undefined && nowMs() >= deadlineMs) return 'deadline';
        if (sampled.size >= policy.maxSamples) return 'sample-budget';
    }

    function sample(frequencyHz: number) {
        if (sampled.has(frequencyHz)) return sampled.get(frequencyHz)!;
        if (exhausted()) return undefined;
        try {
            const raw = evaluatePhysical(frequencyHz);
            const value = Object.fromEntries(gates.map(([name]) => {
                const pair = raw[name];
                if (!pair || pair.length !== 2 || !pair.every(Number.isFinite)) {
                    throw new Error(`Missing or nonfinite observable ${name}`);
                }
                return [name, [pair[0], pair[1]] as const];
            }));
            sampled.set(frequencyHz, value);
            return value;
        } catch (error) {
            sampled.set(frequencyHz, null);
            evaluationFailures.push({ frequencyHz, message: error instanceof Error ? error.message : String(error) });
            return null;
        }
    }

    const supported = ([start, stop]: FrequencyInterval) => stop <= p7FrequencyBoundariesHz.at(-1)! &&
        supportedIntervalsHz.some(([min, max]) => min <= start && stop <= max);
    const isolated = new Set<number>();
    for (const [point, end] of supportedIntervalsHz) {
        if (point === end && point >= requestedIntervalHz[0] && point <= requestedIntervalHz[1] &&
            !segments.some(interval => interval[0] <= point && point <= interval[1] && supported(interval))) {
            isolated.add(point);
        }
    }
    segments.unshift(...Array.from(isolated, (point): FrequencyInterval => [point, point]));

    const pending: FrequencyInterval[] = [];
    for (const intervalHz of segments) {
        const [start, stop] = intervalHz;
        if (!supported(intervalHz)) {
            unverified.push({ intervalHz, reason: 'unsupported' });
            continue;
        }
        const left = sample(start);
        const right = sample(stop);
        if (left) training.add(start);
        if (right) training.add(stop);
        if (!left || !right) {
            unverified.push({ intervalHz, reason: left === null || right === null ? 'physical-evaluation' : exhausted()! });
        } else if (start === stop) {
            validatedIntervalsHz.push(intervalHz);
        } else {
            pending.push(intervalHz);
        }
    }

    for (let i = 0; i < pending.length; i++) {
        const intervalHz = pending[i];
        const [start, stop] = intervalHz;
        const width = stop - start;
        const midpoint = start + width / 2;
        if (midpoint === start || midpoint === stop) {
            unverified.push({ intervalHz, reason: 'floating-point-resolution' });
            continue;
        }
        const actual = sample(midpoint);
        if (!actual) {
            unverified.push({ intervalHz, reason: actual === null ? 'physical-evaluation' : exhausted()! });
            continue;
        }
        const left = sampled.get(start)!;
        const right = sampled.get(stop)!;
        const t = (midpoint - start) / width;
        const normalizedErrors = Object.fromEntries(gates.map(([name, gate]) => {
            const [re, im] = actual[name];
            const [leftRe, leftIm] = left[name];
            const [rightRe, rightIm] = right[name];
            const error = Math.hypot(leftRe * (1 - t) + rightRe * t - re, leftIm * (1 - t) + rightIm * t - im);
            const scale = Math.max(gate.absoluteFloor, Math.hypot(re, im));
            return [name, Number.isFinite(error) && Number.isFinite(scale)
                ? Math.min(Number.MAX_VALUE, error / scale) : Number.MAX_VALUE];
        }));
        const accepted = width <= policy.maxStepHz &&
            gates.every(([name, gate]) => normalizedErrors[name] <= gate.relativeTolerance);
        holdouts.push({ intervalHz, frequencyHz: midpoint, normalizedErrors, accepted, promoted: !accepted });
        if (accepted) {
            validatedIntervalsHz.push(intervalHz);
        } else {
            training.add(midpoint);
            pending.push([start, midpoint], [midpoint, stop]);
        }
    }

    const byStart = (a: FrequencyInterval, b: FrequencyInterval) => a[0] - b[0] || a[1] - b[1];
    const status: 'validated' | 'refused' | 'unverified' = unverified.length === 0 ? 'validated'
        : unverified.every(gap => gap.reason === 'unsupported') && validatedIntervalsHz.length === 0 ? 'refused'
        : 'unverified';
    return {
        status,
        planningVersion: p7FrequencyPlanningVersion,
        policy,
        requestedIntervalHz,
        requestedIntervalsHz,
        validatedIntervalsHz: validatedIntervalsHz.sort(byStart),
        unverified: unverified.sort((a, b) => byStart(a.intervalHz, b.intervalHz)),
        trainingFrequenciesHz: [...training].sort((a, b) => a - b),
        holdouts,
        evaluationFailures,
        samples: [...sampled].flatMap(([frequencyHz, value]) => value ? [{ frequencyHz, value }] : []),
    };
}
