"""Independent box / tensor / complex circuit oracle, never native imports."""
from pathlib import Path
import json,hashlib,sys
import numpy as np
from numpy.polynomial.legendre import leggauss
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-completion'
input_path=Path(sys.argv[1]) if len(sys.argv)>1 else OUT/'coupon.json'
if len(sys.argv)>2: OUT=Path(sys.argv[2]);OUT.mkdir(parents=True,exist_ok=True)
saved=json.loads(input_path.read_text());assert saved['status']=='partial' and saved['acBandAdmission']=='rejected'
eps=8.8541878128e-12;l=w=.0005;t=.000035;h=.001;r=1/(58e6*t)
ls=1.4198058291191782e-10 # unchanged saved independent box self 512-order
levels=[]
for n in [16,32]:
 q,v=leggauss(n);q=(q+1)/2;v=v/2
 # x/y differences are even; z includes both signed half intervals.
 x=q[:,None,None]*l;y=q[None,:,None]*w;z=q[None,None,:]*t
 weight=v[:,None,None]*v[None,:,None]*v[None,None,:]*(l-x)*(w-y)*(t-z)*l*w*t
 integral=4*np.sum(weight*(1/np.sqrt(x*x+y*y+(h+t+z)**2)+1/np.sqrt(x*x+y*y+(h+t-z)**2)))
 lm=float(1e-7*integral/(w*t)**2);levels.append({'order':n,'mutualH':lm})
change=abs(levels[-1]['mutualH']-levels[0]['mutualH'])/lm;assert change<=1e-6
mass=l*w/6*np.array([[2.,1.],[1.,2.]])
stiff=w/l*np.array([[1.,-1.],[-1.,1.]])
C=eps*np.block([[h/3*stiff+mass/h,h/6*stiff-mass/h],[h/6*stiff-mass/h,h/3*stiff+mass/h]])
R=np.eye(2)*r;L=np.array([[ls,lm],[lm,ls]])
A=np.array([[1.,0.],[-1.,0.],[0.,1.],[0.,-1.]])
B=np.array([[1.,0.],[-1.,0.],[0.,1.],[0.,-1.]])
refs=[];audits=[]
assert len(saved['runs'])==2
for run in saved['runs']:
 assert 'Err' in run['frequency'] and 'COUPLED_POWER' in run['frequency']['Err']
 assert len(run['sweep'])==17
 axis=[s['frequencyHz'] for s in run['sweep']]
 assert np.allclose(axis,1000*10**(np.arange(17)/8),rtol=1e-14,atol=0)
 assert any(not s['nativePointAdmitted'] for s in run['sweep'])
 d=run['model'];nativeC=np.zeros((4,4))
 for i,j,v in d['capacitanceF']:nativeC[i,j]=v
 cerror=float(np.max(np.abs(nativeC-C))/np.max(np.diag(C)));assert cerror<=1e-10
 Q=np.array(run['currentProjection']).T;expectedQ=np.zeros((8,2));expectedQ[[0,2],0]=1/np.sqrt(2);expectedQ[[4,6],1]=1/np.sqrt(2);assert np.max(np.abs(Q-expectedQ))<=1e-15
 nativeL=np.zeros((8,8));nativeR=np.zeros((8,8))
 for i,j,v in d['inductanceH']:nativeL[i,j]=v
 for i,j,v in d['resistanceOhm']:nativeR[i,j]=v
 le=float(np.max(np.abs(Q.T@nativeL@Q-L))/ls);assert le<=.002
 re=float(np.max(np.abs(Q.T@nativeR@Q-R))/r);assert re<=1e-10
 rows=[]
 for k,s in enumerate(run['sweep']):
  f=s['frequencyHz'];omega=2*np.pi*f
  G=np.block([[1j*omega*C,A],[-A.T,R+1j*omega*L]])
  gauge=np.r_[np.ones(4),np.zeros(2)];D=np.zeros((7,7),complex);D[:6,:6]=G;D[:6,6]=gauge;D[6,:6]=gauge
  rhs=np.zeros((7,2),complex);rhs[:4]=B
  X=np.linalg.solve(D,rhs)[:6];Z=B.T@X[:4]
  physical=X@np.array([1.,-1.]);zloop=np.array([1.,-1.])@Z@np.array([1.,-1.]);loss=float(np.real(np.conj(physical[4:])@R@physical[4:]))
  if run is saved['runs'][0]:refs.append({'frequencyHz':f,'ZRealOhm':Z.real.tolist(),'ZImagOhm':Z.imag.tolist(),'loopZOhm':[float(zloop.real),float(zloop.imag)],'uniformCurrentRealA':physical[4:].real.tolist(),'uniformCurrentImagA':physical[4:].imag.tolist(),'lossWPeakNorm':loss})
  if not s['nativePointAdmitted']:
   assert any(code in s['diagnostic'] for code in ['COUPLED_POWER','NETWORK_RESIDUAL','NETWORK_SINGULAR','COUPLED_KCL'])
   rows.append({'frequencyHz':f,'status':'rejected','diagnostic':s['diagnostic']})
   continue
  native=complex(*s['loopZOhm'])
  expected_loop=np.array([1.,-1.])@Z@np.array([1.,-1.]);ze=float(abs(native-expected_loop)/abs(expected_loop));assert ze<=.01
  physical=X@np.array([1.,-1.]);zloop=np.array([1.,-1.])@Z@np.array([1.,-1.])
  nz=complex(*s['loopZOhm']);loop_error=abs(nz-zloop)/abs(zloop);assert loop_error<=.01
  ni=np.array(s['currentsRealA'])+1j*np.array(s['currentsImagA']);expected_i=Q@physical[4:]
  ie=float(np.max(np.abs(ni-expected_i))/np.max(np.abs(expected_i)));assert ie<=.01
  loss=float(np.real(np.conj(physical[4:])@R@physical[4:]));pe=abs(s['jouleLossWPeakNorm']-loss)/loss;assert pe<=.01
  local=np.array(s['cellLossesWPeakNorm']);assert abs(np.sum(local)-s['jouleLossWPeakNorm'])<=1e-10*loss
  expected_loss=np.full(4,loss/4);local_e=float(np.max(np.abs(local-expected_loss))/np.max(expected_loss));assert local_e<=.01
  rows.append({'frequencyHz':f,'status':'diagnostic-point-only','independentPortZError':ze,'independentLoopZError':loop_error,'independentPhysicalCurrentError':ie,'independentLossError':pe,'independentLocalLossError':local_e,'projectedPhysicalCurrentError':s['currentNormalizedError'],'projectedLossError':abs(s['reducedJouleLossWPeakNorm']-s['jouleLossWPeakNorm'])/s['jouleLossWPeakNorm']})
 audits.append({'name':run['name'],'CScaledError':cerror,'LScaledError':le,'RScaledError':re,'rows':rows,'rejectedFrequenciesHz':[s['frequencyHz'] for s in rows if s['status']=='rejected'],'worstAcceptedPoint':{key:max(row[key] for row in rows if key in row) for key in next(row for row in rows if row['status']!='rejected') if key not in ['frequencyHz','status']}})
