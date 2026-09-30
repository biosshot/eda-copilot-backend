// Offline precision experiment. Production Rust sources, addon and fixtures stay unchanged.
import { readFileSync, writeFileSync, mkdirSync, readdirSync, existsSync } from 'node:fs';
import { resolve, join, relative } from 'node:path';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import assert from 'node:assert/strict';

const root = resolve('debugging/block-precision-2026-09-30');
const native = resolve('native/pcb-board-packer');
const evaluateOnly = process.argv.includes('--evaluate-only');
const capture = resolve(process.argv.slice(2).find(arg => !arg.startsWith('--')) ?? 'debugging/pcb-layout/runs/PortableScope/2026-09-29T12-02-18-665Z/native/block/process-19228-thread-0/00091-fpga_1v1_caps_fpga_2v5_caps_fpga_1v8_caps_fpga_3v3_caps_fpga_cor-9db75277e8');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const save = (path, value) => writeFileSync(path, JSON.stringify(value, null, 2));
function writeSource(path, text) {
  if (!existsSync(path) || readFileSync(path, 'utf8') !== text) writeFileSync(path, text);
}
mkdirSync(root, { recursive: true });

const main = `#![allow(dead_code)]
mod block_solver; mod fast_route; mod geometry; mod lazy_rank; mod model;
mod micro_router; mod net_class; mod ordinary_net; mod post_place; mod signal_path;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let problem: model::BlockSolveProblem = serde_json::from_slice(&std::fs::read(&args[2])?)?;
    problem.validate(4)?;
    let output = if args[1] == "solve" {
        serde_json::to_value(block_solver::solve_block(problem)?)?
    } else {
        let seeds: Vec<model::BlockPairSeed> = serde_json::from_slice(&std::fs::read(&args[4])?)?;
        block_solver::experiment_evaluate(problem, seeds)?
    };
    std::fs::write(&args[3], serde_json::to_vec_pretty(&output)?)?;
    Ok(())
}
`;

function instrument(source) {
  const signature = 'pub fn solve_block(problem: BlockSolveProblem) -> Result<crate::model::BlockSolveSolution, String> {';
  assert.equal(source.split(signature).length, 2);
  const split = source.indexOf('    let profile = std::env::var_os("PCB_BLOCK_SOLVER_PROFILE").is_some();');
  assert(split > 0);
  source = source.slice(0, split).replace(signature,
    'fn experiment_context(problem: BlockSolveProblem) -> (Context, Vec<WorkingPrimitive>) {')
    + `    (context, primitives)
}

pub fn experiment_evaluate(problem: BlockSolveProblem, seeds: Vec<crate::model::BlockPairSeed>) -> Result<serde_json::Value, String> {
    let (context, sources) = experiment_context(problem);
    let items: Vec<_> = seeds.into_iter().map(|seed| {
        let mut item = sources.iter().find(|p| p.primitive.id == seed.primitive.id).expect("captured inventory").clone();
        item.primitive = seed.primitive;
        item.rotation = seed.rotation;
        rebuild_component_geometry(&mut item);
        item
    }).collect();
    let mut result = serde_json::to_value(solution(&context, &items)?).map_err(|e| e.to_string())?;
    let mut overlaps = Vec::new();
    for i in 0..items.len() { for j in (i+1)..items.len() {
        if !primitive_can_conflict(&items[i], &items[j], &context) { continue; }
        let depth = primitive_overlap_depth(&items[i], &items[j], &context);
        if depth > 0.0 { overlaps.push(serde_json::json!({"a":items[i].primitive.id,"b":items[j].primitive.id,"depthMm":depth})); }
    }}
    // Separately reconstruct intended 0.001 mm poses using untouched F64 source geometry.
    // This distinguishes float-storage boundary artifacts from layout quality changes.
    let rebuilt: Vec<_> = items.iter().map(|item| {
        let source = sources.iter().find(|p| p.primitive.id == item.primitive.id).unwrap();
        let rotated = rotate_primitive(source, item.rotation);
        let first = item.primitive.placements.first().unwrap();
        let old_first = rotated.primitive.placements.first().unwrap();
        translate_primitive(&rotated, round_placement(first.x) - old_first.x, round_placement(first.y) - old_first.y)
    }).collect();
    result["diagnostics"] = serde_json::json!({"rawOverlapPenalty":overlap_penalty(&items,&context,None),
        "rawOverlapPairs":overlaps,"reconstructedFromRoundedPlacements":solution(&context,&rebuilt)?});
    Ok(result)
}

${signature}
    let (context, primitives) = experiment_context(problem);
` + source.slice(split);
  const checkpoint = '        checkpoints.push(crate::model::BlockCheckpoint { stage, result: solution(&context, &centered)? });';
  assert(source.includes(checkpoint));
  source = source.replace(checkpoint, `        eprintln!("PRECISION_GEOMETRY {}", serde_json::json!({"stage":stage,"seeds":centered.iter()
            .map(|p| crate::model::BlockPairSeed { primitive:p.primitive.clone(), rotation:p.rotation }).collect::<Vec<_>>()}));
${checkpoint}`);
  const layer = '        if states.is_empty() {\n            break;\n        }';
  assert(source.includes(layer));
  return source.replace(layer, `        eprintln!("PRECISION_LAYER {}", serde_json::json!({"depth":states.first().map(|s|s.placed.len()),
            "states":states.iter().map(|s|serde_json::json!({"poses":trace::poses(&s.placed),"score":s.score,
                "hard":s.hard_violations,"ordinal":s.ordinal})).collect::<Vec<_>>()}));
${layer}`);
}

