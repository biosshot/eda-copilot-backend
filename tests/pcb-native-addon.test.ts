import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { join, resolve, sep } from 'node:path';
import test from 'node:test';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import type { AnalysisImportReport } from '../native/node-addon/index.cjs';

const require = createRequire(import.meta.url);
// Mandatory integration check: missing/stale binaries fail with rebuild guidance.
const addon = loadNativeBoardPacker();
assert.equal(typeof addon.importAnalysisBoard, 'function', 'Shared native addon is stale; run npm run native:build');
const previousExports = [
    'blockContractVersion', 'blockSearchStagesVersion', 'boardElectricalRegression', 'compareBlockQuality',
    'contractVersion', 'evaluateBlockQuality', 'evaluateBoardQuality', 'evaluateIslandQuality', 'evaluateSignalPath',
    'numericContract', 'passiveIslandContractVersion', 'placementAccessProfile', 'placementAlignmentPolicy',
    'placementBlockSimilarity', 'placementGeometryEpsilon', 'placementLayerPenalties', 'placementNetClass',
    'placementNetWeights', 'placementOrientationWeight', 'placementPriorityWeight', 'placementRelationProfile',
    'placementScoreContractVersion', 'placementScoreSum', 'placementSpacingPolicy', 'postPlaceNetLengths',
    'postPlaceRefineContractVersion', 'postPlaceScoreContractVersion', 'refinePostPlacement', 'scoreBoardAlignment',
    'scoreBoardSpacing', 'scoreFastRouteLayout', 'scoreIslandQuality', 'scorePlacementOrientation', 'scorePostPlace',
    'signalPathBridgeDeltas', 'signalPathContractVersion', 'solveBlockPrimitives', 'solveBlockPrimitivesBatch',
    'solveBoardPacked', 'solvePassiveNetIsland', 'validatePlacement', 'validatePlacementChange',
];

function rustReports(paths: string[]): AnalysisImportReport[] {
    const result = spawnSync('cargo', ['run', '--quiet', '--locked', '--manifest-path', 'native/analyze/Cargo.toml', '--example', 'import', '--', ...paths], {
        cwd: resolve('.'), encoding: 'utf8', windowsHide: true, timeout: 180_000, maxBuffer: 16 * 1024 * 1024,
    });
    assert.ifError(result.error);
    assert.ok(result.status === 0 || result.status === 1, result.stderr);
    const reports = result.stdout.trim().split(/\r?\n/).filter(Boolean).map(line => {
        const { file: _file, ...report } = JSON.parse(line);
        return report as AnalysisImportReport;
    });
    assert.equal(reports.length, paths.length, result.stderr);
    assert.equal(result.status, Number(reports.some(r => !r.imported)), result.stderr);
    return reports;
}

