// Four independent ablations through the real complete-board entry point.
import { spawnSync } from 'node:child_process';
import { mkdirSync, openSync, closeSync } from 'node:fs';
const fixture = process.argv[2] ?? 'Telemetry';
for (const [tag, candidates, refine] of [
    ['candidates-before', '0', '0'], ['candidates-clearance', '1', '0'],
    ['candidates-expanded', '2', '0'], ['candidates-refined', '2', '1'],
]) {
    const dir = `.test-output/architecture/${fixture}/${tag}`;
    mkdirSync(dir, { recursive: true });
    const log = openSync(`${dir}/run.log`, 'w');
    const result = spawnSync(process.execPath, ['--import', 'tsx', 'scripts/experiment-placement-architecture.mjs', fixture, 'full-micro'], {
        windowsHide: true, stdio: ['ignore', log, log],
        env: { ...process.env, PCB_EXPERIMENT_TAG: tag, PCB_BLOCK_CANDIDATES: candidates, PCB_BLOCK_POST_REFINE: refine },
    });
    closeSync(log);
    console.log(`${fixture} ${tag}: ${result.status}`);
    if (result.error) throw result.error;
    if (result.status !== 0) process.exit(result.status ?? 1);
}
