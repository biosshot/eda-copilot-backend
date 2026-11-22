"""Independent bounded reduction audit. No production solver or reference rerun."""
from pathlib import Path
import hashlib,json,sys,numpy as np,mpmath as mp
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-final-integration'
if len(sys.argv)>2:OUT=Path(sys.argv[2]).resolve();OUT.mkdir(parents=True,exist_ok=True)
mp.mp.dps=80
packet=Path(sys.argv[1]) if len(sys.argv)>1 else OUT/'automatic.json';saved=json.loads(packet.read_text())
assert saved['p7Complete'] is False and saved['pcbExtractionAdmitted'] is False and saved['viaAdmitted'] is False
old=json.loads((ROOT/'debugging/p6-p7-structural-gauges/coupon.json').read_text())
ref_path=ROOT/'debugging/p7-completion/parent-audit/references.json'
frozen=json.loads(ref_path.read_text());C=np.array(frozen['capacitanceF']);L=np.array(frozen['inductanceH'])
R=np.eye(2)/(58e6*35e-6);A=np.array([[1.,0.],[-1.,0.],[0.,1.],[0.,-1.]])
W=np.zeros((8,2));W[[0,2],0]=1/np.sqrt(2);W[[4,6],1]=1/np.sqrt(2)
def independent(f):
 w=2*mp.pi*mp.mpf(f);D=mp.matrix(7,7);rhs=mp.matrix(7,1)
 for i in range(4):
  for j in range(4):D[i,j]=mp.j*w*mp.mpf(float(C[i,j]))
  for j in range(2):D[i,4+j]=mp.mpf(float(A[i,j]));D[4+j,i]=-mp.mpf(float(A[i,j]))
  D[i,6]=1;D[6,i]=1;rhs[i]=float(A[i,0]-A[i,1])
 for i in range(2):
  for j in range(2):D[4+i,4+j]=mp.mpf(float(R[i,j]))+mp.j*w*mp.mpf(float(L[i,j]))
 x=np.array([complex(v) for v in mp.lu_solve(D,rhs)])
 return np.r_[x[:4],W@x[4:6]]
def error(actual,ref):
 d=np.maximum(abs(ref),1e-12*np.max(abs(ref)))
 return float(np.max(np.divide(abs(actual-ref),d,out=np.zeros_like(d),where=d!=0))) if np.all((d!=0)|(actual==ref)) else float('inf')
audits=[]
for run in saved['runs']:
 assert run['diagnostic'] is None,run['diagnostic']
 for key in ['input','trace','foils','model']:
  assert run[key]==old['runs'][0][key],key
 model=run['model'];small=run['reducedModel'];Q=np.array(run['basis']).T
 assert np.max(abs(Q.T@Q-np.eye(Q.shape[1])))<1e-10
 for key in ['potentialNodes','capacitanceF','conservedPotentialModes','structuralGaugeModes','portTerminals','portIncidence']:
  assert model[key]==small[key],key
 axis=[r['frequencyHz'] for r in run['rows'] if r['drive']=='opposing']
 assert len(run['rows'])==51
 assert [(r['frequencyHz'],r['drive']) for r in run['rows']]==[(f,d) for f in axis for d in ['port0','port1','opposing']]
 assert axis==[r['frequencyHz'] for r in old['runs'][0]['sweep']]
 assert run['audit']['training_hz']==axis[::2]
 assert set(axis[1::2]).isdisjoint(run['audit']['training_hz'])
 rows=[]
 for row in run['rows']:
  a=np.array(row['fullState']);a=a[:12]+1j*a[12:]
  b=np.array(row['recoveredState']);b=b[:12]+1j*b[12:]
  current=error(b[4:],a[4:]);potential=error(b[:4],a[:4])
  af=abs(a[4:])**2/(58e6*35e-6);bf=abs(b[4:])**2/(58e6*35e-6)
  loss=error(bf,af)
  port=error(np.array([b[0]-b[1],b[2]-b[3]]),np.array([a[0]-a[1],a[2]-a[3]]))
  assert max(current,potential,loss,port)<=1e-6,(row,current,potential,loss,port)
  independent_error=None
  if row['drive']=='opposing':
   ref=independent(row['frequencyHz'])
   independent_error=max(error(a[:4],ref[:4]),error(a[4:],ref[4:]),error(b[:4],ref[:4]),error(b[4:],ref[4:]))
   assert independent_error<=.01,(row,independent_error)
  rows.append({'frequencyHz':row['frequencyHz'],'drive':row['drive'],'current':current,'potential':potential,'port':port,'localLoss':loss,'independentOpposing':independent_error})
 audits.append({'name':run['name'],'fullCurrentOrder':8,'reducedCurrentOrder':Q.shape[1],
  'maxErrors':{k:max(r[k] for r in rows if r[k] is not None) for k in ['current','potential','port','localLoss','independentOpposing']},'rows':rows})
result={'status':'bounded-automatic-current-PASS','p7Complete':False,'referenceArithmetic':'independent 80-digit descriptor, frozen analytic tensor/box coefficients; no expensive reference replay',
 'referenceScope':'Independent physical reference for opposing drive; synthetic Rust parallel-RLC test covers all its unit-drive currents independently. Foil unit-drive continuum accuracy not inferred from full/reduced agreement.',
 'packetSha256':hashlib.sha256(packet.read_bytes()).hexdigest(),'referenceSha256':hashlib.sha256(ref_path.read_bytes()).hexdigest(),
 'auditSourceSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'runs':audits}
(OUT/'audit.json').write_text(json.dumps(result,indent=2));print(json.dumps({'status':result['status'],'runs':[{k:v for k,v in r.items() if k!='rows'} for r in audits]}))
