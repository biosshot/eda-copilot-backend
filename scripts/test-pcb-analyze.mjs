import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { closeSync, existsSync, mkdirSync, mkdtempSync, openSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const backendRoot = fileURLToPath(new URL('../', import.meta.url));
const hash = file => createHash('sha256').update(readFileSync(file, 'utf8').replace(/\r\n/g, '\n')).digest('hex');

// The same one-process-per-case convention as test-pcb-layout, with a named
// analysis file and a distinct output directory for every invocation.
export function runPcbAnalyze(args, backend = backendRoot) {
  const { values, positionals } = parseArgs({ args, allowPositionals: true,
    options: { list: { type: 'boolean' }, all: { type: 'boolean' }, name: { type: 'string' } } });
  const directory = join(backend, 'tests', 'pcb-analyze');
  const cases = readdirSync(directory, { withFileTypes: true }).filter(entry => entry.isDirectory()
    && existsSync(join(directory, entry.name, 'fixture.json'))).map(entry => entry.name).sort();
  if (values.all && positionals.length) throw Error('Use fixture names or --all, not both.');
  const selected = values.all ? cases : positionals;
  for (const fixture of selected) if (!cases.includes(fixture)) throw Error(`Unknown PCB analyze fixture: ${fixture}. Use --list.`);
  if (values.list || !args.length) {
    for (const fixture of selected.length ? selected : cases) {
      const analyses = readdirSync(join(directory, fixture)).filter(file => file.endsWith('.ts') && !file.endsWith('.d.ts'));
      console.log(`${fixture}: ${analyses.length ? analyses.join(', ') : '(data only; no analysis files yet)'}`);
    }
    console.log('\nRun: npm run test:pcb-analyze -- <fixture> [fixture ...] --name <analysis>');
    return 0;
  }
  if (!selected.length) throw Error('Select a fixture or --all.');
  const name = values.name?.replace(/\.ts$/, '');
  if (!name || !/^[a-zA-Z0-9][a-zA-Z0-9_-]*$/.test(name)) throw Error('--name must be an analysis filename stem, e.g. dc or emi.');
  const failed = [];
  for (const fixture of selected) {
    const fixtureDir = join(directory, fixture), script = join(fixtureDir, `${name}.ts`);
    if (!existsSync(script)) {
      console.error(`${fixture}: analysis ${name}.ts does not exist. No run was started.`);
      failed.push(fixture);
      continue;
    }
    const runs = join(backend, 'debugging', 'pcb-analyze', fixture, 'runs');
    mkdirSync(runs, { recursive: true });
    const startedAt = new Date().toISOString();
    const output = mkdtempSync(join(runs, `${startedAt.replace(/[:.]/g, '-')}-${name}-`));
    const manifestFile = join(output, 'run.json');
    const manifest = { fixture, name, startedAt, status: 'running',
      script: `${name}.ts`, scriptSha256: hash(script), fixtureSha256: hash(join(fixtureDir, 'fixture.json')) };
    writeFileSync(manifestFile, JSON.stringify(manifest, null, 2) + '\n');
    console.log(`Running ${fixture}/${name}.ts\nOutput: ${output}`);
    const log = openSync(join(output, 'run.log'), 'w');
    let result;
    try {
      result = spawnSync(process.execPath, ['--import', 'tsx', script], { cwd: backend,
        env: { ...process.env, PCB_ANALYZE_FIXTURE_DIR: fixtureDir, PCB_ANALYZE_RUN_DIR: output, PCB_ANALYZE_NAME: name },
        stdio: ['ignore', log, log], windowsHide: true, timeout: 600_000 });
    } finally { closeSync(log); }
    const ok = !result.error && result.status === 0;
    writeFileSync(manifestFile, JSON.stringify({ ...manifest, finishedAt: new Date().toISOString(),
      status: ok ? 'completed' : 'failed', exitCode: result.status, signal: result.signal,
      error: result.error?.message ?? null }, null, 2) + '\n');
    console.log(`${fixture}/${name}: ${ok ? 'completed' : 'failed'}. Log: ${join(output, 'run.log')}`);
    if (!ok) failed.push(fixture);
  }
  console.log(`\nPCB analyze: ${selected.length - failed.length} completed, ${failed.length} failed.`);
  return failed.length ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = runPcbAnalyze(process.argv.slice(2)); }
  catch (error) { console.error(error.message); process.exitCode = 1; }
}
