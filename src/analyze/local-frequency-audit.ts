import type { FrequencyInterval } from './p7-frequency-planning.ts';

export interface LocalFrequencyAuditRequest {
    intervalHz: FrequencyInterval;
    maxSamples: number;
    maxStepHz: number;
    relativeTolerance: number;
    impedanceFloorOhm: number;
    lossFloorW: number;
}

/** Low-band interpolation resolution policy for intrinsic Z and total Cu loss.
 * Narrower resonance scales require a separate policy and evidence.
 */
export function localFrequencyAuditRequest(intervalHz: FrequencyInterval): LocalFrequencyAuditRequest {
    return {
        intervalHz,
        maxSamples: 33,
        maxStepHz: 25_000,
        relativeTolerance: 0.02,
        impedanceFloorOhm: 1e-6,
        lossFloorW: 1e-12,
    };
}
