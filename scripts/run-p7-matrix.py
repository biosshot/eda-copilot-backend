"""P7 v1 stage 6 matrix runner (and stage 5 scenario runner) under the existing process-tree supervisor.

Items (select with --rows, comma separated tokens; a token matches an id by prefix or substring):
  <row id>        one of the 39 rows of caseTraceability in tests/fixtures/analysis-p7-closure/manifest.json,
                  run through its mapped short controls (scripts, node tests; native Rust controls need --native)
  s5/<id>@<hz>    stage 5 scenario of tests/fixtures/analysis-p7-stage5/manifest.json (pdn-multiport, coupled-pair, band-step)
  board/<id>      one of the five real boards of docs/analyze/reference/p7-extraction/p7-hybrid-final-queue.json
Stage 5 scenarios are also attached to their matrix rows (manifest field matrixRows), so a full matrix run covers them.

Modes: cold = fresh empty cache directory, one process.  warm = same process repeats the scenario (in-process cache);
restart = a new process on the cache directory of the cold run (disk cache; the cold run is made first when missing).
Warm needs a scenario control (--repeat); restart needs a cache-aware control (scenario or board). Other controls report
`skipped` for those modes. Results resume from <out>/results.json; use --rerun to repeat finished items.
Wall times are contended (shared machine); peak private bytes are 20 ms process-tree samples (lower bound).
Physical accuracy of every model stays `unverified`.
"""
import argparse, importlib.util, json, math, os, re, subprocess, sys, time
from pathlib import Path
import psutil

root = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('p7_supervisor', root / 'scripts/verify-p7-automatic-supervised.py')
sup = importlib.util.module_from_spec(spec); spec.loader.exec_module(sup)
MIB = 1024 ** 2
NODE = ['node', '--import', 'tsx']
REFUSAL_CODE = re.compile(r'(UNSUPPORTED|UNRESOLVED|DISCONNECTED|NOT_IMPLEMENTED|REFUSED|MAPPING|REJECTED|INPUT|SCOPE|INVALID_POLYGON|polygon)', re.I)


def load(path):
    return json.loads((root / path).read_text(encoding='utf-8'))


def control_command(ref, native):
    """Command list for one mapped short control, or (None, reason)."""
    path, _, symbol = ref.partition('#')
    if path.endswith('.rs'):
        if not native:
            return None, 'native Rust control; pass --native to run cargo test'
        base = ['cargo', 'test', '--manifest-path', 'native/analyze/Cargo.toml']
        if path.startswith('native/analyze/tests/'):
            return base + ['--test', Path(path).stem], None
        module = path.removeprefix('native/analyze/src/').removesuffix('.rs').replace('/', '::')
        return base + ['--lib', module + ('::' + symbol if symbol else '')], None
    if path.endswith('.test.ts') or path.endswith('.test.mjs'):
        return NODE + ['--test', path], None
    if path.endswith(('.ts', '.mjs')):
        return NODE + [path], None
    if path.endswith('.py'):
        return ['python', path], None
    return None, 'unknown control type'


def build_items(native, prepare):
    closure = load('tests/fixtures/analysis-p7-closure/manifest.json')
    stage5 = load('tests/fixtures/analysis-p7-stage5/manifest.json')
    queue = load('docs/analyze/reference/p7-extraction/p7-hybrid-final-queue.json')
    items = {}
    for row in closure['caseTraceability']['rows']:
        items[row['id']] = dict(id=row['id'], title=row['case'], kind='row', commands=[], notes=[], caps=set())
        for ref in row['implementation']['shortControls']:
            cmd, why = control_command(ref, native)
            if cmd: items[row['id']]['commands'].append(dict(cmd=cmd, label=ref, caps=set()))
            else: items[row['id']]['notes'].append(ref + ': ' + why)
    for scenario in stage5['scenarios']:
        for band in scenario['bandsHz']:
            sid = 's5/%s@%d' % (scenario['id'], band)
            flags = ['--band-hz', str(int(band))]
            if prepare: flags.append('--prepare-only')
            command = dict(cmd=NODE + ['scripts/verify-p7-stage5-scenario.ts', scenario['id']] + flags, label=sid,
                           caps={'warm', 'restart'} if not prepare else set(), repeatable=not prepare, scenario=True)
            items[sid] = dict(id=sid, title=scenario['intent'], kind='scenario', commands=[command], notes=[], caps=command['caps'])
            if scenario['id'] == 'control-vin-channel': continue
            for row_id in scenario.get('matrixRows', []):
                if row_id in items and band == scenario['bandsHz'][0]:
                    items[row_id]['commands'].append(command)  # attached scenario, shares the result of its own item
    for board in queue['cases']:
        bid = 'board/' + board['id']
        command = dict(cmd=NODE + ['scripts/verify-p7-queue-board.ts', board['id']], label=bid, caps={'restart'}, board=True)
        items[bid] = dict(id=bid, title='real board ' + board['id'], kind='board', commands=[command], notes=[], caps={'restart'})
    return items


