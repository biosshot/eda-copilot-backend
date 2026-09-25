import { mkdir, readFile, writeFile, readdir, rm } from 'node:fs/promises';
import { resolve, relative, basename, dirname, isAbsolute, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { parseArgs } from 'node:util';
import { availableParallelism } from 'node:os';
import { parallelJobs, coalescedWriter, runIsolated, writeJsonAtomic } from './testing/bank-runner.ts';
import { compareSignalAssignments, signalCheck, formatSignalCheck } from './testing/schematic-signals.ts';
import { pageBoundarySignals } from './testing/schematic-boundaries.ts';
import type { Circuit, CircuitMod } from '../src/types/circuit.ts';
import type { BankFixture } from './testing/schematic-layout.ts';
import type { inspectLayout } from './testing/schematic-layout.ts';
import type { autoPlaceCircuitWithHierarchy } from '../src/circuit-layout/index.ts';

type Job = { file: string; cached: boolean; source: string; title: string; id: string;
    externalSignals: string[]; pageFile?: string; pageSource?: string; pageHash?: string };
type Variant = ReturnType<typeof inspectLayout> & { elapsedMs: number; refinement?: Awaited<ReturnType<typeof autoPlaceCircuitWithHierarchy>>['refinement'];
    exportValid: boolean; exportErrors: string[]; sceneMatches: boolean; pngSkipped?: string;
    assemblyRoot?: { x: number; y: number; width: number; height: number }; rootAspectRatio: number | null };
type Report = { file: string; title: string; fingerprint?: string; id: string; inputHash?: string; status: string;
    externalSignals?: string[];
    regressions?: string[]; review?: string[]; variants?: Record<'before' | 'after', Variant>; error?: string };

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const output = join(root, '.test-output', process.env.SCHEMATIC_LAYOUT_GALLERY ?? 'new-circuit-layout');
const cache = join(root, '.test-output', 'new-circuit-layout', 'cache');
const digest = (s: string) => createHash('sha256').update(s).digest('hex');
const html = (s: unknown) => String(s ?? '').replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('"', '&quot;');
const json = async (path: string) => JSON.parse(await readFile(path, 'utf8'));

export function parseOptions(args: string[]) {
    const { values } = parseArgs({ args, options: {
        bank: { type: 'string', default: resolve(root, 'tests/schematic-layout/portable-scope') },
        limit: { type: 'string' }, filter: { type: 'string' }, case: { type: 'string' },
        timeout: { type: 'string', default: '180' }, offline: { type: 'boolean' }, resume: { type: 'boolean' },
        workers: { type: 'string', default: String(Math.min(4, availableParallelism())) },
        'group-seed': { type: 'string' },
        'group-seeds': { type: 'string' },
        'dense-net-labels': { type: 'boolean' },
        'no-dense-net-labels': { type: 'boolean' },
        'no-patterns': { type: 'boolean' }, all: { type: 'boolean' }, help: { type: 'boolean' },
        worker: { type: 'string' }, fingerprint: { type: 'string' }, cached: { type: 'boolean' },
    } });
    for (const name of ['limit', 'timeout', 'workers'] as const) if (values[name] !== undefined
        && (!/^\d+$/.test(values[name]!) || !Number.isSafeInteger(Number(values[name])) || Number(values[name]) < 1)) throw new Error(`--${name} must be a positive integer`);
    if (Number(values.timeout) * 1000 > 2147483647) throw new Error('--timeout exceeds the timer limit');
    if (values['group-seed'] && (!/^\d+$/.test(values['group-seed']) || !Number.isSafeInteger(Number(values['group-seed']))
        || Number(values['group-seed']) < 1)) throw new Error('--group-seed must be a positive integer');
    if (values['group-seeds'] && (!/^\d+(,\d+)*$/.test(values['group-seeds'])
        || values['group-seeds'].split(',').some(seed => !Number.isSafeInteger(Number(seed)) || Number(seed) < 1)))
        throw new Error('--group-seeds must be comma-separated positive integers');
    if (values['group-seed'] && values['group-seeds']) throw new Error('Choose either --group-seed or --group-seeds');
    if (values['dense-net-labels'] && values['no-dense-net-labels']) throw new Error('Choose either --dense-net-labels or --no-dense-net-labels');
    if (values.case && (basename(values.case) !== values.case || !values.case.endsWith('.json'))) throw new Error('--case accepts a filename in the ignored cache, not a path');
    return values;
}

/** Accept ASM or a self-contained create request, stripping old placement and
 * service symbols. Never repair missing pins or guess electrical connectivity. */
export function circuitInput(raw: Circuit | CircuitMod): Circuit {
    if ('add_components' in raw) {
        if (!Array.isArray(raw.add_components)) throw new Error('Expected a creation request with add_components');
        if (raw.add_reused_blocks?.length || raw.rm_components?.length || raw.external_connect || raw.external_rm_connect) {
            throw new Error('A creation fixture must be self-contained; modifications need the complete input ASM');
        }
        raw = { components: raw.add_components,
            blocks: [...new Set(raw.add_components.map(c => c.block_name))].map(name => ({ name, description: '', next_block_names: [] })),
            metadata: { project_name: '', description: '' }, reused_blocks: [] };
    }
    if (!Array.isArray(raw.components) || !Array.isArray(raw.blocks)) throw new Error('Expected an ASM with components and blocks');
    const components = raw.components.filter(c => !['GND', 'VCC', '7523d33c197549a39030c4ac7fddee68'].includes(c.part_uuid ?? ''))
        .map(c => { const copy = { ...c } as typeof c & { pos?: unknown }; delete copy.pos; return copy; });
    if (!components.length) throw new Error('No original components');
    if (new Set(components.map(c => c.designator)).size !== components.length) throw new Error('Duplicate designators');
    return { metadata: raw.metadata, blocks: raw.blocks, components, reused_blocks: [] };
}

async function loadFixture(file: string, sourcePath: string, cachedOnly: boolean, offline: boolean) {
    let fixture: BankFixture & { inputHash?: string; subParts?: Record<string, string | undefined> };
    const cached = await json(join(cache, file)).catch(() => null);
    if (cachedOnly) fixture = cached;
    else {
        const source = await readFile(sourcePath, 'utf8'), inputHash = digest(source);
        const circuit = circuitInput(JSON.parse(source));
        if (cached?.inputHash === inputHash) fixture = cached;
        else if (cached && !cached.inputHash && JSON.stringify(circuit) === JSON.stringify(cached.circuit)) fixture = cached;
        else {
            if (offline) throw new Error('No matching frozen symbol cache; run once without --offline');
            const { circuitToSymbols } = await import('../src/devices/symbols/symbol-parser.ts');
            const { getPartIdFromDesignator } = await import('../src/utils/component.ts');
            const symbolCache = join(output, 'symbols'); await mkdir(symbolCache, { recursive: true });
            const symbols: BankFixture['symbols'] = [], subParts: Record<string, string | undefined> = {};
            for (const c of circuit.components) {
                if (!c.part_uuid) throw new Error(`Missing part_uuid: ${c.designator}`);
                const key = digest(`v1:${c.part_uuid}:${getPartIdFromDesignator(c.designator)}`), filename = join(symbolCache, `${key}.json`);
                let geometry = await json(filename).catch(() => null);
                if (!geometry) {
                    const prepared = await circuitToSymbols({ components: [c] });
                    if (!prepared.nodes[0]) throw new Error(`Missing symbol: ${c.designator}`);
                    geometry = { symbol: prepared.nodes[0].symbol, subPart: prepared.subParts[c.designator] };
                    await writeJsonAtomic(filename, geometry);
                }
                symbols.push({ designator: c.designator, block_name: c.block_name, symbol: structuredClone(geometry.symbol) });
                subParts[c.designator] = geometry.subPart;
            }
            fixture = { source: file, inputHash, circuit, symbols, subParts };
            await writeJsonAtomic(join(cache, file), fixture);
        }
    }
    if (!fixture?.symbols?.length) throw new Error('Missing frozen geometry');
    for (const c of fixture.circuit.components) {
        const s = fixture.symbols.find(s => s.designator === c.designator);
        if (!s) throw new Error(`Missing symbol ${c.designator}`);
        for (const p of c.pins) if (!s.symbol.pins.some(pin => String(pin.num) === String(p.pin_number))) throw new Error(`Missing library pin ${c.designator}.${p.pin_number}`);
        for (const p of s.symbol.pins) p.signal_name = c.pins.find(pin => String(pin.pin_number) === String(p.num))?.signal_name ?? '';
    }
    return fixture;
}

async function worker(options: ReturnType<typeof parseOptions>) {
    const job: Job = await json(options.worker!);
    const folder = resolve(output, job.id), rel = relative(output, folder);
    if (!rel || rel.startsWith('..') || isAbsolute(rel)) throw new Error('Invalid output folder');
    await mkdir(folder, { recursive: true });
    const { autoPlaceCircuitWithHierarchy, refinedBlockBounds } = await import('../src/circuit-layout/index.ts');
    const { inspectLayout, renderSvg } = await import('./testing/schematic-layout.ts');
    const { renderSvgToPng } = await import('./testing/render-schematic-png.ts');
    const fixture = await loadFixture(job.file, job.source, job.cached, !!options.offline);
    const page = job.pageFile && await loadFixture(job.pageFile, job.pageSource!, false, !!options.offline);
    await writeFile(join(folder, 'input.json'), JSON.stringify({ ...fixture, externalSignals: job.externalSignals,
        boundaryContext: page?.source }, null, 2));
    const variants = {} as Record<'before' | 'after', Variant>;
    for (const [name, layoutRefinement] of [['before', false], ['after', true]] as const) {
        const start = performance.now();
        const layoutCircuit = structuredClone(fixture.circuit);
        const result = await autoPlaceCircuitWithHierarchy(layoutCircuit, structuredClone(fixture.symbols), undefined,
            { layoutRefinement, layoutPatterns: !options['no-patterns'], externalSignals: job.externalSignals,
                denseNetLabels: layoutRefinement && !options['no-dense-net-labels'],
                boundaryContext: page ? { circuit: page.circuit, symbols: page.symbols } : undefined,
                refinementOrderSeeds: layoutRefinement && (options['group-seeds'] ?? options['group-seed'])
                    ? (options['group-seeds'] ?? options['group-seed']!).split(',').map(Number) : undefined });
        const elapsedMs = performance.now() - start;
        const inspection = inspectLayout(fixture, result);
        // Rebuild the same final geometry from the serialized ASM positions, as
        // exporters do, rather than trusting only the optimizer's internal scene.
        const assembly = { ...layoutCircuit, components: [...layoutCircuit.components, ...result.addedSymbol].map(c => ({ ...c,
            pos: result.positioned.find(p => p.designator === c.designator), sub_part_name: fixture.subParts?.[c.designator] })),
            edges: result.edges, blocks_rect: refinedBlockBounds(fixture.circuit, result.addedSymbol, result.positioned, result.edges) };
        const serialized: typeof assembly = JSON.parse(JSON.stringify(assembly));
        const assignments = compareSignalAssignments(fixture.circuit.components, serialized.components, new Set(result.addedSymbol.map(c => c.designator)));
        // Preserve the backend's resolved symbol ports. The current placer may
        // expand a library symbol during layout, so frozen pre-layout geometry
        // is not enough to reconstruct its final pin offsets.
        const poses = new Map(serialized.components.map(c => [c.designator, c.pos!]));
        const rebuilt = structuredClone(result.renderGraph!);
        rebuilt.children = rebuilt.children?.map(node => {
            const pose = poses.get(node.id);
            return pose ? { ...node, x: pose.x, y: pose.y, width: pose.width, height: pose.height } : node;
        });
        rebuilt.edges = serialized.edges;
        const exportInspection = inspectLayout(fixture, { ...result, renderGraph: rebuilt, edges: serialized.edges });
        // The client reserves assembly space using this root, not the canvas.
        const rootRect = serialized.blocks_rect.find(b => b.name.includes('__v_root__'));
        const occupied = [...serialized.components.flatMap(c => [{ x: c.pos!.x, y: c.pos!.y },
            { x: c.pos!.x + c.pos!.width, y: c.pos!.y + c.pos!.height }]),
            ...serialized.edges.flatMap(e => (e.sections ?? []).flatMap(s => [s.startPoint, ...(s.bendPoints ?? []), s.endPoint]))];
        const boundsErrors = occupied.length && (!rootRect || occupied.some(p => p.x < rootRect.x - 1e-6 || p.y < rootRect.y - 1e-6
            || p.x > rootRect.x + rootRect.width + 1e-6 || p.y > rootRect.y + rootRect.height + 1e-6))
            ? ['Assembly root bounds do not contain final components and wires'] : [];
        const canonical = (graph: typeof rebuilt) => JSON.stringify(graph.children?.map(n => [n.id, n.x, n.y, n.width, n.height,
            n.ports?.map(p => [p.id, p.x, p.y])?.sort((a, b) => String(a[0]).localeCompare(String(b[0])))])?.sort((a, b) => String(a[0]).localeCompare(String(b[0]))));
        const sceneMatches = canonical(rebuilt) === canonical(result.renderGraph!);
        variants[name] = { ...inspection, valid: inspection.valid && assignments.valid,
            signalCheck: signalCheck([...inspection.signalCheck.differences, ...exportInspection.signalCheck.differences, ...assignments.differences]),
            elapsedMs: Math.round(elapsedMs), refinement: result.refinement,
            assemblyRoot: rootRect, rootAspectRatio: rootRect && rootRect.height > 0 ? rootRect.width / rootRect.height : null,
            exportValid: exportInspection.valid && assignments.valid && !boundsErrors.length,
            exportErrors: [...exportInspection.errors, ...boundsErrors, ...(!assignments.valid ? [formatSignalCheck(assignments)] : [])], sceneMatches };
        await writeFile(join(folder, `${name}.result.json`), JSON.stringify(result, null, 2));
        await writeFile(join(folder, `${name}.asm.json`), JSON.stringify(assembly, null, 2));
        await writeFile(join(folder, `${name}.svg`), renderSvg(fixture, result));
        // Existing universal project renderer, not a KiCad-only screenshot.
        if (result.width * result.height > 60_000_000 || Math.max(result.width, result.height) > 30000) variants[name].pngSkipped = 'Canvas exceeds size limit; SVG is available';
        else await renderSvgToPng(renderSvg(fixture, result), join(folder, `${name}.png`));
    }
    const regressions: string[] = (['nodeOverlapCount', 'wireThroughNodeCount'] as const).filter(k => variants.after.quality[k] > variants.before.quality[k]);
    // A broken baseline can look artificially short/simple because it omits
    // required wires. Compare aesthetics only when both drawings are electrical.
    if (variants.before.valid && variants.after.differentNetCrossings > variants.before.differentNetCrossings) regressions.push('differentNetCrossings');
    if (variants.after.protectedRotationChanges.length) regressions.push('protectedRotationChanges');
    const review = variants.before.valid ? [
        ...(variants.after.physicalWireLength > variants.before.physicalWireLength * 1.15 ? ['wire length grew >15%'] : []),
        ...(variants.after.drawingBounds.area > variants.before.drawingBounds.area * 1.15 ? ['drawing area grew >15%'] : []),
    ] : ['baseline connectivity invalid; shorter baseline wires are not a fair comparison'];
    const report = { file: job.file, title: fixture.circuit.metadata?.project_name || job.file, fingerprint: options.fingerprint,
        externalSignals: job.externalSignals, regressions, review,
        status: variants.after.valid && variants.after.exportValid && variants.after.sceneMatches && !regressions.length ? 'ok' : 'failed', variants };
    await writeFile(join(folder, 'report.json'), JSON.stringify(report, null, 2));
}

async function sourceFingerprint() {
    const walk = async (dir: string): Promise<string[]> => (await Promise.all((await readdir(dir, { withFileTypes: true })).map(e => e.isDirectory() ? walk(join(dir, e.name)) : [join(dir, e.name)]))).flat();
    const files = [...await walk(join(root, 'src/circuit-layout')), ...await walk(join(root, 'scripts/testing')), fileURLToPath(import.meta.url),
        join(root, 'scripts/testing/render-schematic-png.ts'), join(root, 'src/devices/symbols/symbol-parser.ts'),
        join(root, 'src/utils/circuit-merge.ts'), join(root, 'package-lock.json')];
    files.push(join(root, 'src/utils/schematic-packing.ts'));
    return digest((await Promise.all(files.sort().map(f => readFile(f, 'utf8')))).join('\n'));
}

async function main(options: ReturnType<typeof parseOptions>) {
    if (options.help) { console.log('npm run test:schematics -- [--limit N] [--filter TEXT] [--case cached.json | --cached] [--offline] [--resume] [--workers N] [--timeout SECONDS] [--no-patterns] [--no-dense-net-labels] [--group-seed N | --group-seeds N,N,...]\nWorkers: up to 4 by default (limited by available CPUs); --workers 1 runs sequentially.\nDefault group seeds: 1,2,4. Output: .test-output/new-circuit-layout/index.html; generated inputs and images stay ignored.'); return; }
    await mkdir(cache, { recursive: true });
    const fingerprint = digest(await sourceFingerprint() + JSON.stringify({ patterns: !options['no-patterns'] }));
    const candidates = options.case ? [{ file: options.case, cached: true }] : (await readdir(options.cached ? cache : options.bank!)).filter(f => f.endsWith('.json')).sort().map(file => ({ file, cached: !!options.cached }));
    const fullPages = await Promise.all((await readdir(options.bank!).catch(() => []))
        .filter(file => file.endsWith('-full.json')).map(async file => {
            const source = await readFile(join(options.bank!, file), 'utf8');
            return { file, prefix: file.slice(0, -'-full.json'.length), circuit: circuitInput(JSON.parse(source)), hash: digest(source) };
        }));
    const jobs: Job[] = [];
    for (const c of candidates) {
        const source = c.cached ? join(cache, c.file) : join(options.bank!, c.file);
        let title = c.file, externalSignals: string[] = [], pageFile: string | undefined,
            pageHash: string | undefined, matchError: string | undefined;
        try {
            const raw = await json(source);
            if (options.cached && !raw.circuit) continue;
            const circuit = circuitInput(raw.circuit ?? raw);
            title = circuit.metadata?.project_name ?? title;
            const page = fullPages.filter(full => c.file.startsWith(`${full.prefix}-`) && c.file !== `${full.prefix}-full.json`)
                .sort((a, b) => b.prefix.length - a.prefix.length)[0];
            const signals = page && pageBoundarySignals(circuit, page.circuit);
            if (signals) { externalSignals = signals; pageFile = page.file; pageHash = page.hash; }
            else if (page) matchError = `${c.file} does not contain exactly the same block components and pins as ${page.prefix}-full.json`;
        } catch { /* worker reports malformed inputs */ }
        if (options.filter && !`${c.file} ${title}`.toLowerCase().includes(options.filter.toLowerCase())) continue;
        if (matchError) throw new Error(matchError);
        jobs.push({ ...c, source, title, externalSignals, pageFile,
            pageSource: pageFile ? join(options.bank!, pageFile) : undefined, pageHash,
            id: c.file.slice(0, -5) + (options['no-patterns'] ? '-plain' : '') });
        if (options.limit && jobs.length >= Number(options.limit)) break;
    }
    if (!jobs.length) throw new Error('No matching circuits');
    // Prepare each referenced page once, before workers start, so isolated
    // cases can reuse the same frozen geometry and page-level port policy.
    await Promise.all([...new Set(jobs.map(job => job.pageFile).filter((file): file is string => !!file))]
        .map(file => loadFixture(file, join(options.bank!, file), false, !!options.offline)));
    const reports: (Report | undefined)[] = new Array(jobs.length);
    const workerCount = Math.min(Number(options.workers), jobs.length), started = performance.now();
    let completed = 0, resumed = 0;
    const cancellation = new AbortController();
    const interrupt = () => { process.exitCode = 130; cancellation.abort(); };
    const terminate = () => { process.exitCode = 143; cancellation.abort(); };
    const suffix = options['no-patterns'] ? '-plain' : '';
    const galleryPath = join(output, `index${suffix}.html`);
    const gallery = async () => {
        // Snapshot in input order even when workers finish out of order.
        const finished = reports.filter((r): r is Report => !!r);
        await writeJsonAtomic(join(output, `summary${suffix}.json`), { fingerprint, bank: options.bank, total: jobs.length,
            completed: finished.length, workers: workerCount, resumed, interrupted: cancellation.signal.aborted,
            elapsedMs: Math.round(performance.now() - started), reports: finished });
        const articles = finished.map(r => {
            const variants = r.variants;
            const metrics = variants ? `Провода: ${variants.before.physicalWireLength} → ${variants.after.physicalWireLength}; пересечения: ${variants.before.differentNetCrossings} → ${variants.after.differentNetCrossings}; время: ${variants.before.elapsedMs} → ${variants.after.elapsedMs} мс\nШирина/высота __v_root__: ${variants.before.rootAspectRatio?.toFixed(3) ?? '—'} → ${variants.after.rootAspectRatio?.toFixed(3) ?? '—'} (цель √2 ≈ 1,414)` : '';
            const previews = variants ? `<section>${(['before', 'after'] as const).map(name => `<figure>
                <figcaption>${name === 'before' ? 'До' : 'После'} · ${variants[name].valid ? 'соединения проверены' : 'ошибка соединений'} ·
                <a href="${r.id}/${name}.png">PNG проекта</a> · <a href="${r.id}/${name}.asm.json">ASM</a></figcaption>
                <pre class="${variants[name].signalCheck.valid ? '' : 'failed'}">${html(formatSignalCheck(variants[name].signalCheck))}</pre>
                <a href="${r.id}/${name}.svg"><img loading="lazy" src="${r.id}/${name}.svg"></a></figure>`).join('')}</section>` : '';
            return `<article><h2 class="${r.status}">${html(r.title)} — ${html(r.status)}</h2>
                <a href="${r.id}/report.json">Метрики и ошибки</a> · <a href="${r.id}/input.json">Исходные данные</a>
                <pre>${html(r.error ?? metrics)}</pre><p class="review">${html(r.review?.join('; '))}</p>${previews}</article>`;
        });
        await writeFile(galleryPath, `<!doctype html><meta charset="utf-8"><title>Schematic layout comparisons</title>
            <style>body{font:16px system-ui;margin:24px;background:#f5f5f5}article{background:white;padding:18px;margin:18px 0}
            section{display:flex;gap:16px}figure{margin:0;width:50%}img{width:100%;max-height:700px;object-fit:contain;object-position:top}
            pre{white-space:pre-wrap}.failed,.error{color:#b00}.review{color:#945600}</style><h1>Схемы: до / после</h1>
            <p>${finished.length} / ${jobs.length}. Воркеров: ${workerCount}. Паттерны: ${options['no-patterns'] ? 'выключены в обоих вариантах' : 'исходный каталог → расширенный каталог'}.
            Прямоугольники обозначают размеры символов. PNG — существующий рендер проекта; SVG — сравнение с именами цепей. Клик открывает полный размер.</p>${articles.join('')}`);
    };
    const saveGallery = coalescedWriter(gallery);
    const execute = async (job: Job): Promise<Report> => {
        const folder = join(output, job.id); await mkdir(folder, { recursive: true });
        const old = await json(join(folder, 'report.json')).catch(() => null);
        const inputHash = digest(await readFile(job.source, 'utf8').catch(() => 'unreadable')
            + JSON.stringify({ externalSignals: job.externalSignals, pageHash: job.pageHash }));
        if (options.resume && old?.fingerprint === fingerprint && old?.inputHash === inputHash && old?.status === 'ok') {
            resumed++; return { ...old, id: job.id };
        } else {
            await rm(join(folder, 'report.json'), { force: true });
            const jobPath = join(folder, 'job.json'); await writeFile(jobPath, JSON.stringify(job));
            const args = ['--import', 'tsx', fileURLToPath(import.meta.url), '--worker', jobPath, '--fingerprint', fingerprint];
            for (const flag of ['offline', 'no-patterns'] as const) if (options[flag]) args.push(`--${flag}`);
            if (options['group-seed']) args.push('--group-seed', options['group-seed']);
            if (options['group-seeds']) args.push('--group-seeds', options['group-seeds']);
            if (options['dense-net-labels']) args.push('--dense-net-labels');
            if (options['no-dense-net-labels']) args.push('--no-dense-net-labels');
            const run = await runIsolated(args, { cwd: root, timeoutMs: Number(options.timeout) * 1000, signal: cancellation.signal });
            await writeFile(join(folder, 'run.log'), run.log);
            const report = run.code === 0 && !run.timedOut && !run.interrupted ? await json(join(folder, 'report.json')) : { file: job.file, title: job.title,
                status: run.interrupted ? 'interrupted' : run.timedOut ? 'timeout' : 'error',
                error: run.interrupted ? 'Run interrupted' : run.timedOut ? `Exceeded ${options.timeout} seconds` : run.log.slice(-3000) };
            Object.assign(report, { id: job.id, inputHash, fingerprint });
            return report;
        }
    };
    process.once('SIGINT', interrupt); process.once('SIGTERM', terminate);
    console.log(`Circuits: ${jobs.length}; workers: ${workerCount}; timeout: ${options.timeout}s per circuit`);
    try {
        await parallelJobs(jobs, workerCount, async (job, index) => {
            let report: Report;
            try { report = await execute(job); }
            catch (error) { report = { id: job.id, file: job.file, title: job.title, fingerprint, status: 'error', error: String(error) }; }
            await writeJsonAtomic(join(output, job.id, 'report.json'), report);
            reports[index] = report; completed++;
            console.log(`[${completed}/${jobs.length}] ${report.status}: ${job.title}`);
            await saveGallery();
        }, cancellation.signal);
        await saveGallery();
    } finally {
        process.removeListener('SIGINT', interrupt); process.removeListener('SIGTERM', terminate);
    }
    console.log(`Finished ${completed}/${jobs.length}; resumed: ${resumed}; wall time: ${((performance.now() - started) / 1000).toFixed(1)}s\nGallery: ${galleryPath}`);
    if (!cancellation.signal.aborted && reports.some(r => r?.status !== 'ok')) process.exitCode = 1;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
    const options = parseOptions(process.argv.slice(2));
    await (options.worker ? worker(options) : main(options)).catch(e => { console.error(e); process.exitCode = 1; });
}
