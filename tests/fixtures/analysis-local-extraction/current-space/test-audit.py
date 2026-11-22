"""Saved-value audit negatives; no native/reference solver replays."""
from pathlib import Path
import copy,json,subprocess,tempfile,sys
ROOT=Path(__file__).resolve().parents[4];source=json.loads((ROOT/'debugging/p7-completion/coupon.json').read_text());audit=Path(__file__).with_name('audit.py')
def matrix(v):v['runs'][0]['model']['capacitanceF'][0][2]*=2
def loss(v):next(s for s in v['runs'][0]['sweep'] if s['nativePointAdmitted'])['cellLossesWPeakNorm'][0]*=2
def missing(v):v['runs'][0]['sweep'].pop()
def fake(v):v['runs'][0]['frequency']={'Ok':{}}
def invented(v):next(s for s in v['runs'][0]['sweep'] if not s['nativePointAdmitted'])['diagnostic']=''
with tempfile.TemporaryDirectory(prefix='p7-audit-') as d:
 d=Path(d)
 for mutation in [matrix,loss,missing,fake,invented]:
  v=copy.deepcopy(source);mutation(v);p=d/(mutation.__name__+'.json');p.write_text(json.dumps(v))
  run=subprocess.run([sys.executable,str(audit),str(p),str(d/mutation.__name__)],cwd=ROOT,capture_output=True,timeout=30)
  assert run.returncode!=0,mutation.__name__
print('5 saved-value audit corruption cases refused; no native replay')
