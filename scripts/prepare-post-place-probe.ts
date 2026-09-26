import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runPcbLayoutDsl } from '../src/pcb-layout/pcb-layout-dsl/spec.ts';
import { LayoutRulesSchema } from '../src/types/pcb/layout-rules.ts';
import { buildPlacementInput } from '../src/pcb-layout/placement-input.ts';
import { applyExistingBoard, applyExistingComponentPlacements, ensurePreservedComponentBlocks } from '../src/pcb-layout/existing-placement.ts';
import type { Placement } from '../src/types/pcb/layout-model.ts';

const dir = resolve('../pcb/portablescope-placement');
const snapshot = JSON.parse(readFileSync(resolve(dir, 'input.json'), 'utf8'));
const rules = ensurePreservedComponentBlocks(snapshot.circuit, applyExistingBoard(
    LayoutRulesSchema().parse(runPcbLayoutDsl(readFileSync(resolve(dir, 'placement.js'), 'utf8'))), snapshot.existingPlacement), snapshot.existingPlacement);
const input = applyExistingComponentPlacements(await buildPlacementInput(snapshot.circuit, rules), rules.preserve, snapshot.existingPlacement);
const svg = readFileSync('C:/Users/kiril/AppData/Local/Temp/easyeda-copilot-mcp/pcb-previews/d15dd74c-45d8-40c4-af10-1385bc8e0232.svg', 'utf8');
const labels = [...svg.matchAll(/<text x="([^"]+)" y="([^"]+)" font-size="11"[^>]*>([A-Z][A-Z0-9]*)<\/text>\s*<text[^>]*>r([0-9.]+) (top|bottom)<\/text>/g)];
const anchor = labels.find(m => m[3] === 'RF1');
const fixed = snapshot.existingPlacement.components.find((p: Placement) => p.designator === 'RF1');
if (!anchor || !fixed) throw new Error('Cannot calibrate SVG coordinates from preserved RF1');
const originX = fixed.x - Number(anchor[1]) / 12;
const originY = fixed.y + 0.1 - Number(anchor[2]) / 12;
const poses = new Map(labels.map(m => [m[3], { designator: m[3], x: Number(m[1])/12 + originX,
    y: Number(m[2])/12 + originY - 0.1, rotate: Number(m[4]), layer: m[5], score: 0 }]));
const placements = input.components.map(c => { const p = poses.get(c.designator); if (!p) throw new Error(`Missing SVG pose: ${c.designator}`); return p; });
writeFileSync(resolve(dir, 'post-place-probe-input.json'), JSON.stringify({ input, placements,
    provenance: 'Older circuit snapshot (U6 display excluded), poses recovered from completed placement preview d15dd74c; no solver rerun.' }, null, 2));
console.log(JSON.stringify({ components: input.components.length, blocks: input.blocks.length, labels: labels.length, originX, originY }));
