"""Independent descriptor reference, reusing immutable analytic/box coefficients.
No production solver import. A bordered uniform gauge is explicit at every f.
"""
from pathlib import Path
import json, numpy as np, hashlib, sys
import mpmath as mp
mp.mp.dps=80
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p6-p7-structural-gauges'
input_path=Path(sys.argv[1]) if len(sys.argv)>1 else OUT/'coupon.json'
if len(sys.argv)>2:OUT=Path(sys.argv[2]);OUT.mkdir(parents=True,exist_ok=True)
saved=json.loads(input_path.read_text());history=json.loads((ROOT/'debugging/p7-completion/parent-coupon.json').read_text())
reference_path=ROOT/'debugging/p7-completion/parent-audit/references.json'
frozen=json.loads(reference_path.read_text());C=np.array(frozen['capacitanceF']);L=np.array(frozen['inductanceH']);R=np.eye(2)/(58e6*35e-6)
A=np.array([[1.,0.],[-1.,0.],[0.,1.],[0.,-1.]])
longB=A.copy();crossB=np.array([[1.,0.],[0.,1.],[-1.,0.],[0.,-1.]])
def independent(f,B):
 # SI descriptor coefficients span >1e14 in magnitude. Plain binary64 LU
 # contaminates a physical island-charge coordinate although port differences
 # still look accurate. Use independently assembled 80-digit arithmetic, with
 # explicit gauge and no added physical constraints or altered excitation.
 w=2*mp.pi*mp.mpf(f);D=mp.matrix(7,7)
 for i in range(4):
  for j in range(4):D[i,j]=mp.j*w*mp.mpf(float(C[i,j]))
  for j in range(2):D[i,4+j]=mp.mpf(float(A[i,j]));D[4+j,i]=-mp.mpf(float(A[i,j]))
  D[i,6]=1;D[6,i]=1
 for i in range(2):
  for j in range(2):D[4+i,4+j]=mp.mpf(float(R[i,j]))+mp.j*w*mp.mpf(float(L[i,j]))
 X=np.zeros((6,2),complex);Z=np.zeros((2,2),complex)
 for column in range(2):
  rhs=mp.matrix(7,1)
  for i in range(4):rhs[i]=mp.mpf(float(B[i,column]))
  state=mp.lu_solve(D,rhs)
  for i in range(6):X[i,column]=complex(state[i])
  for row in range(2):Z[row,column]=complex(sum(mp.mpf(float(B[i,row]))*state[i] for i in range(4)))
 return X,Z
def numbers(response):
 assert 'Ok' in response,response
 return np.array([[[complex(z['real'],z['imaginary']) for z in row] for row in sample] for sample in response['Ok']['parameters']])
def scaled(actual,expected):
 return float(np.max(np.abs(actual-expected)/np.maximum(np.abs(expected),1e-12*np.max(np.abs(expected)))))
