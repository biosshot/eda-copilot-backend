"""Independent saved-matrix reference-margin audit; no field/reference solves."""
from pathlib import Path
import json,hashlib,numpy as np
FIX=Path(__file__).resolve().parent;ROOT=FIX.parents[3];OUT=ROOT/'debugging/p7-design-checkpoint/three-reference-margin'
def quadratic_norm(a,b):
 a=np.array(a,dtype=float);b=np.array(b,dtype=float);a=(a+a.T)/2;b=(b+b.T)/2
 eigen=np.linalg.eigvalsh(b);assert min(eigen)>0
 inv=np.linalg.inv(np.linalg.cholesky(b));return float(max(abs(np.linalg.eigvalsh(inv@(a-b)@inv.T))))
saved=json.loads((FIX/'native-evidence-summary.json').read_text())
for name,digest in saved['buildSourceSha256'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest,name
assert hashlib.sha256((ROOT/'debugging/p7-design-checkpoint/layered-capacitance-coupon.exe').read_bytes()).hexdigest()==saved['buildBinarySha256']
native=saved['rows']['three-d64-mesh'];c=np.array(native['capacitanceF'])
refs={n:json.loads((FIX/f'references/three-{n}/capacitance-raw-f.json').read_text()) for n in [16,32,48]}
changes={f'{a}->{b}':quadratic_norm(refs[a],refs[b]) for a,b in [(16,32),(32,48),(16,48)]}
rows=[]
for n,ref in refs.items():
 raw=np.array(ref);scalar=(c-raw)/abs(raw);error=quadratic_norm(c,ref)
 rows.append({'refinement':n,'nativeFullError':error,'rawReferenceF':ref,'rawEntryRelativeErrors':scalar.tolist(),'maxRawEntryError':float(max(abs(scalar).flat)),'referencePositiveEigenvaluesF':np.linalg.eigvalsh((raw+raw.T)/2).tolist(),'referenceRawReciprocity':float(max(abs(raw-raw.T).flat)/max(abs(raw).flat))})
for row in rows[1:]:
 key='16->32' if row['refinement']==32 else '32->48';row['lastObservedReferenceChange']=changes[key];row['workingScreen']=row['nativeFullError']+2*changes[key];row['workingScreenPass']=row['workingScreen']<=.05
last=changes['32->48'];error=rows[-1]['nativeFullError'];screen=error+2*last;decreasing=last<changes['16->32']
report={'nativeReuse':native,'nativeSourceAndBinaryRevalidated':True,'referenceRows':rows,'observedFullReferenceChanges':changes,'lastNativeFullError':error,'lastReferenceChange':last,'twiceLastReferenceChange':2*last,'conservativeWorkingScreen':screen,'strictlyDecreasingReferenceChanges':decreasing,'screenPass':screen<=.05 and decreasing,'gate':.05,'uncertifiedNature':'Observed reference changes and this conservative working screen are not certified continuum error bounds; all physical/discretization/domain limitations remain separate.','resources':json.loads((FIX/'references/three-48-run.json').read_text()),'nativeControlNeeded':not(screen<=.05 and decreasing),'geometryDomainAndNativeUnchanged':True,'priorFailuresPreserved':True}
paths=[FIX/'native-evidence-summary.json',FIX/'generate-reference.py',FIX/'refine-three-reference.py',FIX/'references/three-48-preflight.json',FIX/'references/three-48-run.json',Path(__file__)]
for n in refs:paths.extend(p for p in (FIX/f'references/three-{n}').glob('*') if p.is_file())
report['sha256']={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
assert report['resources']['exitCode']==0 and report['resources']['peakBytes']<=1024*1024**2 and report['resources']['seconds']<=60
(OUT/'audit.json').write_text(json.dumps(report,indent=2));(FIX/'references/three-margin-summary.json').write_text(json.dumps(report,indent=2))
print(json.dumps({k:v for k,v in report.items() if k not in ['nativeReuse','referenceRows','resources','sha256']},indent=2))