def selected(items, filters):
    if not filters: return [k for k, v in items.items() if v['kind'] == 'row']
    picked = []
    for f in filters:  # an exact id wins over prefix/substring matches, so s5/band-step@100000000 does not pull @1000000000
        hits = [k for k in items if k == f] or [k for k in items if k.startswith(f)] or [k for k in items if f in k]
        picked += [k for k in hits if k not in picked]
    return picked


def run_command(command, folder, cache_dir, args, seconds, mode, repeat):
    folder.mkdir(parents=True, exist_ok=True); cache_dir.mkdir(parents=True, exist_ok=True)
    cmd = list(command['cmd'])
    if repeat and command.get('repeatable'): cmd += ['--repeat', '2']
    env = {**os.environ, 'P7_OUTPUT_DIR': str(folder), 'PCB_ANALYSIS_CACHE_DIR': str(cache_dir), 'P7_TREE_CAP_MIB': str(args.memory_mib),
           'P7_PHASE_TRACE': '1', 'P7_VERIFY_SECONDS': str(int(seconds)), 'P7_SUPERVISOR_TEST_TIMEOUT': str(int(seconds)),
           'PYTHONUTF8': '1'}
    if command.get('board'): env['P7_SUPERVISED_BOARD'] = '1'
    if mode == 'restart': env['P7_SMOKE_MODE'] = 'restart'
    cap = args.memory_mib * MIB; reserve = MIB * 1024
    head = sup.windows_commit_headroom()
    reason = sup.memory_refusal(True, psutil.virtual_memory().available, head, 0, cap, reserve, True)
    record = dict(cmd=cmd, label=command['label'], cache=str(cache_dir), folder=str(folder), peakPrivateMiB=0, peakRssMiB=0)
    if reason:
        return {**record, 'status': 'resource-failure', 'reason': 'preflight: ' + reason, 'wallSeconds': 0}
    start = time.monotonic(); rss = private = 0; reason = None
    with (folder / 'stdout.txt').open('w', encoding='utf-8', errors='replace') as log:
        p = subprocess.Popen(cmd, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT,
                             creationflags=getattr(subprocess, 'CREATE_NO_WINDOW', 0))
        proc = psutil.Process(p.pid)
        while p.poll() is None:
            children = proc.children(recursive=True)
            r, _, v = sup.tree_memory(proc, children, True); rss = max(rss, r); private = max(private, v)
            reason = sup.memory_refusal(True, psutil.virtual_memory().available, sup.windows_commit_headroom(), v, cap, reserve)
            if time.monotonic() - start > seconds: reason = '%gs row cap' % seconds
            if reason:
                for child in reversed(children):
                    try: child.kill()
                    except psutil.Error: pass
                p.kill(); break
            time.sleep(.02)
        p.wait()
    wall = time.monotonic() - start
    status = ('timeout' if reason.endswith('row cap') else 'resource-failure') if reason else ('pass' if p.returncode == 0 else 'fail')
    return {**record, 'status': status, 'reason': reason, 'exitCode': p.returncode, 'wallSeconds': round(wall, 2),
            'peakPrivateMiB': round(private / MIB), 'peakRssMiB': round(rss / MIB)}


