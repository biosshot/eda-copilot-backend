// DSL/circuit checks only. This intentionally does not resolve footprints or run placement.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { runPcbLayoutDsl } from "../../../src/pcb-layout/pcb-layout-dsl/spec.ts";
import { applyExistingBoard, ensurePreservedComponentBlocks } from "../../../src/pcb-layout/existing-placement.ts";
import { validatePlacementRulesForCircuit } from "../../../src/pcb-layout/placement-validation.ts";
import { LayoutRulesSchema } from "../../../src/types/pcb/layout-rules.ts";
import { ExistingPlacementSchema } from "../../../src/types/pcb/layout-model.ts";

const directory = path.dirname(fileURLToPath(import.meta.url));
const read = file => readFileSync(path.join(directory, file), "utf8");
const circuit = JSON.parse(read("PortableScope.json"));
const existing = ExistingPlacementSchema().parse(JSON.parse(read("existing-placement.json")));
const parsed = LayoutRulesSchema().parse(runPcbLayoutDsl(read("PortableScope.js")));
const rules = ensurePreservedComponentBlocks(circuit, applyExistingBoard(parsed, existing), existing);
validatePlacementRulesForCircuit(circuit, rules);

const actual = new Set(circuit.components.map(component => component.designator));
const owners = new Map();
for (const block of rules.blocks) for (const designator of block.component_designators) {
    if (!actual.has(designator)) throw new Error(`Unknown block member ${designator}`);
    if (owners.has(designator)) throw new Error(`Duplicate ownership ${designator}: ${owners.get(designator)}, ${block.name}`);
    owners.set(designator, block.name);
}
const missing = [...actual].filter(designator => !owners.has(designator));
if (missing.length) throw new Error(`Unowned components: ${missing.join(", ")}`);
const components = new Map(circuit.components.map(component => [component.designator, component]));
const netAt = target => {
    const component = components.get(target.designator);
    const pad = component?.pins.find(pin => String(pin.pin_number) === String(target.pin_number));
    if (!pad) throw new Error(`Missing pin ${target.designator}.${target.pin_number}`);
    return pad.signal_name;
};
for (const pathRule of rules.paths) for (const [index, segment] of pathRule.segments.entries()) {
    const sourceNet = netAt(segment.source);
    const targetNet = netAt(segment.target);
    if (!sourceNet || sourceNet !== targetNet) {
        throw new Error(`Disconnected path ${pathRule.id}[${index}]: ${segment.source.designator}.${segment.source.pin_number}=${sourceNet}, ${segment.target.designator}.${segment.target.pin_number}=${targetNet}`);
    }
}
const board = rules.board;
const bbox = { x: [Math.min(...board.points.map(p => p.x)), Math.max(...board.points.map(p => p.x))],
    y: [Math.min(...board.points.map(p => p.y)), Math.max(...board.points.map(p => p.y))] };
const preserved = existing.components.map(c => c.designator);
if (preserved.length !== 8) throw new Error(`Expected 8 installed mechanical components, found ${preserved.length}`);
console.log(`DSL valid: ${actual.size} components, ${rules.blocks.length} blocks, ${preserved.length} preserved parts, board ${bbox.x[1]-bbox.x[0]} x ${bbox.y[1]-bbox.y[0]} mm.`);
