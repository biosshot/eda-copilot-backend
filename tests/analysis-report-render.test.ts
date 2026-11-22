import assert from 'node:assert/strict';
import test from 'node:test';
import { renderAnalysisReport, type AnalysisDslOutput, type Report } from '../src/analyze.ts';

const makeReport = (changes: Partial<Report>): Report => ({
  contextId: 'fixture', descriptors: [], calculations: [], checks: [], logs: [], plots: [],
  summary: { pass: 0, fail: 0, unknown: 0 }, diagnostics: [], ...changes,
});

test('PI chart preserves missing samples and draws the authored target deterministically', () => {
  const output: AnalysisDslOutput = { status: 'complete', reports: [makeReport({
    calculations: [{ phase: 'completed', result: {
      kind: 'pi', status: 'partial', inputs: { targetImpedanceOhm: 0.15 },
      solution: {
        frequenciesHz: [1000, 2000, 3000], bands: [{}],
        zMatrix: [[[{ real: 0.1, imaginary: 0 }]], null, [[{ real: 0.2, imaginary: 0 }]]],
      },
    } }],
  })] };
  const rendered = renderAnalysisReport(output);
  assert.deepEqual(renderAnalysisReport(output), rendered);
  const chart = rendered.images.find(image => image.name.endsWith('-impedance'))!.svg;
  assert.match(chart, /Target 0\.15 Ω/);
  assert.match(chart, /1 frequency samples unavailable/);
  assert.match(chart, /d="M[\d.,]+M[\d.,]+"/);
  assert.match(chart, /r="4"/);
  assert.equal(rendered.summary.calculations[0]?.status, 'partial');
});

test('plot text is escaped and unknown grid cells use the missing-data color', () => {
  const output: AnalysisDslOutput = { status: 'complete', reports: [makeReport({
    plots: [
      { kind: 'heatmap', title: '<grid>&', data: [[0, null], [2, 3]] },
      { kind: 'table', title: 'Readings', data: { rows: [{ reading: '<image/>' }] } },
      { kind: 'bar', title: 'Power', data: [0, 2, -1] },
    ],
  })] };
  const { images } = renderAnalysisReport(output);
  assert.deepEqual(images.map(i => i.kind), ['heatmap', 'table', 'bar', 'summary']);
  assert.match(images[0]!.svg, /&lt;grid&gt;&amp;/);
  assert.match(images[0]!.svg, /fill="#526174"/);
  assert.doesNotMatch(images[1]!.svg, /<image/);
  assert.match(images[2]!.svg, /Sample index/);
});

test('a refusal retains the diagnostic and emits no nonexistent result field', () => {
  const { images, summary } = renderAnalysisReport({ status: 'failed', reports: [],
    error: { filename: 'fixture.js', code: 'LOCAL_DEADLINE', message: 'Requested solver exceeded its budget', diagnostics: [] },
  });
  assert.deepEqual(images.map(image => image.kind), ['summary']);
  assert.match(images[0]!.svg, /LOCAL_DEADLINE/);
  assert.equal(summary.error?.message, 'Requested solver exceeded its budget');
});

