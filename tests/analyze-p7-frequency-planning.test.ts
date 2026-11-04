import assert from 'node:assert/strict';
import test from 'node:test';
import { auditFrequencyCoverage, requestedFrequencyInterval, type PhysicalFrequencyEvaluator } from '../src/analyze/p7-frequency-planning.ts';

const policy = {
    maxSamples: 100,
    maxStepHz: 250,
    gates: { s21: { absoluteFloor: 1e-3, relativeTolerance: 0.02 } },
    modelVersion: 'test',
};

test('explicit demand is preserved across all P7 intervals and outside applicability', () => {
    assert.throws(() => requestedFrequencyInterval({}), /FREQUENCY_DEMAND/);
    assert.throws(() => requestedFrequencyInterval({ frequencyHz: 1, spectralBandHz: [0, 1] }), /FREQUENCY_DEMAND/);
    const report = auditFrequencyCoverage({
        demand: { spectralBandHz: [0, 12e9] },
        supportedIntervalsHz: [[1e3, 1e5]],
        policy: { ...policy, maxStepHz: 1e5 },
        evaluatePhysical: f => ({ s21: [f / 1e5, 0] }),
    });
    assert.equal(report.status, 'unverified');
    assert.deepEqual(report.requestedIntervalsHz, [[0, 1e3], [1e3, 1e5], [1e5, 1e6], [1e6, 1e9], [1e9, 1e10], [1e10, 12e9]]);
    assert.deepEqual(report.validatedIntervalsHz, [[1e3, 1e5]]);
    assert.equal(report.unverified.length, 5);
    assert.deepEqual(report.samples.map(sample => sample.frequencyHz), [1e3, 1e5, 50500]);
});

test('a failed complex holdout is promoted and fresh holdouts audit each refined interval', () => {
    const report = auditFrequencyCoverage({
        demand: { frequency: { startHz: 1000, stopHz: 2000 } },
        supportedIntervalsHz: [[1000, 2000]],
        transitionFrequenciesHz: [1500],
        policy,
        evaluatePhysical: f => ({ s21: [0, 1 + ((f - 1250) / 1000) ** 2] }),
    });
    assert.equal(report.status, 'validated');
    assert.ok(report.trainingFrequenciesHz.includes(1500));
    const failed = report.holdouts.find(point => point.frequencyHz === 1250);
    assert.ok(failed && failed.promoted && failed.normalizedErrors.s21 > 0.02);
    const fresh = report.holdouts.filter(point => point.accepted);
    assert.deepEqual(fresh.map(point => point.frequencyHz), [1125, 1375, 1625, 1875]);
    assert.ok(fresh.every(point => !report.trainingFrequenciesHz.includes(point.frequencyHz)));
});

test('budget cannot validate a span without its fresh holdout', () => {
    const report = auditFrequencyCoverage({
        demand: { frequency: { startHz: 1000, stopHz: 1250 } },
        supportedIntervalsHz: [[1000, 2000]],
        policy: { ...policy, maxSamples: 2 },
        evaluatePhysical: () => ({ s21: [0, 0] }),
    });
    assert.equal(report.status, 'unverified');
    assert.deepEqual(report.unverified, [{ intervalHz: [1000, 1250], reason: 'sample-budget' }]);
    assert.deepEqual(report.validatedIntervalsHz, []);
});

test('a DC point is evaluated separately and unsupported points refuse before evaluation', () => {
    const run = (frequencyHz: number) => auditFrequencyCoverage({
        demand: { frequencyHz }, supportedIntervalsHz: [[0, 0]], policy,
        evaluatePhysical: () => ({ s21: [0, 0] }),
    });
    assert.deepEqual(run(0).validatedIntervalsHz, [[0, 0]]);
    assert.equal(run(0).holdouts.length, 0);
    assert.equal(run(1).status, 'refused');
    assert.equal(run(1).samples.length, 0);
});

test('an evaluator missing a required observable leaves a physical-evaluation gap', () => {
    const report = auditFrequencyCoverage({
        demand: { frequency: { startHz: 1000, stopHz: 1250 } },
        supportedIntervalsHz: [[1000, 2000]],
        policy: { ...policy, gates: { ...policy.gates, loss: policy.gates.s21 } },
        evaluatePhysical: (f): ReturnType<PhysicalFrequencyEvaluator> => f === 1125 ? { s21: [0, 0] } as const : { s21: [0, 0], loss: [1, 0] } as const,
    });
    assert.deepEqual(report.unverified, [{ intervalHz: [1000, 1250], reason: 'physical-evaluation' }]);
    assert.match(report.evaluationFailures[0].message, /loss/);
});


test('an elapsed deadline stops before scheduling a fresh physical solve', () => {
    let now = 0;
    const report = auditFrequencyCoverage({
        demand: { frequency: { startHz: 1000, stopHz: 1250 } },
        supportedIntervalsHz: [[1000, 2000]],
        deadlineMs: 2,
        nowMs: () => now,
        policy,
        evaluatePhysical: () => {
            now++;
            return { s21: [0, 0] };
        },
    });
    assert.equal(now, 2);
    assert.deepEqual(report.unverified, [{ intervalHz: [1000, 1250], reason: 'deadline' }]);
});
