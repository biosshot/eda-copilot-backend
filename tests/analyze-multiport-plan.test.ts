import test from 'node:test';
import assert from 'node:assert/strict';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import { createAnalysisExecutor, type AnalysisTask } from '../src/analyze/analysis-dsl/executor.ts';
import { createGroupExecutor } from '../src/analyze/analysis-dsl/group-executor.ts';
import { isLocalTarget } from '../src/analyze/analysis-dsl/local-executor.ts';
import { localChannelGroupRequest, localRequest, type LocalExtractionPlan } from '../src/analyze/analysis-dsl/local-request.ts';
import { boardChannelPlan } from '../src/analyze/board-channel-plan.ts';
import type { ChannelTerminalRequest } from '../src/analyze/channel-terminals.ts';
import { channelTopology, validateChannelOwnership } from '../src/analyze/segmented-channel.ts';
import { segmentedCoupon, segmentedCouponDsl, segmentedCouponVariant } from './helpers/pcb-analysis-segmented-coupon.ts';

async function plan(board: any, terminals: ChannelTerminalRequest) {
  let result: ReturnType<typeof boardChannelPlan> | undefined;
  const output = await executeScenario(
    { board, code: segmentedCouponDsl.split('a.plot(')[0] },
    { filename: 'multiport-geometry.js', maxEntries: 1000, maxOutputBytes: 4 * 1024 * 1024 },
    () => {},
    async task => {
      result = boardChannelPlan(board, localRequest(task as any), terminals);
      return { id: 'plan', status: 'complete', method: 'Physical port geometry only', diagnostics: [], assumptions: [], metrics: {} };
    },
  );
  assert.equal(output.status, 'complete', JSON.stringify(output.error));
  assert.ok(result);
  return result;
}

const terminal = (id: string, padIndex: number, layer: string) => ({ id, sourcePath: 'pads[' + padIndex + ']', layer });

test('parallel signal and return islands retain distinct conductors and exact oriented ports', async () => {
  const board = segmentedCoupon();
  board.nets.push({ name: 'VICTIM' }, { name: 'OTHER_RETURN' });
  for (const [index, y, net] of [[0, .18, 'VICTIM'], [1, .18, 'OTHER_RETURN']] as const) {
    const track = structuredClone(board.copper.fixed.tracks[index]);
    track.id = net;
    track.net = net;
    track.widthMm = .1;
    for (const point of track.points) point.y = y;
    board.copper.fixed.tracks.push(track);
  }
  board.copper.fixed.tracks[1].widthMm = .1;
  for (const pad of board.pads.slice(2)) pad.shape.heightMm = .1;
  for (const [index, original] of board.pads.slice().entries()) {
    const pad = structuredClone(original);
    pad.id += '-other';
    pad.number = index < 2 ? '3' : '4';
    pad.at.y = .18;
    pad.net = index < 2 ? 'VICTIM' : 'OTHER_RETURN';
    board.pads.push(pad);
  }
  const terminals = {
    signals: [terminal('s1', 1, 'TOP'), terminal('s0', 0, 'TOP'), terminal('v1', 5, 'TOP'), terminal('v0', 4, 'TOP')],
    references: [terminal('r0', 2, 'BOTTOM'), terminal('r1', 3, 'BOTTOM'), terminal('w0', 6, 'BOTTOM'), terminal('w1', 7, 'BOTTOM')],
    ports: [
      { name: 'victim-reversed', positive: 'w1', negative: 'v1' },
      { name: 'driver', positive: 's0', negative: 'r0' },
      { name: 'victim-in', positive: 'v0', negative: 'w0' },
      { name: 'receiver', positive: 's1', negative: 'r1' },
      { name: 'differential', positive: 'v1', negative: 's1' },
    ],
  };
  const result = await plan(board, terminals);
  assert.deepEqual(validateChannelOwnership(result), [1000, 10000]);
  assert.equal(result.audit.signalIslands.length, 2);
  assert.equal(result.audit.returnIslands.length, 2);
  assert.deepEqual(result.ports, terminals.ports.map(({ name, positive, negative }) => ({
    name, cut: { positive: 'external:' + positive, negative: 'external:' + negative },
  })));
  const sections = result.blocks.filter(b => b.kind === 'section');
  assert.ok(sections.length);
  for (const section of sections) {
    assert.equal(section.cuts.length, 6);
    for (const end of section.sectionEnds!) assert.equal(new Set(Object.values(end)).size, 4);
  }
  const topology = channelTopology(result);
  assert.deepEqual(topology.ports.map(p => p.name), terminals.ports.map(p => p.name));
  assert.ok(topology.connectionContacts.includes('negative-positive') || topology.connectionContacts.includes('positive-negative'));
});

