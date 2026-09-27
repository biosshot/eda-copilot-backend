import { readFileSync, writeFileSync, readdirSync, mkdirSync } from 'node:fs';
import { runPcbLayoutDsl } from '../src/pcb-layout/pcb-layout-dsl/spec.ts';
import { LayoutRulesSchema } from '../src/types/pcb/layout-rules.ts';
import { applyPlacementPreviewFilter, buildPlacementInput } from '../src/pcb-layout/placement-input.ts';
import { validatePlacementRulesForCircuit } from '../src/pcb-layout/placement-validation.ts';
import { attachLocalLayoutSeeds, extractLocalLayouts, instrumentLocalLayoutDsl, stripLocalLayoutCarriers, validateLocalLayouts } from '../src/pcb-layout/pcb-auto-place-v2/local-layout.ts';
const root = '.test-output/global-placement';
mkdirSync(root, { recursive: true });
const manifest = [];
for (const name of ['Telemetry', 'ESPower', 'esp32c3']) manifest.push({ name, source: 'captured board', input: `tests/fixtures/block-placement/${name}/input.json` });
for (const dir of readdirSync('tests/pcb-layout', { withFileTypes: true }).filter(d => d.isDirectory())) {
    const path = `tests/pcb-layout/${dir.name}`;
    const runner = readdirSync(path).find(f => f.endsWith('.ts'));
    if (!runner) { manifest.push({ name: dir.name, error: 'Нет DSL и запускаемого теста в исходном банке.' }); continue; }
    const source = readFileSync(`${path}/${runner}`, 'utf8');
    const base = source.match(/runPcbLayoutFixture\(import\.meta\.url,\s*"([^"]+)"/)?.[1];
    const name = `bank-${dir.name}`;
    try {
        if (!base) throw Error('Не удалось определить исходный fixture.');
        const code = readFileSync(`${path}/${base}.js`, 'utf8');
        const raw = runPcbLayoutDsl(instrumentLocalLayoutDsl(code));
        const locals = extractLocalLayouts(raw);
        const parsed = LayoutRulesSchema().parse(stripLocalLayoutCarriers(raw));
        const { circuit, rules, preview } = applyPlacementPreviewFilter(JSON.parse(readFileSync(`${path}/${base}.json`, 'utf8')), parsed);
        if (preview.enabled) throw Error('Fixture ограничен preview; полная плата не подготовлена.');
        validatePlacementRulesForCircuit(circuit, rules);
        validateLocalLayouts(circuit, rules, locals);
        const input = attachLocalLayoutSeeds(await buildPlacementInput(circuit, rules), locals);
        const inputPath = `${root}/${name}/input.json`;
        mkdirSync(`${root}/${name}`, { recursive: true });
        writeFileSync(inputPath, JSON.stringify(input));
        manifest.push({ name, source: path, input: inputPath, components: input.components.length, blocks: input.blocks.length });
        console.log(`${name}: ${input.components.length} components, ${input.blocks.length} blocks`);
    } catch (e) { manifest.push({ name, source: path, error: e.message }); console.log(`${name}: SKIP ${e.message.slice(0, 180)}`); }
    writeFileSync(`${root}/manifest.json`, JSON.stringify(manifest, null, 2));
}
