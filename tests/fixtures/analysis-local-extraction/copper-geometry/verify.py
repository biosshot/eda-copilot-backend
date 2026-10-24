"""Saved-data custody checks only; no repeat numerical simulation."""
from pathlib import Path
import hashlib,json,py_compile
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent
OUT=ROOT/'debugging/p7-design-checkpoint/copper-geometry';OLD=OUT.parent
read=lambda p:json.loads(p.read_text(encoding='utf-8'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
a=read(OUT/'audit.json');c=read(OUT/'corpus-baseline.json');checks=[]
assert a['status']=='A_BLOCKED_B_NOT_STARTED' and a['kernelEvaluations']==0
assert all(r['exactCoordinates'] and r['symmetricDifferenceMm2']==0 for r in a['replays'])
checks.append('five native full source polygon replays exact')
assert all(v['ulpResidual']==0 for v in a['intersections']) and a['twoSourceCdtReproduces613']
checks.append('independent Decimal intersections and exact two-source CDT613')
assert a['rejectedHullBridge']['nativeOverlapAreaMm2']>0 and a['rejectedHullBridge']['hullGapMm']>0
assert a['rectanglePreservingPrototype']['bridgeOverlapMm2']>0 and not a['rectanglePreservingPrototype']['accepted']
checks.append('negative hull bridge retained and positive local alternative not promoted')
assert len(c['boards'])==8 and c['afterA'] is None
for p,h in c['inputs'].items():assert sha(Path(p))==h
for p,h in read(OUT/'build.json')['sources'].items():assert sha(ROOT/p)==h
for p,v in c['nativeSources'].items():assert sha(ROOT/p)==v['baseline']==v['current']
checks.append('all eight frozen inputs and unchanged production geometry source hashes')
batch=list(map(json.loads,(OLD/'copper-partition/batch.jsonl').read_text().splitlines()))
fail=[r for r in batch if r.get('sample')==25]
assert len(fail)==2 and all('no unchecked fallback' in r['forwardM5']['Err'] for r in fail)
assert {(r['xyOrder'],r['zOrder']) for r in fail}=={(12,8),(24,16)}
checks.append('original two failed STM613 source-rule refinements preserved')
control=read(FIX.parent/'copper-partition/reused-coupon-control.json')
assert control['fullGate'] and control['farGate'] and control['rawReciprocityGate']
expected={'polyhedron':'1f9498a626e53e91a2d6de2882b799dd30612109fb2ab8996befe3854df81e5e',
          'prism':'22857c009475aa9b8884bab0cb8b5dede6d75c84a1a9112bd7ae284b20a8fddc',
          'near_far':'ee557489931d7d78e126a420fdac23105640d32fc586efbae9f282edec97c81c'}
for n,h in expected.items():assert sha(ROOT/f'native/analyze/src/extraction/local/{n}.rs')==h
checks.append('accepted magnetic kernel and independent full-operator control unchanged; no rerun')
task='docs/experimental/analyze/next-task-copper-geometry-and-pairs.md'
assert sha(ROOT/task)==sha(ROOT.parent/'eda-copilot-backend'/task)
for p in FIX.glob('*.py'):py_compile.compile(str(p),doraise=True)
(OUT/'verification.json').write_text(json.dumps({'checks':checks,'pass':True,'phaseAAccepted':False,'phaseBStarted':False,'kernelCalls':0},indent=2))
paths=[*FIX.glob('*'),*(p for p in OUT.iterdir() if p.is_file() and p.name not in ['manifest.json','probe.exe']),
       ROOT/task,ROOT/'docs/experimental/analyze/copper-geometry-and-pairs-verification.md',ROOT/'CHANGELOG.md',ROOT/'debugging/agent-handoff.md',
       FIX.parent/'copper-partition/reused-coupon-control.json',FIX.parent/'magnetic-near-far/verification.json',FIX.parent/'magnetic-stability/closed-reference.json',
       OLD/'copper-partition/batch.jsonl',OLD/'copper-partition/run.json',OLD/'copper-partition/geometry-overlays.png',OLD/'copper-partition/categories-cost.png',
       OLD/'magnetic-near-far/audit.png',ROOT/'native/analyze/Cargo.lock']
paths += [ROOT/f'native/analyze/src/extraction/local/{n}.rs' for n in expected]
paths += [ROOT/f'native/analyze/src/geometry/{n}.rs' for n in ['areas','primitives']]
paths += [Path(p) for p in c['inputs']]
hashes={str(p):sha(p) for p in paths if p.is_file()}
hashes.update(read(FIX/'regressions.json')['boardInputHashes'])
assert all(sha(Path(p))==h for p,h in hashes.items())
(OUT/'manifest.json').write_text(json.dumps({'hashes':hashes,'mismatchCount':0,'phaseAAccepted':False,'phaseBStarted':False},indent=2))
print(json.dumps({'savedChecks':len(checks),'hashes':len(hashes),'mismatches':0,'kernelCalls':0}))
