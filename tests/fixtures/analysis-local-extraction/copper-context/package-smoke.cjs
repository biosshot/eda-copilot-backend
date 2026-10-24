const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../../../..');
const native = require(path.join(root, 'native/node-addon/index.cjs'));
for (const name of ['importAnalysisBoard', 'prepareAnalysisGeometry', 'AnalysisDcSession', 'AnalysisElectricalSession']) {
  if (typeof native[name] !== 'function') throw new Error(`Missing ABI export ${name}`);
}
const board = JSON.parse(fs.readFileSync(path.join(root, 'tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json')));
const frozen = JSON.parse(fs.readFileSync(path.join(__dirname, '../copper-geometry/regressions.json')));
board.pads = [];
for (const owner of ['fixed', 'editable']) {
  board.copper[owner].vias = [];
  board.copper[owner].zones = [];
  board.copper[owner].tracks = owner === 'fixed' ? frozen.tracks : [];
}
const report = native.prepareAnalysisGeometry(JSON.stringify(board));
if (report.operation !== 'prepare-geometry' || report.units !== 'mm' || report.coordinates !== 'absolute-y-down') throw new Error('Changed geometry contract');
if (report.excluded.length || report.regions.length !== 5) throw new Error('Frozen sources missing');
const ids = report.regions.map(r => r.source.source_id).sort();
if (JSON.stringify(ids) !== JSON.stringify(frozen.tracks.map(t => t.id).sort())) throw new Error('Lost source identities');
const result = { exportsPresent: true, geometryStatus: report.status, regions: report.regions.length, ids, booleanErrorBoundMm: report.boolean_error_bound_mm, placementEpsilonF32: native.placementGeometryEpsilon() };
fs.writeFileSync(path.join(root, 'debugging/p7-design-checkpoint/copper-context/package-smoke.json'), JSON.stringify(result, null, 2));
console.log(JSON.stringify(result));
