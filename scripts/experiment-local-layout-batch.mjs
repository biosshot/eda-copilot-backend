import { readFileSync, writeFileSync, mkdirSync, openSync, closeSync } from 'node:fs';
import { spawn } from 'node:child_process';
const root = 'docs/experiments/global-placement-2026-09-27';
const boards = JSON.parse(readFileSync(`${root}/measurements.json`, 'utf8')).filter(b => b.entities.length && !b.duplicateOf && b.name !== 'Telemetry');
const results = [];
async function worker() {
    while (boards.length) {
        const b = boards.shift(), dir = `.test-output/architecture/${b.name}/local-fixes`;
        mkdirSync(dir, { recursive: true }); const fd = openSync(`${dir}/run.log`, 'w');
        const result = await new Promise(resolve => {
            const child = spawn(process.execPath, ['--import', 'tsx', 'scripts/experiment-placement-architecture.mjs', b.name, 'full-micro'], {
                windowsHide: true, stdio: ['ignore', fd, fd], env: { ...process.env, PCB_EXPERIMENT_INPUT: `${root}/${b.name}/input.json`, PCB_EXPERIMENT_TAG: 'local-fixes', PCB_BLOCK_CANDIDATES: '2', PCB_BLOCK_POST_REFINE: '1', PCB_PLACEMENT_PAD_CROSSINGS: '1' },
            });
            const timer = setTimeout(() => child.kill(), 900_000);
            child.on('error', e => resolve({ error: e.message }));
            child.on('close', (code, signal) => { clearTimeout(timer); resolve({ code, signal }); });
        });
        closeSync(fd); results.push({ fixture: b.name, ...result });
        writeFileSync('.test-output/local-layout-runs.json', JSON.stringify(results, null, 2));
        console.log(`${results.length}/14 ${b.name}: ${JSON.stringify(result)}`);
    }
}
await Promise.all([worker(), worker()]);