def inspect(record):
    """Stage, error code and tier from the retained output; the tier follows plan section 2."""
    folder = Path(record['folder'])
    text = ''
    try: text = (folder / 'stdout.txt').read_text(encoding='utf-8', errors='replace')
    except OSError: pass
    lines = text.splitlines()
    phases = [l for l in lines if l.startswith(('P7_PHASE ', 'P7_COMPACT_STAGE'))]
    record['stage'] = (phases[-1][:90] if phases else '-')
    code = None; detail = {}
    for name in ('scenario-result.json', 'prepare.json', 'board-result.json'):
        try: detail = json.loads((folder / name).read_text(encoding='utf-8')); break
        except (OSError, ValueError): continue
    if detail:
        err = detail.get('error') if isinstance(detail.get('error'), dict) else {}
        code = detail.get('errorCode') or err.get('code')
        record['modelStatus'] = detail.get('status')
        passes = detail.get('passes') or []
        if passes: record['passes'] = [dict(pass_=x['pass'], wallSeconds=round(x['wallSeconds'], 2), cache=x.get('cache'), dslStatus=x['dslStatus']) for x in passes]
        if detail.get('ports'): record['ports'] = detail['ports']
        if detail.get('frequenciesHz'): record['frequenciesHz'] = detail['frequenciesHz']
        record['message'] = (detail.get('errorMessage') or err.get('message') or '')[:300]
    if not code and record['status'] == 'fail':
        for line in reversed(lines[-60:]):
            m = re.search(r"code: '([A-Z][A-Z0-9_]+)'", line) or re.search(r'\b([A-Z][A-Z0-9]+(?:_[A-Z0-9]+)+)\b', line)
            if m and not line.startswith('P7_'): code = m.group(1); break
        if not code and lines: record['message'] = lines[-1][:300]
    record['errorCode'] = code or ('exit %s' % record.get('exitCode') if record['status'] == 'fail' else None)
    if record['status'] == 'pass':
        record['tier'] = ('planned (no solve)' if detail.get('status') == 'planned' else ('model' if detail else 'control passed')
                          if detail.get('status') in (None, 'model') else 'refusal (tier 3)')
        if 'approx' in text.lower() and record['tier'] == 'model': record['tier'] = 'model+approximation'
    elif record['status'] in ('timeout', 'resource-failure'): record['tier'] = 'budget (no answer; not a physical refusal)'
    else:
        record['tier'] = 'refusal (tier 3)' if (record['errorCode'] and REFUSAL_CODE.search(str(record['errorCode']))) or detail.get('status') in ('refused', 'planned') else 'failure (unclassified)'
    return record


def aggregate(item, commands):
    if not commands:
        return 'no-control' if not item['notes'] else 'native-not-run', 'n/a (' + ('no executable control mapped' if not item['notes'] else '; '.join(item['notes'])[:120]) + ')'
    statuses = [c['status'] for c in commands]
    for bad in ('resource-failure', 'timeout', 'fail', 'skipped'):
        if bad in statuses: return bad, next(c['tier'] if 'tier' in c else '-' for c in commands if c['status'] == bad)
    tiers = [c.get('tier', '-') for c in commands]
    return 'pass', ('refusal (tier 3)' if any(t.startswith('refusal') for t in tiers) else tiers[0])


