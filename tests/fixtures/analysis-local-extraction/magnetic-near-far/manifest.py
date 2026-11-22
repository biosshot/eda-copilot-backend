"""Hash complete packet plus reused frozen sources; verify immediately."""
from pathlib import Path
import hashlib,json
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent;OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-near-far'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
files=[p for folder in [FIX,OUT] for p in folder.rglob('*') if p.is_file() and p.name!='manifest.json' and '__pycache__' not in p.parts]
files += [ROOT/'native/analyze/src/extraction/local'/n for n in ['mod.rs','near_far.rs','far.rs','polyhedron.rs','prism.rs','capacitance.rs','layered_capacitance.rs']]
files += [ROOT/'native/analyze/tests/magnetic_near_far.rs',ROOT/'docs/experimental/analyze/magnetic-near-far-verification.md',ROOT/'docs/experimental/analyze/next-task-magnetic-near-far.md',ROOT/'CHANGELOG.md',ROOT/'debugging/agent-handoff.md']
files += [FIX.parent/'magnetic-stability'/n for n in ['closed-coupon.json','closed-reference.json','closed-verification.json']]
for n in ['BandAmpV5','STM32G0_CAN_RS485','TPS63060DSCR']:files.append(ROOT/f'debugging/p7-design-checkpoint/independent-current/{n}-supports.json')
data={'files':{p.relative_to(ROOT).as_posix():sha(p) for p in sorted(set(files))},'nativeSourceMatchesBuild':all(sha(ROOT/p)==h for p,h in json.loads((OUT/'build.json').read_text())['sources'].items()),'notes':['All raw matrices/actions/bounds retained.','No overwritten old independent reference or failed coarse policy.','No commit, full ROI solve or public integration.']}
assert data['nativeSourceMatchesBuild'];(OUT/'manifest.json').write_text(json.dumps(data,indent=2))
mismatches=[p for p,h in data['files'].items() if sha(ROOT/p)!=h];assert not mismatches
print(json.dumps({'hashes':len(data['files']),'mismatches':mismatches,'nativeSourceMatchesBuild':data['nativeSourceMatchesBuild']}))
