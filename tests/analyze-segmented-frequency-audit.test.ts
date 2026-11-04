import test from 'node:test';
import assert from 'node:assert/strict';
import type { AnalysisFrequencyReport } from '../native/analyze/index.js';
import { auditSegmentedResponse, type ChannelFrequencyReplay } from '../src/analyze/segmented-frequency-audit.ts';

function response(frequency: number, value = 0.1): AnalysisFrequencyReport {
    return {
        status: 'complete',
        diagnostics: [],
        elapsedMs: 0,
        ports: [{name:'p', index:0}],
        referenceOhm: [50],
        frequenciesHz: [frequency],
        parameters: [[[{real:value, imaginary:0}]]],
    };
}

const common = {
    requestedIntervalHz: [1000, 10000] as const,
    applicabilityBandHz: [2500, 6500] as const,
    portNames: ['p'],
    deadlineMs: 1000,
    nowMs: () => 0,
};

test('replay preserves DSL demand, limits component band, and separates physical evidence', () => {
    const frequencies: number[] = [];
    const report = auditSegmentedResponse({
        ...common,
        evaluate: f => {
            frequencies.push(f);
            return response(f);
        },
    });
    assert.deepEqual(report.interpolation.requestedIntervalHz, [1000, 10000]);
    assert.deepEqual(report.interpolation.validatedIntervalsHz, [[2500, 4500], [4500, 6500]]);
    assert.deepEqual(report.interpolation.unverified, [
        { intervalHz: [1000, 2500], reason: 'unsupported' },
        { intervalHz: [6500, 10000], reason: 'unsupported' },
    ]);
    assert.deepEqual(frequencies, [2500, 6500, 4500, 3500, 5500]);
    assert.equal(report.physicalCoverage.status, 'unverified');

    const within = auditSegmentedResponse({...common, requestedIntervalHz:[3000, 5000], evaluate:response});
    assert.equal(within.interpolation.status, 'validated');
    assert.equal(within.physicalCoverage.status, 'unverified');
});

test('malformed and nonfinite S responses leave the interval unverified', () => {
    for (const evaluate of [
        (f: number) => ({ ...response(f), parameters: [[[]]] }),
        (f: number) => response(f, NaN),
    ]) {
        const report = auditSegmentedResponse({...common, requestedIntervalHz:[3000, 5000], evaluate});
        assert.equal(report.interpolation.status, 'unverified');
        assert.equal(report.interpolation.unverified[0].reason, 'physical-evaluation');
        assert.deepEqual(report.interpolation.validatedIntervalsHz, []);
    }
});

test('replay consumes the shared deadline and skips further solves when exhausted', () => {
    let now = 0;
    const budgets: number[] = [];
    const evaluate: ChannelFrequencyReplay = (f, remainingSeconds) => {
        budgets.push(remainingSeconds);
        now += 500;
        return response(f);
    };
    const report = auditSegmentedResponse({...common, requestedIntervalHz:[3000, 5000], evaluate, nowMs:()=>now});
    assert.deepEqual(budgets, [1, .5]);
    assert.deepEqual(report.interpolation.unverified, [{ intervalHz: [3000, 5000], reason: 'deadline' }]);
    assert.equal(report.physicalCoverage.status, 'unverified');
});

test('rejects active outgoing power and only clamps binary64 roundoff', () => {
    const options = {...common, requestedIntervalHz:[3000, 5000] as const};
    const active = auditSegmentedResponse({...options, evaluate:f => response(f, 2)});
    assert.equal(active.interpolation.status, 'unverified');
    assert.match(active.interpolation.evaluationFailures[0].message, /passive input balance/);
    assert.deepEqual(active.interpolation.validatedIntervalsHz, []);

    const roundoff = auditSegmentedResponse({...options, evaluate:f => response(f, 1 + Number.EPSILON)});
    assert.equal(roundoff.interpolation.status, 'validated');
    assert.deepEqual(roundoff.interpolation.samples[0].value['totalPassiveLossW:0'], [0, 0]);
});

test('rejects empty or oversized port mappings before replay', () => {
    for (const portNames of [[], Array.from({length:17}, (_, i) => String(i))]) {
        assert.throws(() => auditSegmentedResponse({
            ...common, portNames, evaluate:() => assert.fail('unexpected replay'),
        }), /1\.\.16 physical ports required/);
    }
});