reference={'methods':['frozen independent box self convolution','smooth vertical-box difference convolution','analytic rectangle P1 mass/stiffness z tensor','handwritten6-state complex descriptor with one common gauge'],'mutualLevels':levels,'mutualLastChange':change,'capacitanceF':C.tolist(),'inductanceH':L.tolist(),'samples':refs}
(OUT/'references.json').write_text(json.dumps(reference,indent=2))
roi=json.loads((ROOT/'tests/fixtures/analysis-local-extraction/copper-context/verification.json').read_text())['meshCounts']
refusals=[{'board':v['name'],'foilSupports':v['foil'],'axialSupports':v['axial'],'candidateCurrentDofs':v['candidateCurrentCoefficients'],'potentialNodes':v['potentialNodes'],'status':'not-admitted','reasons':['saved GEOS triangulation is counts-only, not a validated current/contact space','all-foil current assembler64-support64-potential cap exceeded','ordinary barrel potential/current traces and foil/barrel junction absent','spatial trace field accuracy lacks independent ROI reference','no board extraction DSL operation or P6 reduction contract for arbitrary trace coordinates']} for v in roi]
audit={'status':'partial-refusal-preserved','scope':'spatial-C/foil-DC/energy-projection pass; AC band rejected; no PCB/via/full P7 acceptance','sourceSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'runs':audits,'realRoiRefusals':refusals,'originalCrossFoilPowerRefusal':'failed-cross-foil-ports.json'}
(OUT/'audit.json').write_text(json.dumps(audit,indent=2));print(json.dumps({'status':audit['status'],'worstAcceptedPoint':[v['worstAcceptedPoint'] for v in audits], 'rejected':[v['rejectedFrequenciesHz'] for v in audits]}))