def write_summary(out, results, mode_filter):
    (out / 'results.json').write_text(json.dumps(results, indent=1, ensure_ascii=False), encoding='utf-8')
    lines = ['# P7 v1 matrix run', '', 'Wall times are contended (shared machine). Peak private is a 20 ms sampled lower bound. Physical accuracy: unverified.', '',
             '| item | mode | status | tier | wall, s | peak private, MiB | stage / error code |', '|---|---|---|---|---:|---:|---|']
    for key in sorted(results['items']):
        r = results['items'][key]
        stage = ' / '.join(x for x in [r.get('stage'), r.get('errorCode')] if x and x != '-') or '-'
        lines.append('| %s | %s | %s | %s | %s | %s | %s |' % (r['id'], r['mode'], r['status'], r['tier'], r.get('wallSeconds', ''), r.get('peakPrivateMiB', ''), stage.replace('|', '/')[:140]))
    (out / 'summary.md').write_text('\n'.join(lines) + '\n', encoding='utf-8')


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--mode', choices=['cold', 'warm', 'restart'], default='cold')
    ap.add_argument('--rows', default='', help='comma separated ids/prefixes; default: the 39 matrix rows')
    ap.add_argument('--row-cap', type=float, default=900, help='seconds per command (default 900)')
    ap.add_argument('--budget', type=float, default=6 * 3600, help='global seconds; later items are recorded as not-run')
    ap.add_argument('--memory-mib', type=int, default=8192, help='process tree private cap')
    ap.add_argument('--out', default='debugging/p7-v1-stage6')
    ap.add_argument('--native', action='store_true', help='also run native Rust controls with cargo test (compiles the addon crate)')
    ap.add_argument('--prepare', action='store_true', help='stage 5 scenarios: plan ports/blocks only, no solve')
    ap.add_argument('--rerun', action='store_true', help='repeat items already in results.json')
    ap.add_argument('--dry-run', action='store_true', help='print the plan, run nothing')
    args = ap.parse_args()
    if not 256 <= args.memory_mib <= 65536 or not math.isfinite(args.row_cap) or args.row_cap <= 0: ap.error('invalid cap')
    out = root / args.out; out.mkdir(parents=True, exist_ok=True)
    items = build_items(args.native, args.prepare)
    chosen = selected(items, [t for t in args.rows.split(',') if t])
    results = json.loads((out / 'results.json').read_text(encoding='utf-8')) if (out / 'results.json').exists() else dict(items={})
    if args.dry_run:
        for key in chosen:
            item = items[key]
            print('%-44s %-8s %d command(s) %s' % (key, item['kind'], len(item['commands']), '' if item['commands'] else '| ' + ('; '.join(item['notes']) or 'no control')))
            for c in item['commands']: print('    ', ' '.join(c['cmd']))
        print('mode=%s items=%d rowCap=%gs budget=%gs memory=%dMiB out=%s' % (args.mode, len(chosen), args.row_cap, args.budget, args.memory_mib, out))
        return 0
    started = time.monotonic(); shared = {}
    for key in chosen:
        item = items[key]; rkey = args.mode + '/' + key
        if rkey in results['items'] and not args.rerun and results['items'][rkey]['status'] != 'not-run':
            print('resume: %s already %s' % (rkey, results['items'][rkey]['status']), flush=True); continue
        remaining = args.budget - (time.monotonic() - started)
        base = dict(id=key, mode=args.mode, title=item['title'], kind=item['kind'], commands=[])
        if remaining <= 1:
            results['items'][rkey] = {**base, 'status': 'not-run', 'tier': 'n/a (global budget exhausted)'}; write_summary(out, results, args.mode); continue
        commands = []
        for index, command in enumerate(item['commands']):
            if args.mode == 'warm' and 'warm' not in command['caps']:
                commands.append(dict(label=command['label'], status='skipped', tier='n/a (warm needs a scenario control)', reason='mode not supported')); continue
            if args.mode == 'restart' and 'restart' not in command['caps']:
                commands.append(dict(label=command['label'], status='skipped', tier='n/a (restart needs a cache-aware control)', reason='mode not supported')); continue
            safe = re.sub(r'[^A-Za-z0-9_.@-]+', '_', command['label'])
            folder = out / args.mode / safe; cache_dir = out / 'cold' / safe / 'cache' if args.mode == 'restart' else folder / 'cache'
            token = (args.mode, command['label'])
            if token in shared: commands.append({**shared[token], 'sharedWith': 'earlier item in this invocation'}); continue
            if args.mode == 'restart' and not (cache_dir.exists() and any(cache_dir.iterdir())):
                cold = out / 'cold' / safe
                print('restart: no cold cache for %s, running the cold pass first' % command['label'], flush=True)
                inspect(run_command(command, cold, cold / 'cache', args, min(args.row_cap, max(remaining, 1)), 'cold', False))
            seconds = min(args.row_cap, args.budget - (time.monotonic() - started))
            if seconds <= 1: commands.append(dict(label=command['label'], status='not-run', tier='n/a (global budget exhausted)')); continue
            print('run %s [%s] cap %gs: %s' % (key, args.mode, seconds, ' '.join(command['cmd'])), flush=True)
            record = inspect(run_command(command, folder, cache_dir, args, seconds, args.mode, args.mode == 'warm'))
            shared[token] = record; commands.append(record)
        status, tier = aggregate(item, commands)
        worst = max(commands, key=lambda c: c.get('wallSeconds', 0), default={})
        results['items'][rkey] = {**base, 'commands': commands, 'status': status, 'tier': tier,
            'wallSeconds': round(sum(c.get('wallSeconds', 0) for c in commands), 2), 'peakPrivateMiB': max([c.get('peakPrivateMiB', 0) for c in commands] or [0]),
            'stage': worst.get('stage'), 'errorCode': next((c.get('errorCode') for c in commands if c.get('errorCode')), None), 'notes': item['notes']}
        write_summary(out, results, args.mode)
        print('  -> %s | %s | %ss | %s MiB' % (status, tier, results['items'][rkey]['wallSeconds'], results['items'][rkey]['peakPrivateMiB']), flush=True)
    write_summary(out, results, args.mode)
    print('summary:', out / 'summary.md')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
