import { readFileSync, writeFileSync, mkdirSync, openSync, closeSync } from 'node:fs';
import { spawn } from 'node:child_process';
mkdirSync('.test-output/global-placement', { recursive: true });
const saved = process.argv.includes('--saved');
const manifest = JSON.parse(readFileSync(saved ? 'docs/experiments/global-placement-2026-09-27/manifest.json' : '.test-output/global-placement/manifest.json', 'utf8'));
if (saved) writeFileSync('.test-output/global-placement/manifest.json', JSON.stringify(manifest, null, 2));
const variants = [['before', '0', '0', '0'], ['refined', '2', '1', '0'], ['pads', '2', '1', '1']];
const selected = new Set(process.argv.slice(2).filter(arg => arg !== '--saved'));
const fixtures = manifest.filter(f => f.input && (!selected.size || selected.has(f.name)));
const jobs = fixtures.flatMap(f => variants.map(v => ({ f, v })));
const results = selected.size ? JSON.parse(readFileSync('.test-output/global-placement/runs.json', 'utf8')).filter(r => !selected.has(r.fixture)) : [];
async function worker() {
    while (jobs.length) {
        const { f, v: [tag, candidates, refine, pads] } = jobs.shift();
        const dir = `.test-output/architecture/${f.name}/global-${tag}`;
        mkdirSync(dir, { recursive: true });
        const fd = openSync(`${dir}/run.log`, 'w');
        const start = Date.now();
        const result = await new Promise(resolve => {
            const child = spawn(process.execPath, ['--import', 'tsx', 'scripts/experiment-placement-architecture.mjs', f.name, 'full-micro'], {
                windowsHide: true, stdio: ['ignore', fd, fd], env: { ...process.env, PCB_EXPERIMENT_INPUT: f.input, PCB_EXPERIMENT_TAG: `global-${tag}`, PCB_BLOCK_CANDIDATES: candidates, PCB_BLOCK_POST_REFINE: refine, PCB_PLACEMENT_PAD_CROSSINGS: pads },
            });
            const timer = setTimeout(() => { child.kill(); }, 600_000);
            child.on('error', e => resolve({ error: e.message }));
            child.on('close', (code, signal) => { clearTimeout(timer); resolve({ code, signal }); });
        });
        closeSync(fd);
        results.push({ fixture: f.name, tag, ...result, wallMs: Date.now() - start });
        writeFileSync('.test-output/global-placement/runs.json', JSON.stringify(results, null, 2));
        console.log(`${results.length}/${manifest.filter(f => f.input).length * 3} ${f.name} ${tag}: ${JSON.stringify(result)} ${Math.round((Date.now() - start) / 1000)}s`);
    }
}
await Promise.all([worker(), worker(), worker()]);