test('single addon retains all production exports, versions and matching CommonJS declarations', () => {
    assert.deepEqual(Object.keys(addon).sort(), [...previousExports, 'importAnalysisBoard', 'prepareAnalysisGeometry'].sort());
    const declaration = readFileSync('native/pcb-board-packer/index.d.ts', 'utf8') + readFileSync('native/analyze/index.d.ts', 'utf8');
    const aggregate = readFileSync('native/node-addon/index.d.cts', 'utf8');
    assert.match(aggregate, /export \* from '\.\.\/pcb-board-packer\/index\.js'/);
    assert.match(aggregate, /export \* from '\.\.\/analyze\/index\.js'/);
    const declared = [...declaration.matchAll(/export declare function (\w+)\(/g)].map(m => m[1]);
    assert.deepEqual(declared.sort(), Object.keys(addon).sort());
    assert.deepEqual([
        addon.contractVersion(), addon.blockContractVersion(), addon.passiveIslandContractVersion(),
        addon.placementScoreContractVersion(), addon.postPlaceScoreContractVersion(), addon.postPlaceRefineContractVersion(),
        addon.signalPathContractVersion(), addon.blockSearchStagesVersion!(), addon.numericContract!(), addon.placementGeometryEpsilon(),
    ], [7, 4, 1, 13, 1, 3, 1, 1, 'f32-rte-ftz-v1', Math.fround(.001)]);
    assert.throws(() => addon.solveBoardPacked({} as never), { code: 'InvalidArg', message: 'Invalid native board pack problem: missing field `version`' });
    assert.throws(() => addon.placementRelationProfile({} as never), { code: 'GenericFailure' });
    assert.throws(() => addon.importAnalysisBoard(123 as never), { code: 'StringExpected' });
});

test('eight saved PCBs have identical Node/Rust accepted counts and complete diagnostics', () => {
    const root = resolve('tests/pcb-analyze');
    const paths = readdirSync(root, { withFileTypes: true }).filter(e => e.isDirectory())
        .map(e => join(root, e.name, `${e.name}.json`)).sort();
    assert.equal(paths.length, 8);
    const expected = rustReports(paths);
    for (const [i, path] of paths.entries()) {
        const report = addon.importAnalysisBoard(readFileSync(path, 'utf8'));
        assert.deepEqual(report, expected[i], path);
        assert.equal(report.imported, true, path);
        assert.equal('board' in report, false);
        assert.ok(report.diagnostics.some(d => d.code === 'stackup_missing'));
    }
});

test('negative inputs and unavailable fill preserve Rust diagnostics without inventing stackup or copper', t => {
    const directory = mkdtempSync(join(tmpdir(), 'eda-native-import-'));
    t.after(() => {
        assert.ok(resolve(directory).startsWith(resolve(tmpdir()) + sep));
        rmSync(directory, { recursive: true, force: true });
    });
    const base = JSON.parse(readFileSync('docs/experimental/analyze/routing-board-v2-example.json', 'utf8'));
    const unknownField = { ...base, unknownField: true };
    const unknownLayer = structuredClone(base);
    unknownLayer.copper.fixed.zones[0].layers[0] = 'MISSING';
    const unavailable = structuredClone(base);
    delete unavailable.stackup;
    unavailable.copper.fixed.zones[0].filledPolygons = [{ layer: 'top', source: 'cad', status: 'failed', reason: 'fixture has no native fill' }];
    const texts = ['{', JSON.stringify(unknownField), JSON.stringify(unknownLayer), JSON.stringify(unavailable)];
    const paths = texts.map((s, i) => { const p = join(directory, `${i}.json`); writeFileSync(p, s); return p; });
    const expected = rustReports(paths);
    const actual = texts.map(s => addon.importAnalysisBoard(s));
    assert.deepEqual(actual, expected);
    assert.deepEqual(actual.map(r => r.imported), [false, false, false, true]);
    assert.ok(actual[2].diagnostics.some(d => d.code === 'unknown_layer' && d.source_id === 'pour-gnd-top'));
    assert.ok(actual[3].diagnostics.some(d => d.code === 'cad_fill_unavailable' && d.severity === 'missing'));
    assert.equal(actual[3].accepted.unavailable_fills, 1);
});

test('packer -> analyze -> packer retains F32 FTZ and independent F64 subnormal geometry', () => {
    const board = JSON.parse(readFileSync('docs/experimental/analyze/routing-board-v2-example.json', 'utf8'));
    // Area is a nonzero F64 subnormal. A leaked packer FTZ guard makes it zero.
    board.outline = [{ x: 0, y: 0 }, { x: 1e-160, y: 0 }, { x: 0, y: 1e-160 }];
    const text = JSON.stringify(board);
    const before = addon.importAnalysisBoard(text);
    assert.equal(before.imported, true, JSON.stringify(before.diagnostics));
    assert.equal(addon.placementScoreSum([2 ** -149]), 0);
    assert.deepEqual(addon.importAnalysisBoard(text), before);
    assert.equal(addon.placementScoreSum([1, 2 ** -24, 2 ** -24]), 1);
    assert.deepEqual(addon.importAnalysisBoard(text), before);
});

test('analysis import loads and runs with GPU disabled or both GPU drivers unavailable', () => {
    const path = resolve('tests/pcb-analyze/BandAmpV5/BandAmpV5.json');
    const loader = require.resolve('../native/node-addon/index.cjs');
    const script = `const a=require(${JSON.stringify(loader)}); process.stdout.write(JSON.stringify(a.importAnalysisBoard(require('node:fs').readFileSync(${JSON.stringify(path)},'utf8'))));`;
    for (const options of [
        { PCB_BLOCK_GPU_DISABLED: '1' },
        { PCB_GPU_RUNTIME: 'auto', PCB_GPU_CUDA_DISABLED: '1', VK_DRIVER_FILES: join(tmpdir(), 'absent-analyze-vulkan-driver.json'), VK_ICD_FILENAMES: join(tmpdir(), 'absent-analyze-vulkan-driver.json') },
    ]) {
        const result = spawnSync(process.execPath, ['-e', script], {
            env: { ...process.env, PCB_BLOCK_BACKEND: 'cubecl', PCB_BOARD_BACKEND: 'cubecl', PCB_POST_PLACE_BACKEND: 'cubecl', PCB_BLOCK_SOLVER_PROFILE: '1', ...options },
            encoding: 'utf8', windowsHide: true, timeout: 30_000,
        });
        assert.ifError(result.error);
        assert.equal(result.status, 0, result.stderr);
        assert.deepEqual(JSON.parse(result.stdout), addon.importAnalysisBoard(readFileSync(path, 'utf8')));
        assert.doesNotMatch(result.stderr, /gpu|cuda|vulkan|cubecl/i);
    }
});
