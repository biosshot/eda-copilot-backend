import type { AnalysisFrequencyReport } from '../../native/analyze/index.js';
import { auditFrequencyCoverage, type FrequencyInterval } from './p7-frequency-planning.ts';

export const p7LowBandAuditPolicy = {maxStepHz: 2250, relativeTolerance: 0.02, waveFloor: 0.01, lossFloorW: 1e-18} as const;

export type ChannelFrequencyReplay = (frequencyHz: number, remainingSeconds: number) => AnalysisFrequencyReport;

export function auditSegmentedResponse(options: {
    requestedIntervalHz: FrequencyInterval;
    applicabilityBandHz: FrequencyInterval;
    portNames: readonly string[];
    evaluate: ChannelFrequencyReplay;
    deadlineMs: number;
    nowMs?: () => number;
}) {
    const { requestedIntervalHz, applicabilityBandHz, portNames, evaluate, deadlineMs, nowMs = () => performance.now() } = options;
    const count = portNames.length;
    if (count < 1 || count > 16) throw new Error('CHANNEL_FREQUENCY_REPLAY: 1..16 physical ports required');
    // Only absorb binary64 rounding in the sum of squares, scaled by its term count.
    const powerRoundoff = 64 * Number.EPSILON * count;
    const keys = (prefix: string, size: number) => Array.from({ length: size }, (_, index) => prefix + index);
    const waves = keys('S:', count * count);
    const losses = keys('totalPassiveLossW:', count);
    const gates = Object.fromEntries([
        ...waves.map(key => [key, { absoluteFloor: p7LowBandAuditPolicy.waveFloor, relativeTolerance: p7LowBandAuditPolicy.relativeTolerance }]),
        ...losses.map(key => [key, { absoluteFloor: p7LowBandAuditPolicy.lossFloorW, relativeTolerance: p7LowBandAuditPolicy.relativeTolerance }]),
    ]);
    const interpolation = auditFrequencyCoverage({
        demand: { frequency: { startHz: requestedIntervalHz[0], stopHz: requestedIntervalHz[1] } },
        supportedIntervalsHz: [applicabilityBandHz],
        policy: {
            maxSamples: 17,
            maxStepHz: p7LowBandAuditPolicy.maxStepHz,
            gates,
            modelVersion: 'segmented-p6-interpolation-v1',
        },
        deadlineMs,
        nowMs,
        evaluatePhysical: frequencyHz => {
            const remainingSeconds = (deadlineMs - nowMs()) / 1000;
            if (remainingSeconds <= 0) throw new Error('CHANNEL_FREQUENCY_REPLAY: shared deadline exhausted');
            const report = evaluate(frequencyHz, remainingSeconds);
            const parameters = report.parameters?.[0];
            if (report.status !== 'complete' || report.frequenciesHz?.length !== 1 ||
                report.frequenciesHz[0] !== frequencyHz || report.parameters?.length !== 1 ||
                report.ports?.length !== count || report.ports.some((port, i) => port.name !== portNames[i]) ||
                report.referenceOhm?.length !== count || report.referenceOhm.some(value => value !== 50) ||
                parameters?.length !== count || parameters.some(row => !Array.isArray(row) || row.length !== count ||
                    row.some(value => !value || !Number.isFinite(value.real) || !Number.isFinite(value.imaginary)))) {
                throw new Error('CHANNEL_FREQUENCY_REPLAY: finite square S50 response on the declared axis and port order required');
            }
            const response: Record<string, readonly [number, number]> = {};
            for (let input = 0; input < count; input++) {
                let outgoing = 0;
                for (let output = 0; output < count; output++) {
                    const { real, imaginary } = parameters[output][input];
                    response[waves[output * count + input]] = [real, imaginary];
                    outgoing += real * real + imaginary * imaginary;
                }
                if (!Number.isFinite(outgoing) || outgoing > 1 + powerRoundoff) {
                    throw new Error('CHANNEL_FREQUENCY_REPLAY: outgoing wave power exceeds the passive input balance');
                }
                response[losses[input]] = [12.5 * Math.max(0, 1 - outgoing), 0];
            }
            return response;
        },
    });
    return {
        applicabilityBandHz,
        interpolation,
        physicalCoverage: {
            status: 'unverified' as const,
            reason: 'Independent full assembled-channel frequency reference unavailable',
        },
        unverifiedObservables: ['group delay', 'regional copper loss', 'separate component/copper loss'],
        interpolationScope: 'Fresh evaluations of the final P6 composition with the admitted block and component models. S50 entries are output-major; total passive loss is the power-wave balance for each 1 A RMS Norton input.',
    };
}
