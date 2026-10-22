import { Script, createContext } from 'node:vm';
import { createAnalysisApi, type DslInput, type Limits } from './context.ts';
import { type AnalysisExecutor } from './executor.ts';
import type { Report, Diagnostic } from './spec-doc.js';
import { object } from './descriptions.ts';
import { applyReportUpdate, type ReportUpdate } from './data.ts';

export interface ScenarioError { code: string; message: string; filename: string; line?: number; column?: number; diagnostics: readonly Diagnostic[] }
export interface AnalysisDslOutput { status: 'complete' | 'failed'; reports: readonly Report[]; error?: ScenarioError }
export interface ScenarioInput extends DslInput { code: string }
export interface ScenarioOptions extends Limits { filename: string }
export const ERROR_RESERVE_BYTES = 2048;

/** Internal scenario kernel: test code can inject an executor here. The public
 * runner always calls this in a disposable process with the production executor.
 * vm controls convenience globals; it is NOT a security boundary for hostile JS.
 */
export async function executeScenario(input: ScenarioInput, options: ScenarioOptions, changed: (update: ReportUpdate) => void = () => {}, executor?: AnalysisExecutor): Promise<AnalysisDslOutput> {
  const reports: Report[] = [];
  try {
    const api = createAnalysisApi(input, { ...options, maxOutputBytes: options.maxOutputBytes - ERROR_RESERVE_BYTES }, update => { applyReportUpdate(reports, update); changed(update); }, executor);
    const sandbox = createContext({ ...api.globals }, { codeGeneration: { strings: false, wasm: false }, name: 'analysis-dsl' });
    // One wrapper line, corrected by lineOffset; user code remains ordinary JS.
    const script = new Script(`(async () => { "use strict";\n${input.code}\n})()`, { filename: options.filename, lineOffset: -1 });
    await script.runInContext(sandbox, { displayErrors: true });
    return { status: 'complete', reports: api.snapshot() };
  } catch (error) {
    return { status: 'failed', reports, error: scenarioError(error, options.filename, input.code) };
  }
}
export function scenarioError(error: unknown, filename: string, source?: string): ScenarioError {
  const e = object(error) ? error : {};
  const stack = typeof e.stack === 'string' ? e.stack : '';
  const escaped = filename.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const location = new RegExp(`^${escaped}:(\\d+)(?::(\\d+))?`);
  let match: RegExpMatchArray | null = null;
  for (const raw of stack.split('\n')) {
    const frame = raw.trim();
    const named = frame.indexOf(`(${filename}:`);
    const candidate = frame.startsWith(`at async ${filename}:`) ? frame.slice(9)
      : frame.startsWith(`at ${filename}:`) ? frame.slice(3)
      : named >= 0 ? frame.slice(named + 1) : frame;
    match = candidate.match(location);
    if (match) break;
  }
  const caret = stack.split('\n')[2]?.indexOf('^');
  const result: ScenarioError = { code: typeof e.code === 'string' && /^[A-Z][A-Z0-9_]{0,79}$/.test(e.code) ? e.code : e.name === 'SyntaxError' ? 'SYNTAX_ERROR' : 'RUNTIME_ERROR', message: truncate(typeof e.message === 'string' ? e.message : String(error), 512), filename,
    ...(match ? { line: Number(match[1]), ...(match[2] ? { column: Number(match[2]) } : caret >= 0 ? { column: caret + 1 } : {}) } : {}),
    diagnostics: [] };
  const lines = source?.split('\n');
  if (e.name === 'SyntaxError' && lines && result.line !== undefined && result.line > lines.length) {
    result.line = lines.length; result.column = lines.at(-1)!.length + 1;
  }
  while (Buffer.byteLength(JSON.stringify(result)) > ERROR_RESERVE_BYTES - 128 && result.message.length > 20) result.message = truncate(result.message, Math.max(16, Buffer.byteLength(result.message) / 2));
  if (Array.isArray(e.diagnostics)) for (const d of e.diagnostics) {
    if (!object(d)) continue;
    const candidate: Diagnostic = { code: truncate(String(d.code), 80), severity: d.severity === 'error' || d.severity === 'warning' ? d.severity : 'info', message: truncate(String(d.message), 160), ...(d.path ? { path: truncate(String(d.path), 80) } : {}) };
    if (Buffer.byteLength(JSON.stringify({ ...result, diagnostics: [...result.diagnostics, candidate] })) > ERROR_RESERVE_BYTES - 128) break;
    result.diagnostics = [...result.diagnostics, candidate];
  }
  return result;
}
function truncate(text: string, bytes: number) { return Buffer.byteLength(text) <= bytes ? text : Buffer.from(text).subarray(0, bytes - 16).toString('utf8') + ' [truncated]'; }
