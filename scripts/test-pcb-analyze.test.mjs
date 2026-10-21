import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, mkdirSync, writeFileSync, readFileSync, readdirSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { runPcbAnalyze } from './test-pcb-analyze.mjs';

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'pcb-analyze-runner-'));
  t.after(() => {
    assert.ok(resolve(root).startsWith(resolve(tmpdir(), 'pcb-analyze-runner-')));
    rmSync(root, { recursive: true, force: true });
  });
  symlinkSync(fileURLToPath(new URL('../node_modules', import.meta.url)), join(root, 'node_modules'), 'junction');
  writeFileSync(join(root, 'package.json'), '{"type":"module"}');
  const board = join(root, 'tests', 'pcb-analyze', 'Board');
  mkdirSync(board, { recursive: true });
  writeFileSync(join(board, 'fixture.json'), '{}');
  return { root, board, runs: join(root, 'debugging', 'pcb-analyze', 'Board', 'runs') };
}

test('data-only fixtures list successfully, missing named analyses fail without fake runs', t => {
  const f = fixture(t);
  assert.equal(runPcbAnalyze(['--list'], f.root), 0);
  assert.equal(runPcbAnalyze(['Board', '--name', 'dc'], f.root), 1);
  assert.equal(existsSync(f.runs), false);
  assert.throws(() => runPcbAnalyze(['Board'], f.root), /--name/);
  assert.throws(() => runPcbAnalyze(['Board', '--name', '../dc'], f.root), /--name/);
  assert.throws(() => runPcbAnalyze(['../Board', '--name', 'dc'], f.root), /Unknown/);
});

test('named scripts select independently and repeated runs retain separate logs and output', t => {
  const f = fixture(t);
  const script = 'import {writeFileSync} from "node:fs"; import {join} from "node:path"; '
    + 'writeFileSync(join(process.env.PCB_ANALYZE_RUN_DIR,"result.json"),JSON.stringify({name:process.env.PCB_ANALYZE_NAME,fixture:process.env.PCB_ANALYZE_FIXTURE_DIR})); console.log("ran "+process.env.PCB_ANALYZE_NAME);';
  writeFileSync(join(f.board, 'dc.ts'), script);
  writeFileSync(join(f.board, 'emi.ts'), script);
  for (const name of ['dc', 'emi', 'dc.ts']) assert.equal(runPcbAnalyze(['Board', '--name', name], f.root), 0);
  const runs = readdirSync(f.runs);
  assert.equal(runs.length, 3);
  assert.equal(runs.filter(name => name.includes('-dc-')).length, 2);
  assert.equal(runs.filter(name => name.includes('-emi-')).length, 1);
  for (const name of runs) {
    const output = join(f.runs, name), manifest = JSON.parse(readFileSync(join(output, 'run.json'), 'utf8'));
    assert.equal(manifest.status, 'completed');
    assert.equal(manifest.scriptSha256.length, 64);
    assert.equal(JSON.parse(readFileSync(join(output, 'result.json'), 'utf8')).fixture, f.board);
    assert.match(readFileSync(join(output, 'run.log'), 'utf8'), new RegExp('ran ' + manifest.name));
  }
});

test('a failing script records failure and a batch continues with remaining fixtures', t => {
  const f = fixture(t);
  writeFileSync(join(f.board, 'dc.ts'), 'throw new Error("expected test failure");');
  const next = join(f.root, 'tests', 'pcb-analyze', 'Next'); mkdirSync(next);
  writeFileSync(join(next, 'fixture.json'), '{}'); writeFileSync(join(next, 'dc.ts'), 'console.log("next completed");');
  assert.equal(runPcbAnalyze(['--all', '--name', 'dc'], f.root), 1);
  const first = JSON.parse(readFileSync(join(f.runs, readdirSync(f.runs)[0], 'run.json'), 'utf8'));
  assert.equal(first.status, 'failed');
  const nextRuns = join(f.root, 'debugging', 'pcb-analyze', 'Next', 'runs');
  assert.equal(JSON.parse(readFileSync(join(nextRuns, readdirSync(nextRuns)[0], 'run.json'), 'utf8')).status, 'completed');
});

