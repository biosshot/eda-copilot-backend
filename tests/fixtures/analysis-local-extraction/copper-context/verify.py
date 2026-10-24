"""Final saved-packet custody/static gates; never calls a solver or build."""
from pathlib import Path
import json,hashlib,collections,py_compile,subprocess
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent
OUT=ROOT/'debugging/p7-design-checkpoint/copper-context';read=lambda p:json.loads(p.read_text(encoding='utf-8'));sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
a=read(OUT/'audit.json');checks=[]
assert len(a['boards'])==8 and all(x['islandSourceMembershipSame'] for x in a['boards'])
old=read(OUT.parent/'copper-geometry/corpus-baseline.json')
for path,h in old['inputs'].items():assert sha(Path(path))==h
def shape_counts(r):
    v=collections.Counter()
    for p in r['regions']:v[(p['source']['path'],p['layer'],'regions')]+=1;v[(p['source']['path'],p['layer'],'holes')]+=len(p['polygon']['holes'])
    return v
for b in a['boards']:
    x,y=[read(OUT/(b['name']+'-'+g+'-opt.json'))['report'] for g in ['G0','G1']]
    assert shape_counts(x)==shape_counts(y),b['name']
    for r in [x,y]:assert r['status']=='partial' and not r['excluded']
    assert x['sources']==y['sources'] and x['outline']==y['outline']
checks.append('8 frozen input/preparation hashes, source IDs, per-source fragment/hole counts and reliable island membership')
changes=read(OUT/'contact-differences.json');assert len(changes)==9
assert all(not q['reliable'] for c in changes for q in c['before']+c['after'])
assert read(OUT/'shared-cap-regression.json')['newlySharedExactVertex']
checks.append('only 9 non-reliable contact relabels; exact corrected shared cap')
for b in a['coupon']:
    assert len(b['runs'])==3 and b['batch']['exitCode']==0 and b['batch']['refusal'] is None
    assert b['batch']['wallSeconds']<=120 and b['batch']['peakSubtreeBytes']<=512*1024**2
    for r in b['runs']:assert r['fullScreen']<=.05 and r['aggregateFarBudget']<=.01 and r['rawReciprocity']<=1e-9 and min(r['physicalEigenvaluesH'])>0
assert all(r['screen']<=.002 for r in a['postfixNear'])
assert a['measuredMeanSpeedup']>1 and a['speedupNonoverlapLowerRatio']>1
checks.append('exactly two bounded batches; independent full/near, reciprocity/PSD, action bounds, zero/weak and all timing samples')
for b in a['plans']:
    assert b['sameGeometry'] and b['G1']['nearPairs']-b['G2']['nearPairs']==b['G2']['promotedPairs']
    assert b['G1']['nearPairs']+b['G1']['farPairs']==b['G2']['nearPairs']+b['G2']['farPairs']
checks.append('same post-fix geometry for all ROI G1/G2 count plans; pair totals retained')
for meta in ['optimized-build.json','kernel-build.json','addon-build.json']:
    v=read(OUT/meta);sources=v.get('actualSourceHashes',v.get('sources',v.get('sourceHashes',{})))
    for path,h in sources.items():assert sha(ROOT/path)==h,(meta,path)
assert read(OUT/'addon-build.json')['exitCode']==0 and read(OUT/'package-smoke.json')['exportsPresent']
assert read(OUT/'addon-build.json')['binarySha256']==read(OUT/'addon-build.json')['dllSha256']
checks.append('measured/normal-addon actual source hashes match; default-GPU build and Node ABI smoke')
task='docs/experimental/analyze/next-task-copper-geometry-and-pairs.md'
assert sha(ROOT/task)==sha(ROOT.parent/'eda-copilot-backend'/task)
for p in FIX.glob('*.py'):py_compile.compile(str(p),doraise=True)
diff=subprocess.run(['git','diff','--check'],cwd=ROOT,capture_output=True);assert diff.returncode==0,diff.stderr
(OUT/'static-verification.json').write_text(json.dumps({'checks':checks,'pass':True,'kernelCalls':0,'buildCalls':0,'fullROIAdmitted':False},indent=2),encoding='utf-8')
paths=[p for p in OUT.iterdir() if p.is_file() and p.name!='manifest.json']+[p for p in FIX.iterdir() if p.is_file()]
paths += [ROOT/task,ROOT/'docs/experimental/analyze/copper-context-and-pairs-verification.md',ROOT/'CHANGELOG.md',ROOT/'debugging/agent-handoff.md',ROOT/'native/analyze/Cargo.toml',ROOT/'native/analyze/Cargo.lock',ROOT/'native/node-addon/Cargo.lock',ROOT/'native/analyze/examples/copper_context.rs',ROOT/'native/analyze/tests/geometry.rs',ROOT/'native/analyze/tests/magnetic_near_far.rs']
paths += [ROOT/path for path in read(OUT/'optimized-build.json')['actualSourceHashes']]
paths += [Path(path) for path in old['inputs']]
paths += [FIX.parent/'copper-geometry/regressions.json',FIX.parent/'copper-geometry/construction-613.json',FIX.parent/'magnetic-near-far/coupon.json',FIX.parent/'magnetic-near-far/reference.json',FIX.parent/'magnetic-stability/closed-reference.json',OUT.parent/'copper-partition/batch.jsonl',OUT.parent/'magnetic-near-far/coupon-run/stdout.json',ROOT/'native/node-addon/eda-copilot-node.win32-x64-msvc.node']
hashes={str(p):sha(p) for p in paths};assert all(sha(Path(p))==h for p,h in hashes.items())
(OUT/'manifest.json').write_text(json.dumps({'hashes':hashes,'mismatchCount':0,'kernelCallsInStaticCheck':0,'fullP7Closed':False},indent=2),encoding='utf-8')
print(json.dumps({'savedChecks':len(checks),'hashes':len(hashes),'mismatches':0,'solverCalls':0}))