const sourceManifest = [];
function copySources(from, to, precision) {
  mkdirSync(to, { recursive: true });
  for (const item of readdirSync(from, { withFileTypes: true })) {
    if (item.isDirectory()) { copySources(join(from, item.name), join(to, item.name), precision); continue; }
    if (!item.name.endsWith('.rs') || item.name === 'lib.rs') continue;
    const original = readFileSync(join(from, item.name), 'utf8');
    if (precision === 'f64') sourceManifest.push({ path: relative(native, join(from, item.name)), sha256: hash(original) });
    const normalized = original.replace(/\r\n/g, '\n');
    let text = item.name === 'block_solver.rs' ? instrument(normalized) : normalized;
    if (precision === 'f32') {
      text = text.replace(/\bf64\b/g, 'f32')
        .replace(/(?<=\d)_?f64\b/g, 'f32')
        .replace(/f32::from\(([^()]*)\)/g, '(($1) as f32)')
        .replace(/\.to_bits\(\)/g, '.to_bits() as u64')
        .replace(/\.as_f64\(\)/g, '.as_f64().map(|value| value as f32)');
    }
    writeSource(join(to, item.name), text);
  }
}

function execute(command, args, logName) {
  const started = performance.now();
  const child = spawnSync(command, args, { encoding: 'utf8', windowsHide: true, maxBuffer: 128 * 1024 * 1024 });
  writeFileSync(join(root, logName), (child.stdout ?? '') + (child.stderr ?? ''));
  if (child.error || child.status !== 0) {
    console.error((child.stderr ?? '').slice(-14000));
    throw child.error ?? Error(`${command} exited ${child.status}; see ${logName}`);
  }
  return { wallMs: performance.now() - started, stderr: child.stderr };
}