test('saved PCB inputs match their hashes and preserve the native capture inventories', () => {
  const directory = fileURLToPath(new URL('../tests/pcb-analyze/', import.meta.url));
  const names = readdirSync(directory).filter(name => existsSync(join(directory, name, 'fixture.json')));
  assert.ok(names.length > 0);
  const copper = layer => layer === 'TOP' || layer === 'BOTTOM' || /^INNER_\d+$/.test(layer);
  const ids = values => values.map(value => value.id).sort();
  const point = value => assert.ok(typeof value.x === 'number' && Number.isFinite(value.x)
    && typeof value.y === 'number' && Number.isFinite(value.y));
  const polygon = value => {
    for (const ring of [value.outer, ...(value.holes ?? [])]) {
      assert.ok(ring.length >= 3); ring.forEach(point);
    }
  };
  for (const name of names) {
    const fixtureDir = join(directory, name), read = file => JSON.parse(readFileSync(join(fixtureDir, file), 'utf8'));
    const fixture = read('fixture.json'), board = read(`${name}.json`), evidence = read('export-evidence.json');
    assert.equal(fixture.name, name);
    assert.equal(fixture.export.status, 'exported');
    assert.equal(fixture.verification.sourceRecordsUnchanged, true);
    for (const file of ['source.epro2', 'source-pcb.txt', 'export-evidence.json', `${name}.json`]) {
      const info = fixture.files[file], bytes = readFileSync(join(fixtureDir, file));
      const normalized = info.normalization === 'binary' ? bytes : Buffer.from(bytes.toString('utf8').replace(/\r\n/g, '\n'));
      assert.equal(normalized.length, info.bytes, `${name}/${file}: size`);
      assert.equal(createHash('sha256').update(normalized).digest('hex'), info.sha256, `${name}/${file}: hash`);
    }
    assert.equal(readFileSync(join(fixtureDir, 'source.epro2')).subarray(0, 4).toString('hex'), '504b0304');
    assert.deepEqual(evidence.diagnostics, fixture.export.diagnostics);
    const raw = evidence.capture.pcb, fixed = board.copper.fixed;
    assert.deepEqual(board.layers.map(layer => layer.name), raw.routing.layers);
    assert.deepEqual(ids(board.components), ids(raw.components));
    assert.deepEqual(ids(board.pads), ids(raw.pads.filter(p => p.shape && (copper(p.layer) || p.layer === 'MULTI'))));
    assert.deepEqual(ids(board.holes ?? []), ids(raw.pads.filter(p => !p.shape && p.hole && p.plated === false)));
    assert.equal(board.pads.filter(p => p.hole).length + (board.holes?.length ?? 0), raw.pads.filter(p => p.hole).length);
    assert.deepEqual(ids(fixed.vias), ids(raw.vias));
    const expectedTracks = [...raw.tracks, ...raw.arcs, ...raw.routing.polylines].filter(t => copper(t.layer));
    assert.deepEqual([...new Set(ids(fixed.tracks))], [...new Set(ids(expectedTracks))]);
    assert.deepEqual(ids(fixed.zones), ids([...raw.routing.pours.filter(p => copper(p.layer)),
      ...raw.polygons.filter(p => !p.pourId && copper(p.layer))]));
    const layers = new Set(raw.routing.layers), nets = new Set(board.nets.map(net => net.name));
    for (const pad of board.pads) { point(pad.at); pad.layers.forEach(layer => assert.ok(layers.has(layer))); }
    polygon({ outer: board.outline, holes: board.cutouts });
    for (const via of fixed.vias) {
      point(via.at); assert.ok(via.diameterMm > 0 && via.drillMm > 0);
      assert.ok(raw.routing.layers.indexOf(via.fromLayer) >= 0);
      assert.ok(raw.routing.layers.indexOf(via.toLayer) > raw.routing.layers.indexOf(via.fromLayer));
    }
    for (const track of fixed.tracks) { assert.ok(layers.has(track.layer) && track.widthMm > 0); track.points.forEach(point); }
    for (const zone of fixed.zones) {
      polygon(zone.outline);
      for (const fill of zone.filledPolygons) {
        assert.ok(layers.has(fill.layer));
        if (fill.status === 'available') fill.polygons.forEach(polygon);
      }
    }
    for (const object of [...board.pads, ...fixed.tracks, ...fixed.vias, ...fixed.zones]) {
      if (object.net) assert.ok(nets.has(object.net), `${name}: unknown net ${object.net}`);
    }
    assert.deepEqual(board.copper.editable, { tracks: [], vias: [], zones: [] });
  }
});
