from pathlib import Path
import hashlib,json
from trace import ROOT,FIX,OUT,OLD,NAMES,read,sha
files=[p for directory in [FIX,OUT] for p in directory.rglob('*') if p.is_file() and p.name!='manifest.json' and '__pycache__' not in p.parts]
for name in NAMES:
 for filename in [name+'.json','source-pcb.txt','source.epro2','fixture.json','export-evidence.json']:
  files.append(ROOT/f'tests/pcb-analyze/{name}'/filename)
for p in read(OUT/'input-hashes.json'):files.append(Path(p))
files += [ROOT/p for p in read(OUT/'build.json')['sources']]
files += [ROOT/'docs/experimental/analyze/next-task-copper-partition-audit.md',ROOT/'docs/experimental/analyze/copper-partition-audit.md',ROOT/'CHANGELOG.md',ROOT/'debugging/agent-handoff.md']
for name in ['coupon.json','reference.json','verification.json']:files.append(FIX.parent/'magnetic-near-far'/name)
files += [OLD/'magnetic-near-far/audit.json',OLD/'magnetic-near-far/plan-run/stdout.json',next((OLD/'magnetic-stability').glob('current-run-*/stdout.json')),OLD/'independent-current-counts.json']
hashes={str(p.resolve()):sha(p) for p in sorted(set(files))}
native_match=all(sha(ROOT/p)==h for p,h in read(OUT/'build.json')['sources'].items())
input_match=all(sha(Path(p))==h for p,h in read(OUT/'input-hashes.json').items())
data={'files':hashes,'nativeSourceMatchesBuild':native_match,'originalInputsUnchanged':input_match,'newKernelBatches':1,'noProductionMeshKernelChanges':True,'notes':['Raw batch/refusals preserved; no retries.','One Python mirror mismatch retained; actual native partition used.','Sparse shape strata stay unpriced.','Saved-value control is not a new continuum certificate.']}
assert native_match and input_match
(OUT/'manifest.json').write_text(json.dumps(data,indent=2))
mismatches=[p for p,h in hashes.items() if sha(Path(p))!=h];assert not mismatches
print(json.dumps({'hashes':len(hashes),'mismatches':mismatches,'nativeSourceMatchesBuild':native_match,'originalInputsUnchanged':input_match}))