audits=[]
for run in saved['runs']:
 assert run['input']==history['runs'][0]['input'] and run['foils']==history['runs'][0]['foils']
 for key in ['incidence','resistanceOhm','inductanceH','capacitanceF','portIncidence']:
  assert run['model'][key]==history['runs'][0]['model'][key],key
 assert run['trace']==history['runs'][0]['trace']
 assert run['model']['structuralGaugeModes']==[[1.,1.,1.,1.]]
 assert run['reducedModel']['structuralGaugeModes']==run['model']['structuralGaugeModes']
 assert run['reducedModel']['capacitanceF']==run['model']['capacitanceF']
 Q=np.array(run['currentProjection']).T
 assert scaled(np.array(run['dcState'][4:]),Q@np.array([1.,-1.]))<1e-9
 assert abs(sum(run['dcCellLossesW'])-2*R[0,0])<1e-10*2*R[0,0]
 expected_density=np.array([[1/(.0005*.000035),0.]]*2+[[-1/(.0005*.000035),0.]]*2)
 assert scaled(np.array(run['dcDensityAPerM2']),expected_density)<1e-9
 long=numbers(run['longitudinal']);rlong=numbers(run['reducedLongitudinal']);cross=numbers(run['crossFoil']);loop=numbers(run['frequency']);rloop=numbers(run['reducedFrequency'])
 assert len(run['sweep'])==17
 rows=[];refs=[]
 for k,s in enumerate(run['sweep']):
  f=s['frequencyHz'];assert f==history['runs'][0]['sweep'][k]['frequencyHz'];assert s['nativePointAdmitted'],s
  X,Z=independent(f,longB);_,CZ=independent(f,crossB)
  drive=np.array([1.,-1.]);physical=X@drive;zloop=drive@Z@drive
  ni=np.array(s['currentsRealA'])+1j*np.array(s['currentsImagA']);ri=np.array(s['recoveredRealA'])+1j*np.array(s['recoveredImagA']);expected_i=Q@physical[4:]
  nv=np.array(s['potentialsRealV'])+1j*np.array(s['potentialsImagV']);rv=np.array(s['reducedPotentialsRealV'])+1j*np.array(s['reducedPotentialsImagV'])
  loss=float(np.real(np.conj(physical[4:])@R@physical[4:]));local=np.array(s['cellLossesWPeakNorm']);expected_local=np.full(4,loss/4)
  reduced_local=np.array([R[0,0]*np.sum(np.abs(ri[2*cell:2*cell+2])**2) for cell in range(4)])
  errors={'physicalPotential':scaled(nv,physical[:4]),'recoveredPotential':scaled(rv,physical[:4]),'fullProjectedPotential':scaled(nv,rv),'longitudinalZ':scaled(long[k],Z),'reducedLongitudinalZ':scaled(rlong[k],Z),'crossFoilZ':scaled(cross[k],CZ),'loopZ':float(abs(complex(*s['loopZOhm'])-zloop)/abs(zloop)),'reducedLoopZ':float(abs(complex(*s['reducedLoopZOhm'])-zloop)/abs(zloop)),'physicalCurrent':scaled(ni,expected_i),'recoveredCurrent':scaled(ri,expected_i),'reducedLocalLoss':scaled(reduced_local,expected_local),'fullProjectedLocalLoss':scaled(local,reduced_local),'localLoss':scaled(local,expected_local),'totalLoss':abs(s['jouleLossWPeakNorm']-loss)/loss,'reducedLoss':abs(s['reducedJouleLossWPeakNorm']-loss)/loss,'fullProjectedCurrent':scaled(ni,ri)}
  # Complex full-response norm avoids meaningless component relative errors
  # at exact or weak real/imaginary zeros. Every physical quantity stays present.
  assert max(errors.values())<=.01,(f,errors)
  assert abs(sum(local)-s['jouleLossWPeakNorm'])<=1e-10*loss
  assert abs(loop[k,0,0]-complex(*s['loopZOhm']))<=1e-10*abs(zloop)
  assert abs(rloop[k,0,0]-complex(*s['reducedLoopZOhm']))<=1e-10*abs(zloop)
  rows.append({'frequencyHz':f,'full':'PASS','reduced':'PASS',**errors})
  refs.append({'frequencyHz':f,'loopZOhm':[zloop.real,zloop.imag],'lossW':loss,'currentRealA':expected_i.real.tolist(),'currentImagA':expected_i.imag.tolist()})
 audits.append({'name':run['name'],'rows':rows,'maxErrors':{key:max(row[key] for row in rows) for key in errors},'references':refs})
for control in saved['exteriorControls']:
 ext=control['exteriorCapacitanceF'];assert abs(control['energyJ']-9*ext)<=1e-20+1e-4*9*ext
 assert np.max(abs(np.array(control['dcState'][:2])-3))<1e-8
 assert 'incompatible with C' in control['falseCertificate']
 response=numbers(control['frequency'])
 axis=np.array([s['frequencyHz'] for s in saved['runs'][0]['sweep']]);expected=1/(.01+2j*np.pi*axis*(1e-6+ext/2))
 assert scaled(response[:,0,0],expected)<1e-9
result={'auditSourceSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'inputSha256':hashlib.sha256(input_path.read_bytes()).hexdigest(),'referenceArithmetic':'mpmath 80 decimal digits; exact imported binary64 coefficients, high-precision omega and explicit gauge','status':'17/17-full-reduced-PASS','referenceSource':str(reference_path.relative_to(ROOT)),'referenceSha256':hashlib.sha256(reference_path.read_bytes()).hexdigest(),'methods':frozen['methods'],'normalization':'per-entry complex error / max(entry reference magnitude, 1e-12 complete-observable reference scale); unchanged 1% gate','runs':audits}
(OUT/'audit.json').write_text(json.dumps(result,indent=2));print(json.dumps({'status':result['status'],'maxErrors':[r['maxErrors'] for r in audits]}))
