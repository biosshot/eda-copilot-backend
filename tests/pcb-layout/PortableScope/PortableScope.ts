import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { runPcbLayout } from "#pcb-layout/run-pcb-layout.ts";
import type { ExplainCircuit } from "#types/circuit.ts";
import type { ExistingPlacement } from "#types/pcb/layout-model.ts";

const directory = path.dirname(fileURLToPath(import.meta.url));
const read = (file: string) => readFileSync(path.join(directory, file), "utf8");
const run = await runPcbLayout({
    circuit: JSON.parse(read("PortableScope.json")) as ExplainCircuit,
    code: read("PortableScope.js"),
    existingPlacement: JSON.parse(read("existing-placement.json")) as ExistingPlacement,
    outputDir: path.join(".test-output", "pcb-layout", "PortableScope"),
});

console.log([
    `placement_ok: ${run.placementReport.ok}`,
    `components: ${run.placementInput.components.length}`,
    `board: ${run.layout.board.outline.width}mm x ${run.layout.board.outline.height}mm`,
    `errors: ${run.digest.errors.length}`,
    ...run.digest.errors.slice(0, 24).map(line => `- ${line}`),
    `diagnostics: ${run.digest.diagnostics.length}`,
    ...run.digest.diagnostics.slice(0, 24).map(line => `- ${line}`),
].join("\n"));