test('three physical endpoints on a branch receive distinct external ports', async () => {
  const board = segmentedCouponVariant('branch');
  const branch = { x: 5, y: 2 };
  board.components.push({ ...board.components[1], designator: 'J2', at: branch });
  for (const pad of [board.pads[1], board.pads[3]]) {
    board.pads.push({ ...pad, id: pad.id + '-branch', component: 'J2', at: branch, rotationDeg: 90 });
  }
  const terminals = {
    signals: [terminal('s0', 0, 'TOP'), terminal('s1', 1, 'TOP'), terminal('s2', 4, 'TOP')],
    references: [terminal('r0', 2, 'BOTTOM'), terminal('r1', 3, 'BOTTOM'), terminal('r2', 5, 'BOTTOM')],
    ports: [2, 0, 1].map(i => ({ name: 'p' + i, positive: 's' + i, negative: 'r' + i })),
  };
  const result = await plan(board, terminals);
  assert.deepEqual(validateChannelOwnership(result), [1000, 10000]);
  assert.equal(result.audit.signalIslands.length, 1);
  assert.ok(result.blocks.filter(b => b.kind === 'section').length >= 2);
  assert.deepEqual(channelTopology(result).ports.map(p => p.name), ['p2', 'p0', 'p1']);
  assert.equal(new Set(result.audit.physicalTerminals.map((t: { owner: string }) => t.owner)).size, 3);
});
test('a shared terminal is never duplicated into a separate external block', async () => {
  const board = segmentedCoupon();
  const terminals = {
    signals: [terminal('s0', 0, 'TOP'), terminal('s1', 1, 'TOP')],
    references: [terminal('r0', 2, 'BOTTOM')],
    ports: [
      { name: 'local', positive: 's0', negative: 'r0' },
      { name: 'remote', positive: 's1', negative: 'r0' },
    ],
  };
  await assert.rejects(() => plan(board, terminals), /External port spans separate physical blocks: remote/);
});
function differentialCoupon() {
  const board = segmentedCouponVariant('coupled');
  for (const original of board.pads.slice(0, 2)) {
    const pad = structuredClone(original);
    pad.id += '-negative';
    pad.number = '3';
    pad.net = 'VICTIM';
    pad.at.y = .18;
    board.pads.push(pad);
  }
  const code = segmentedCouponDsl.split('a.plot(')[0].replace('const channel=a.channel({from,to});', `
const positive = a.channel({from,to});
const negative = a.channel({
  from: a.port({positive:pad('J0','3'),negative:pad('J0','2'),positiveLayer:'TOP',negativeLayer:'BOTTOM'}),
  to: a.port({positive:pad('J1','3'),negative:pad('J1','2'),positiveLayer:'TOP',negativeLayer:'BOTTOM'}),
});
const channel = a.differential({positive,negative});
`);
  return { board, code };
}

async function planDifferential(board: any, code: string) {
  let result: ReturnType<typeof boardChannelPlan> | undefined;
  const output = await executeScenario(
    { board, code },
    { filename: 'differential-geometry.js', maxEntries: 1000, maxOutputBytes: 4 * 1024 * 1024 },
    () => {},
    async task => {
      assert.ok(isLocalTarget(task));
      let terminalRequest: ChannelTerminalRequest | undefined;
      const geometry = localRequest(task as AnalysisTask<'impedance'>, plan => {
        terminalRequest = plan.terminalRequest;
        assert.equal(plan.confirmedLocalBarrel, false);
      });
      assert.ok(terminalRequest);
      assert.equal(terminalRequest.signals.length, 4);
      assert.equal(terminalRequest.references.length, 2);
      assert.equal(terminalRequest.ports[0].negative, terminalRequest.ports[2].negative);
      assert.equal(terminalRequest.ports[1].negative, terminalRequest.ports[3].negative);
      assert.notEqual(terminalRequest.ports[0].negative, terminalRequest.ports[1].negative);
      result = boardChannelPlan(board, geometry, terminalRequest);
      return { id: 'plan', status: 'complete', method: 'Differential geometry only', diagnostics: [], assumptions: [], metrics: {} };
    },
  );
  return { output, result };
}

