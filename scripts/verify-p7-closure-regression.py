"""Compare the exact previously positive coupled request after native rebuild."""
import json,pathlib,sys,hashlib
root=pathlib.Path(__file__).resolve().parents[1]
folder=pathlib.Path(sys.argv[1])
oldpath=root/'debugging/p7-closure/stripline-native.json';newpath=folder/'stripline-native-replay.json'
old=json.loads(oldpath.read_text())['result'];new=json.loads(newpath.read_text())['result']
def numbers(x):
 if isinstance(x,(int,float)) and not isinstance(x,bool):yield float(x)
 elif isinstance(x,list):
  for v in x:yield from numbers(v)
 elif isinstance(x,dict):
  for k in sorted(x):yield from numbers(x[k])
groups={}
for key in ['parameters','network','frequenciesHz']:
 a=list(numbers(old[key]));b=list(numbers(new[key]))
 if any(not __import__('math').isfinite(v) for v in a+b):raise RuntimeError('Nonfinite '+key)
 if len(a)!=len(b):raise RuntimeError('Shape mismatch '+key)
 groups[key]=dict(exactJsonEqual=old[key]==new[key],numericCount=len(a),maxAbsoluteDifference=max([abs(x-y) for x,y in zip(a,b)],default=0),maxRelativeDifference=max([abs(x-y)/max(abs(x),1e-30) for x,y in zip(a,b)],default=0))
e=dict(status='pass' if all(g['exactJsonEqual'] for g in groups.values()) else 'fail',groups=groups,oldAddonSha256='c348e48a125249939327f5f1c26dd28f7d30e27edb5d24ec4f9cd210dc6effa2',newProvenance=json.loads(newpath.read_text()).get('provenance'),originalPacketSha256=hashlib.sha256(oldpath.read_bytes()).hexdigest(),newPacketSha256=hashlib.sha256(newpath.read_bytes()).hexdigest(),scope='Exact unchanged coupled-stripline direct DSL versus historical native request; complete C/L/R/G/C0/modal parameters and full four-port complex S on three frequencies; regression only, not independent accuracy')
(folder/'stripline-regression.json').write_text(json.dumps(e,indent=2));print(json.dumps(e));sys.exit(0 if e['status']=='pass' else 1)
