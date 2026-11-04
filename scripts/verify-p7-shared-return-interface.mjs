import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const marker = 'P7_SHARED_RETURN_INTERFACE ';
const residualLimit = 1e-8;
const ports = [['p0', 'p2'], ['p1', 'p3']];

function count(value, minimum = 0) {
  assert.ok(Number.isSafeInteger(value) && value >= minimum);
  return value;
}

function residual(value) {
  assert.ok(Number.isFinite(value) && value >= 0 && value <= residualLimit);
}

function complex(value) {
  assert.ok(Array.isArray(value) && value.length === 2 && value.every(Number.isFinite));
  return value;
}

/** Verify the private native split/unsplit compact control from one saved run.
 * Its original two-port coupon is independent of the D1 board fixtures. */
export function verifySharedReturnInterface(log) {
  const lines = log.split(/\r?\n/).filter(line => line.startsWith(marker));
  assert.equal(lines.length, 1, 'Expected one successful native interface packet');
  const packet = JSON.parse(lines[0].slice(marker.length));
  assert.equal(packet.schema, 'p7-shared-return-interface-v1');
  assert.equal(packet.frequencyHz, 1000);
  assert.equal(packet.coverage, 'unverified');
  assert.deepEqual(packet.ports, ports);

  const sources = packet.sourceMapping;
  assert.ok(Array.isArray(sources) && sources.length > 0);
  for (const [index, layer] of ['TOP', 'TOP', 'BOTTOM', 'BOTTOM'].entries()) {
    assert.ok(sources.some(([, path, , physicalLayer]) => path === `pads[${index}]` && physicalLayer === layer));
  }
  assert.ok(sources.some(([, path, , layer]) => path.includes('zones[1]') && layer === 'BOTTOM'));

  const topology = packet.topology;
  assert.equal(topology.owners, 2);
  count(topology.originalCopperTetrahedra, 2);
  count(topology.plannedBytes, 1);
  const traceDofs = topology.tracePotentialDofs;
  assert.ok(Array.isArray(traceDofs) && traceDofs.length > 1);
  assert.equal(new Set(traceDofs.map(dof => count(dof))).size, traceDofs.length);
  const faces = topology.orientedCrossOwnerFaces;
  assert.ok(Array.isArray(faces) && faces.length >= 2);
  const faceKeys = new Set();
  for (const { nodes, outwardPositiveOwner, outwardNegativeOwner } of faces) {
    assert.deepEqual([outwardPositiveOwner, outwardNegativeOwner].sort(), [0, 1]);
    assert.ok(Array.isArray(nodes) && nodes.length === 3 && new Set(nodes).size === 3);
    const key = nodes.map(node => count(node)).sort((a, b) => a - b).join(',');
    assert.ok(!faceKeys.has(key), 'Duplicate cross-owner face');
    faceKeys.add(key);
  }
  const mask = count(packet.faceDirectionMask, 1);
  assert.equal(mask, 0b011, 'Expected both in-plane cut directions');

  assert.deepEqual(packet.splitParameters, packet.unsplitParameters);
  const [matrix] = packet.splitParameters;
  assert.equal(packet.splitParameters.length, 1);
  assert.equal(matrix.length, ports.length);
  for (const row of matrix) {
    assert.equal(row.length, ports.length);
    for (const { real, imaginary } of row) complex([real, imaginary]);
  }
  assert.ok(Math.hypot(matrix[0][1].real, matrix[0][1].imaginary) > 0, 'Expected cross-port response');

  assert.equal(packet.splitColumns.length, ports.length);
  assert.equal(packet.unsplitColumns.length, ports.length);
  let maxOwnerA = 0;
  let maxDisplacementA = 0;
  let maxTraceBalanceErrorA = 0;
  for (const [input, split] of packet.splitColumns.entries()) {
    const unsplit = packet.unsplitColumns[input];
    assert.equal(split.copperLossW, unsplit.copperLossW);
    const netPowerW = 50 / 4 * (1 - matrix.reduce((sum, row) => {
      const { real, imaginary } = row[input];
      return sum + real ** 2 + imaginary ** 2;
    }, 0));
    for (const column of [split, unsplit]) {
      assert.equal(column.input, input);
      residual(column.kclRelative);
      residual(column.kvlRelative);
      const lossW = column.copperLossW;
      assert.ok(Number.isFinite(lossW) && lossW > 0);
      const toleranceW = 1e-12 + 1e-8 * (Math.abs(netPowerW) + lossW);
      assert.ok(Number.isFinite(column.powerBalanceErrorW) && column.powerBalanceErrorW >= 0 &&
        column.powerBalanceErrorW <= toleranceW);
      assert.ok(Math.abs(netPowerW - lossW) <= toleranceW);
    }
    assert.equal(unsplit.spatialTrace, undefined);

    const { maxWeldRelative, maxKclRelative, ownerJouleLossW, traces } = split.spatialTrace;
    assert.ok(Array.isArray(ownerJouleLossW) && ownerJouleLossW.length === topology.owners);
    assert.ok(ownerJouleLossW.every(loss => Number.isFinite(loss) && loss > 0));
    const ownerLossW = ownerJouleLossW.reduce((sum, loss) => sum + loss, 0);
    assert.ok(Math.abs(ownerLossW - split.copperLossW) <=
      residualLimit * Math.max(ownerLossW, split.copperLossW, 1e-18));
    residual(maxWeldRelative);
    residual(maxKclRelative);
    assert.deepEqual(traces.map(trace => trace.potentialDof), traceDofs);
    for (const trace of traces) {
      complex(trace.voltageV);
      const displacement = complex(trace.commonDisplacementA);
      const injected = complex(trace.externalInjectionA);
      assert.deepEqual(trace.ownerDuals.map(dual => dual.owner).sort(), [0, 1]);
      const balance = displacement.map((a, phase) => a - injected[phase]);
      for (const { weakConductionA } of trace.ownerDuals) {
        const conduction = complex(weakConductionA);
        maxOwnerA = Math.max(maxOwnerA, Math.hypot(...conduction));
        balance[0] += conduction[0];
        balance[1] += conduction[1];
      }
      maxDisplacementA = Math.max(maxDisplacementA, Math.hypot(...displacement));
      maxTraceBalanceErrorA = Math.max(maxTraceBalanceErrorA, Math.hypot(...balance));
    }
  }
  assert.ok(maxOwnerA > 1e-4 && maxDisplacementA > 0);
  // Rust and JS implementations of hypot may differ by a few ulps.
  for (const [measured, reported] of [
    [maxOwnerA, packet.maxOwnerWeakConductionA],
    [maxDisplacementA, packet.maxCommonDisplacementA],
  ]) {
    assert.ok(Number.isFinite(reported) && Math.abs(measured / reported - 1) <= 1e-12);
  }
  return {
    fixture: 'Compact continuous-return native control', coverage: packet.coverage,
    frequencyHz: packet.frequencyHz, ports, owners: topology.owners, traceDofs: traceDofs.length,
    crossOwnerFaces: faces.length, faceDirectionMask: mask, maxOwnerA, maxDisplacementA,
    maxTraceBalanceErrorA,
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const [log] = process.argv.slice(2);
  assert.ok(log, 'Usage: node scripts/verify-p7-shared-return-interface.mjs <native-test.log>');
  console.log(JSON.stringify(verifySharedReturnInterface(readFileSync(log, 'utf8')), null, 2));
}