const problemBytes = readFileSync(join(capture, 'problem.json'));
writeFileSync(join(root, 'problem.json'), problemBytes);
for (const precision of ['f64', 'f32']) {
  const dir = join(root, precision);
  copySources(join(native, 'src'), join(dir, 'src'), precision);
  writeSource(join(dir, 'src/main.rs'), main);
  writeSource(join(dir, 'Cargo.toml'), `[package]
name = "block-precision-${precision}"
version = "0.0.0"
edition = "2021"
publish = false
[dependencies]
rustc-hash = "2.1.3"
serde = { version = "1", features = ["derive", "rc"] }
serde_json = "1"
smallvec = "1.15"
[profile.release]
lto = "thin"
codegen-units = 1
panic = "abort"
strip = "symbols"
`);
  // Seed dependency versions from the production lockfile; Cargo adjusts only the root package.
  writeFileSync(join(dir, 'Cargo.lock'), readFileSync(join(native, 'Cargo.lock')));
  console.log(`Building isolated ${precision} solver`);
  execute('cargo', ['build', '--offline', '--release', '--manifest-path', join(dir, 'Cargo.toml')], `${precision}-build.log`);
}
if (!evaluateOnly) save(join(root, 'manifest.json'), { capture, problemHash: hash(problemBytes), sourceManifest,
  method: 'Full native CPU search in f64 versus f32; all source floating types/math converted, same formulas, epsilons, search parameters. Not a GPU benchmark.' });

const rows = evaluateOnly ? JSON.parse(readFileSync(join(root, 'runs.json'), 'utf8')) : {};
for (const precision of ['f64', 'f32']) {
  const binary = join(root, precision, 'target/release', `block-precision-${precision}.exe`);
  console.log(evaluateOnly ? `Re-evaluating saved ${precision} search` : `Running full ${precision} search`);
  const run = evaluateOnly ? { wallMs: rows[precision].wallMs, stderr: readFileSync(join(root, `${precision}-search.log`), 'utf8') }
    : execute(binary, ['solve', join(root, 'problem.json'), join(root, `${precision}-solution.json`)], `${precision}-search.log`);
  const solution = JSON.parse(readFileSync(join(root, `${precision}-solution.json`), 'utf8'));
  rows[precision] = { wallMs: run.wallMs, binaryHash: evaluateOnly ? rows[precision].binaryHash : hash(readFileSync(binary)), solution,
    layers: run.stderr.split(/\r?\n/).filter(line => line.startsWith('PRECISION_LAYER ')).map(line => JSON.parse(line.slice(16))) };
  for (const line of run.stderr.split(/\r?\n/).filter(line => line.startsWith('PRECISION_GEOMETRY '))) {
    const { stage, seeds } = JSON.parse(line.slice(19));
    const seedsPath = join(root, `${precision}-${stage}-geometry.json`);
    save(seedsPath, seeds);
    execute(join(root, 'f64/target/release/block-precision-f64.exe'), ['evaluate', join(root, 'problem.json'),
      join(root, `${precision}-${stage}-f64-evaluation.json`), seedsPath], `${precision}-${stage}-evaluate.log`);
  }
  console.log(`${precision} completed: hard=${solution.rank.hardCount}, score=${solution.rank.score}`);
}
const reference = JSON.parse(readFileSync(join(capture, 'solution.json'), 'utf8'));
assert.deepEqual(rows.f64.solution, reference, 'Experimental F64 must exactly reproduce captured CPU result');
save(join(root, 'runs.json'), rows);
// Same-layout control: evaluate the untouched F64 final geometry with F32 arithmetic.
execute(join(root, 'f32/target/release/block-precision-f32.exe'), ['evaluate', join(root, 'problem.json'),
  join(root, 'f64-pairs-f32-evaluation.json'), join(root, 'f64-pairs-geometry.json')], 'f64-pairs-f32-evaluate.log');
save(join(root, 'evaluation-manifest.json'), { binaryHash: hash(readFileSync(join(root, 'f64/target/release/block-precision-f64.exe'))),
  f32BinaryHash: hash(readFileSync(join(root, 'f32/target/release/block-precision-f32.exe'))),
  scriptHash: hash(readFileSync(new URL(import.meta.url))), problemHash: hash(problemBytes) });
console.log(`Exact F64 reference confirmed. Results: ${root}`);
