import test from 'node:test';
import assert from 'node:assert/strict';
import { createBoardChannelExecutor } from '../src/analyze/board-channel-executor.ts';
import { boardChannelPlan } from '../src/analyze/board-channel-plan.ts';
import { executeScenario } from '../src/analyze/analysis-dsl/scenario.ts';
import { localRequest, inferContactLayer } from '../src/analyze/analysis-dsl/local-request.ts';
import { capturedBoardGeometry } from '../src/analyze/geometry-snapshot.ts';
import { segmentedCoupon, segmentedCouponDsl } from './helpers/pcb-analysis-segmented-coupon.ts';

test('automatic extraction retains separate remote reference islands when a signal corridor has no finite return', async () => {
  const board = segmentedCoupon();
  board.outline = [{x:-1,y:-1.6},{x:11,y:-1.6},{x:11,y:1.6},{x:-1,y:1.6}];
  board.copper.fixed.tracks.pop();
  for (const pad of board.pads.filter((pad: {net:string}) => pad.net === 'RETURN')) pad.at.y = 1.1;
  const captured = capturedBoardGeometry(JSON.stringify(board));
  const islands = board.pads.map((_: unknown, i: number) => {
    const id = captured.regions.find(region => region.source.path === 'pads[' + i + ']')!.id;
    return captured.islands.find(island => island.region_ids.includes(id))!;
  });
  assert.equal(islands[0].id, islands[1].id);
  assert.notEqual(islands[2].id, islands[3].id);
  const code = segmentedCouponDsl.split('a.plot(')[0];
  let calls = 0;
  const output = await executeScenario({board,code}, {filename:'remote-reference.js',maxEntries:1000,maxOutputBytes:4*1024*1024}, () => {}, async task => {
    let contacts: Parameters<typeof boardChannelPlan>[2] = [];
    const geometry = localRequest(task as any, plan => { contacts = plan.contacts; });
    assert.throws(() => boardChannelPlan(board, geometry, contacts), {
      code: 'CHANNEL_CORRIDOR_UNSUPPORTED', geometryReason: 'no-finite-return-corridor',
    });
    const solve = createBoardChannelExecutor(undefined, () => assert.fail('segment solve selected'), (plan, frequencies, budget) => {
      calls++;
      assert.deepEqual(frequencies, [1000, 3000]);
      assert.equal(budget.maxMemoryMiB, 256);
      assert.deepEqual(plan.electrodePairs.map(pair => pair.map(name =>
        (plan.geometry.electrodes as {name:string;sourcePath:string}[]).find(electrode => electrode.name === name)!.sourcePath
      )), [['pads[0]', 'pads[2]'], ['pads[1]', 'pads[3]']]);
      assert.deepEqual(plan.sourceCopper.map(source => source.regionId).sort(),
        [...new Set(islands.flatMap((island: typeof islands[number]) => island.region_ids))].sort());
      assert.equal(plan.geometry.allowInsulatedCopperCuts, false);
    });
    solve(board, geometry, contacts, [1000, 3000], {maxSeconds:5,maxMemoryMiB:256});
    return {id:'plan',status:'complete',method:'Geometry dispatch only',diagnostics:[],assumptions:[],metrics:{}};
  });
  assert.equal(output.status, 'complete', JSON.stringify(output.error));
  assert.equal(calls, 1);
});

test('contact layer inference uses the single pad layer, then component side, and keeps explicit or unresolved cases strict', () => {
  const board: any = {
    pads: [
      { id: 'a', component: 'U5', number: '7', layers: ['TOP'] },
      { id: 'b', component: 'U5', number: '5', layers: ['TOP', 'BOTTOM'] },
      { id: 'c', component: 'J1', number: '1', layers: ['TOP', 'BOTTOM'] },
    ],
    components: [{ designator: 'U5', side: 'bottom' }, { designator: 'J1', side: 'top' }],
  };
  assert.equal(inferContactLayer(board, { kind: 'pad', designator: 'U5', number: '7' })?.layer, 'TOP');
  assert.equal(inferContactLayer(board, { kind: 'pad', designator: 'U5', number: '5' })?.layer, 'BOTTOM');
  assert.equal(inferContactLayer(board, { kind: 'padId', id: 'c' })?.layer, 'TOP');
  assert.equal(inferContactLayer(board, { kind: 'pad', designator: 'U9', number: '1' }), undefined);
});
