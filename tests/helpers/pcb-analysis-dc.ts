import { readFile, writeFile, mkdir, readdir } from 'node:fs/promises';
import { basename, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { runAnalysisDsl, type RunAnalysisDslOptions } from '../../src/analyze/run-analysis-dsl.ts';
import type { RoutingBoardV2 } from '../../src/types/pcb/routing-board-v2.ts';
import { renderDcReport, renderDcIndex } from './pcb-analysis-dc-render.ts';
import { auditDcReport, completedDc, type DcExpectations } from './pcb-analysis-dc-audit.ts';

/** Transport/provenance/audit only. Physical inputs live in the authored DSL file. */
export async function runSavedAnalysisDc(input: { script: URL; expectations: URL; runner: Pick<RunAnalysisDslOptions, 'timeoutMs' | 'maxOutputBytes'> }) {
  const fixture = process.env.PCB_ANALYZE_FIXTURE_DIR, output = process.env.PCB_ANALYZE_RUN_DIR;
  if (!fixture || !output) throw Error('Use test:pcb-analyze -- <fixture> --name dc');
  const name = basename(fixture), bytes = await readFile(join(fixture, `${name}.json`), 'utf8');
  const board = JSON.parse(bytes) as RoutingBoardV2;
  const code = await readFile(input.script, 'utf8');
  const expectedText = await readFile(input.expectations, 'utf8');
  const expectations = JSON.parse(expectedText) as DcExpectations;
  // Preserve the exact authored bytes/text, including final newline, without generation.
  await writeFile(join(output, 'scenario.js'), code);
  const implementation: Record<string, string> = {};
  const digest = (v: string | Buffer) => createHash('sha256').update(v).digest('hex');
  async function capture(path: string) {
    for (const entry of await readdir(path, { withFileTypes: true })) {
      const file = join(path, entry.name);
      if (entry.isDirectory()) await capture(file);
      else if (/\.(?:rs|ts)$/.test(file)) implementation[file.replaceAll('\\', '/')] = digest((await readFile(file, 'utf8')).replaceAll('\r\n', '\n'));
    }
  }
  for (const path of ['src/analyze', 'native/analyze/src', 'native/node-addon/src']) await capture(path);
  for (const file of ['native/analyze/Cargo.toml', 'native/analyze/Cargo.lock', 'native/node-addon/Cargo.toml', 'native/node-addon/Cargo.lock', 'tests/helpers/pcb-analysis-dc-audit.ts', 'tests/helpers/pcb-analysis-dc.ts', 'tests/helpers/pcb-analysis-dc-render.ts']) implementation[file] = digest((await readFile(file, 'utf8')).replaceAll('\r\n', '\n'));
  await writeFile(join(output, 'implementation.json'), JSON.stringify(implementation, null, 2) + '\n');
  const { nativeFilename } = createRequire(import.meta.url)('../../native/node-addon/platform.cjs') as { nativeFilename(): string };
  const metadata = {
    fixture: name, authoredScript: fileURLToPath(input.script), expectations: fileURLToPath(input.expectations),
    version: { sourceSha256: digest(JSON.stringify(implementation)), nativeSha256: digest(await readFile(join('native/node-addon', nativeFilename()))), scriptSha256: digest(code), expectationsSha256: digest(expectedText) },
    sourceSha256: digest(bytes.replaceAll('\r\n', '\n')),
    codeVersion: execFileSync('git', ['rev-parse', 'HEAD'], { encoding: 'utf8', windowsHide: true }).trim(),
    workingTree: 'Review pending; implementation.json hashes identify the executed version',
    resources: 'No external profiles/models: explicit physical inputs are in the hashed authored script and completed result inputs/requests',
    coordinates: 'absolute mm, Y down', cadFreshness: 'unknown; unchanged',
  };
  const result = await runAnalysisDsl({ code, board }, { ...input.runner, maxEntries: 3000, filename: fileURLToPath(input.script) });
  const savedMetadata = { ...metadata, physicalInputs: completedDc(result).map(r => ({ resultId: r.id, inputsSha256: digest(JSON.stringify((r as unknown as { inputs: unknown }).inputs)), profileSha256: digest(JSON.stringify((r.solution.request.conduction as { profile?: unknown }).profile ?? null)) })) };
  await writeFile(join(output, 'dc.json'), JSON.stringify({ metadata: savedMetadata, result }) + '\n');
  let audit: unknown;
  try { audit = { status: 'pass', ...auditDcReport(result, expectations) }; }
  catch (error) { audit = { status: 'fail', reason: error instanceof Error ? error.message : String(error) }; process.exitCode = 1; console.error('DC corpus audit failed', audit); }
  await writeFile(join(output, 'audit.json'), JSON.stringify(audit, null, 2) + '\n');
  await writeFile(join(output, 'index.html'), renderDcReport(board, { ...savedMetadata, audit }, result));
  console.log(JSON.stringify({ fixture: name, status: result.status, error: result.error, audit }));
  const overview = resolve('debugging/pcb-analyze/dc'); await mkdir(overview, { recursive: true });
  await writeFile(join(overview, `${name}.json`), JSON.stringify({ fixture: name, output: resolve(output), status: result.status, sourceSha256: metadata.sourceSha256, audit }) + '\n');
  await renderDcIndex(overview);
}
