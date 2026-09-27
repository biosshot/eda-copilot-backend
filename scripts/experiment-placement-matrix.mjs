import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';

mkdirSync('.test-output/architecture', { recursive: true });
const repack = process.argv.includes('--repack');
const results = repack ? JSON.parse(readFileSync('.test-output/architecture/measurements.json')) : [];
for (const fixture of ['Telemetry', 'ESPower', 'esp32c3']) {
    for (const mode of repack ? ['full-micro-repack', 'full-geometric-repack'] : ['legacy', 'full-micro-single', 'full-micro', 'full-off', 'full-geometric-single', 'full-geometric']) {
        const run = spawnSync(process.execPath, ['--import', 'tsx', 'scripts/experiment-placement-architecture.mjs', fixture, mode],
            { encoding: 'utf8', windowsHide: true, maxBuffer: 16 * 1024 * 1024 });
        writeFileSync(`.test-output/architecture/${fixture}-${mode}.log`, `${run.stdout ?? ''}\n${run.stderr ?? ''}`);
        if (run.status !== 0 || run.error) throw Error(`${fixture}/${mode}: ${run.error ?? run.stderr ?? run.status}`);
        const summary = JSON.parse(readFileSync(`.test-output/architecture/${fixture}/${mode}/summary.json`));
        const previous = results.findIndex(r => r.fixture === fixture && r.mode === mode);
        if (previous >= 0) results[previous] = summary; else results.push(summary);
        writeFileSync('.test-output/architecture/measurements.json', JSON.stringify(results, null, 2));
        console.log(JSON.stringify({ fixture, mode, ok: summary.ok, seconds: +(summary.ms / 1000).toFixed(2),
            ...summary.stageMetrics.at(-1), portfolio: summary.diagnostics.filter(d => d.message.startsWith('Block portfolio')).map(d => d.message) }));
    }
}