test('the existing layered differential target prepares one joint four-port geometry', async () => {
  const { board, code } = differentialCoupon();
  const { output, result } = await planDifferential(board, code);
  assert.equal(output.status, 'complete', JSON.stringify(output.error));
  assert.ok(result);
  assert.deepEqual(validateChannelOwnership(result), [1000, 10000]);
  assert.deepEqual(result.ports.map(p => p.name), ['positive-input', 'positive-output', 'negative-input', 'negative-output']);
  assert.equal(result.audit.signalIslands.length, 2);
  assert.ok(result.blocks.some(block => block.kind === 'section' && block.cuts.length === 4));
});

test('the differential adapter checks both signal legs for confirmed copper continuity', async () => {
  const { board, code } = differentialCoupon();
  const victim = board.copper.fixed.tracks.at(-1);
  victim.points[1].x = 4;
  const { output } = await planDifferential(board, code);
  assert.equal(output.status, 'failed');
  assert.match(JSON.stringify(output.error), /physically disconnected on leg 2/);
});
test('the existing SI channel list prepares one joint board group with physical port bindings', async () => {
  const { board, code } = differentialCoupon();
  const prefix = code.slice(0, code.indexOf('const run='));
  for (const members of ['[positive,negative]', '[channel,positive]']) {
    const source = prefix + `
const driver = a.model({kind:'linear-driver',lowV:0,highV:1,outputResistanceOhm:50});
const receiver = a.model({kind:'rc-input',capacitanceF:1e-12});
const stimulus = {kind:'bits',bits:'01',bitRateBps:1e3,riseS:1e-4,fallS:1e-4,repeatCount:1};
await a.si().channels(${members}).stackup(layers)
  .materials({cu:{conductivitySM:58e6,state:'Explicit copper'},d:{relativePermittivity:4,state:'Explicit dielectric'}})
  .drivers([{port:from,model:driver,stimulus}]).receivers([{port:to,model:receiver}])
  .time({stopS:2e-3,sampleStepS:1e-5}).run();
`;
    const output = await executeScenario(
      { board, code: source },
      { filename: 'channel-group.js', maxEntries: 1000, maxOutputBytes: 4 * 1024 * 1024 },
      () => {},
      async task => {
        let prepared: LocalExtractionPlan | undefined;
        const geometry = localChannelGroupRequest(task as AnalysisTask<'si'>, plan => { prepared = plan; });
        assert.ok(prepared?.terminalRequest);
        const result = boardChannelPlan(board, geometry, prepared.terminalRequest);
        assert.deepEqual(validateChannelOwnership(result), [1000, 10000]);
        assert.equal(result.audit.signalIslands.length, 2);
        assert.ok(result.blocks.some(block => block.kind === 'section' && block.cuts.length === 4));
        assert.equal(result.ports.length, 4);
        assert.equal(geometry.ports.length, 4);
        assert.equal(new Set(prepared.portHandles!.map(binding => binding.handleId)).size, 4);
        assert.deepEqual(new Set(prepared.portHandles!.map(binding => binding.portName)), new Set(result.ports.map(port => port.name)));
        return { id: 'plan', status: 'complete', method: 'Group geometry only', diagnostics: [], assumptions: [], metrics: {} };
      },
    );
    assert.equal(output.status, 'complete', JSON.stringify(output.error));
  }
});
test('SI board-group run keeps full DDR demand and one joint passive extraction', async () => {
  const { board, code } = differentialCoupon();
  const source = code.slice(0, code.indexOf('const run=')) + `
const driver = a.model({kind:'linear-driver',lowV:0,highV:1,outputResistanceOhm:50});
const receiver = a.model({kind:'rc-input',capacitanceF:1e-12});
const stimulus = {kind:'bits',bits:'01',bitRateBps:2.5e3,riseS:2e-10,fallS:2e-10,repeatCount:1};
const builder = a.si().channels([channel,positive]).stackup(layers)
  .materials({cu:{conductivitySM:58e6,state:'Explicit copper'},d:{relativePermittivity:4,state:'Explicit dielectric'}})
  .receivers([{port:to,model:receiver}]);
const result = await builder.drivers([{port:from,model:driver,stimulus}])
  .time({stopS:8e-4,sampleStepS:1e-11}).run();
if (result.voltage(to).validity.status !== 'unavailable') throw Error('unexpected waveform');
const model = result.solution.network;
if (model.analyses.join(',') !== 'networkParameters') throw Error('unexpected network admission');
a.network({model,ports:Object.fromEntries(model.ports.map(port => [port,port]))});
`;
  let calls = 0;
  const group = createGroupExecutor((captured, geometry, request, axis, budget, demand) => {
    calls++;
    assert.deepEqual(captured, board);
    assert.ok(!Array.isArray(request));
    assert.equal(request.ports.length, 4);
    assert.equal(geometry.ports.length, 4);
    assert.equal(request.ports[0].negative, request.ports[2].negative);
    assert.notEqual(request.ports[0].negative, request.ports[1].negative);
    assert.ok(demand && demand[0] === 0 && demand[1] > 1e9);
    assert.deepEqual(axis, [1000, 2500, 1 / (8e-4 - (4e-4 + 2e-10)), 100000]);
    assert.ok(budget.maxSeconds > 0);
    const ports = request.ports.map(({ name }, index) => ({ name, index }));
    const parameters = axis.map(() => ports.map(() => ports.map(() => ({ real: 0, imaginary: 0 }))));
    return {
      result: {
        status: 'complete', diagnostics: [], elapsedMs: 0, ports,
        assembledModel: { format: 'network-parameters', representation: 'S', nature: 'passive',
          referenceTopology: 'common', referenceOhm: 50, frequenciesHz: axis, parameters },
      },
      bandHz: [1000, 100000], audit: {}, blocks: [], evidence: [], verification: [], ownership: [],
      wallSeconds: 0, magneticCoupling: null,
    };
  });
  const options = { filename: 'channel-group-runtime.js', maxEntries: 1000, maxOutputBytes: 4 * 1024 * 1024 };
  const executor = createAnalysisExecutor(group);
  const output = await executeScenario({ board, code: source }, options, () => {}, executor);
  assert.equal(output.status, 'complete', JSON.stringify(output.error));
  assert.equal(calls, 1);
  const records = output.reports[0].calculations as any[];
  assert.equal(records[0].preparation.executorAvailable, true);
  const result = records[1].result;
  assert.equal(result.status, 'partial');
  assert.equal(result.solution.temporal.status, 'unavailable');
  assert.equal(result.solution.status, 'unverified');
  assert.equal(result.solution.requestedIntervalHz[0], 0);
  assert.equal(result.solution.requestedIntervalHz[1], result.solution.drivers[0].spectrum.requiredUpperHz);
  assert.deepEqual(result.solution.applicabilityBandHz, [1000, 100000]);
  assert.deepEqual(result.solution.uncovered.map((gap: any) => gap.intervalHz), [
    [0, 1000], [100000, result.solution.requestedIntervalHz[1]],
  ]);
  assert.equal(result.solution.portHandles.length, 6);
  assert.equal(new Set(result.solution.portHandles.map((p: any) => p.portName)).size, 4);
  assert.equal(result.solution.drivers[0].ports.port.portName, 'channel-1-positive-input');
  assert.equal(result.solution.receivers[0].ports.port.portName, 'channel-1-positive-output');
  assert.equal(result.solution.network.kind, 'network');
  assert.equal(result.solution.network.resourceId, result.id);
  assert.match(result.solution.network.sha256, /^[0-9a-f]{64}$/);

  const missing = await executeScenario({
    board, code: source.slice(0, source.indexOf('const result =')) + 'await builder.run();',
  }, options, () => {}, executor);
  assert.equal(missing.status, 'failed');
  assert.equal(calls, 1);
  assert.deepEqual(missing.error?.diagnostics.filter(d => d.code === 'MISSING_FIELD').map(d => d.path), ['drivers', 'time']);
});
