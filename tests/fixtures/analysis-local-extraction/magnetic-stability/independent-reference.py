"""Offline references: positive4D XY+exact double-z, box convolution/6D Gauss.
No production prism fan, polyhedral faces/edges, or native formulas imported.
"""
import os
os.environ['OPENBLAS_NUM_THREADS']='1';os.environ['OMP_NUM_THREADS']='1'
from pathlib import Path
import json,time,hashlib
import numpy as np
from scipy.stats import qmc
from numpy.polynomial.legendre import leggauss
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent;OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-stability';started=time.perf_counter();inp=json.loads((FIX/'coupon.json').read_text())
assert not (OUT/'reference.json').exists(),'do not repeat a saved independent reference'
def clock():assert time.perf_counter()-started<58,'reference internal58s guard'
def area(t):return abs(np.cross(t[1]-t[0],t[2]-t[0]))/2
def triangle(p,u):
 s=np.sqrt(u[:,0,None]);return p[0]+s*(p[1]-p[0])+s*u[:,1,None]*(p[2]-p[1])
def xy_reference(a,b,m,seed):
 clock();origin=np.array(a['triangleM'][0]);ta=np.array(a['triangleM'])-origin;tb=np.array(b['triangleM'])-origin
 assert a['zM']==b['zM'];h=a['zM'][1]-a['zM'][0]
 u=qmc.Sobol(4,scramble=True,seed=seed).random_base2(m);pa=triangle(ta,u[:,:2]);pb=triangle(tb,u[:,2:]);rho=np.linalg.norm(pa-pb,axis=1);assert min(rho)>0
 k=h/rho
 # ∫0h∫0h dz dz'/sqrt(rho²+(z-z')²), stable without h² cancellation.
 kernel=2*h*(np.arcsinh(k)-k/(np.hypot(1,k)+1))
 return float(area(ta)*area(tb)*np.mean(kernel))
pair=inp['stmPair'];stm=[]
for label,a,b in [('stm668-556',*pair),('stm668-self',pair[0],pair[0]),('stm556-self',pair[1],pair[1])]:
 levels=[]
 for m in [16,18,20]:
  values=[xy_reference(a,b,m,s) for s in [20261024,20261025,20261026,20261027]]
  levels.append({'sobolPower':m,'samplesPerScramble':2**m,'scrambleSeeds':[20261024,20261025,20261026,20261027],'valuesM5':values,'meanM5':float(np.mean(values)),'standardErrorM5':float(np.std(values,ddof=1)/2),'maxDeviationM5':float(max(abs(np.array(values)-np.mean(values))))})
 last=levels[-1];previous=levels[-2];uncertainty=2*abs(last['meanM5']-previous['meanM5'])+6*last['standardErrorM5']
 stm.append({'label':label,'levels':levels,'referenceM5':last['meanM5'],'empiricalAbsoluteUncertaintyM5':uncertainty,'empiricalRelativeUncertainty':uncertainty/last['meanM5'],'meaning':'Independent exact-double-z4D randomized Sobol;2×last change+6SE working uncertainty, not certified continuum bound.'})
bars=inp['bars'];selfs=[]
for b in bars:
 d=np.array([b['lengthM'],b['widthM'],b['zM'][1]-b['zM'][0]]);volume=np.prod(d);levels=[]
 for n in [64,128,256]:
  clock();x,w=leggauss(n);x=(x+1)/2;w=w/2;s=x[:,None];t=x[None,:];weights=w[:,None]*w[None,:];radial=1/6-(s+t)/12+s*t/20;total=0.
  for k in range(3):
   j,l=[i for i in range(3) if i!=k];total+=np.sum(weights*radial/np.sqrt(d[k]**2+(d[j]*s)**2+(d[l]*t)**2))
  value=float(8*volume**2*total*1e-7/(d[1]*d[2])**2);levels.append({'order':n,'LH':value})
 selfs.append(levels)
def samples(b,n):
 x,w=leggauss(n);x=(x+1)/2;w=w/2;xx,yy,zz=np.meshgrid(x,x,x,indexing='ij');wx,wy,wz=np.meshgrid(w,w,w,indexing='ij')
 return np.column_stack([(b['x0M']+xx*b['lengthM']).ravel(),(b['y0M']+yy*b['widthM']).ravel(),(b['zM'][0]+zz*(b['zM'][1]-b['zM'][0])).ravel()]),(wx*wy*wz).ravel()
mutual=[];matrices=[]
for n in [4,8,12,16]:
 samples_all=[samples(b,n) for b in bars];matrix=np.diag([s[-1]['LH'] for s in selfs])
 for i in range(4):
  for j in range(i):
   clock();pa,wa=samples_all[i];pb,wb=samples_all[j];total=0.
   for start in range(0,len(pa),32):
    distance=np.linalg.norm(pa[start:start+32,None,:]-pb[None,:,:],axis=2);total+=np.sum(wa[start:start+32,None]*wb[None,:]/distance)
   matrix[i,j]=matrix[j,i]=1e-7*bars[i]['lengthM']*bars[j]['lengthM']*total
 matrices.append({'mutualOrder':n,'rawLH':matrix.tolist()})
result={'independentMethods':['STM: exactdouble-z positive4D randomized integration','boxself: difference-convolution cube-Duffy radial polynomial exact','boxmutual: direct smooth6D product Gauss'],'stm':stm,'selfConvolutionLevels':selfs,'operatorLevels':matrices,'seconds':time.perf_counter()-started,'scriptSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'inputSha256':hashlib.sha256((FIX/'coupon.json').read_bytes()).hexdigest(),'sourceScope':'No production imports; current densities and finite geometry match native coupon.'}
(OUT/'reference.json').write_text(json.dumps(result,indent=2));(FIX/'reference.json').write_text(json.dumps(result,indent=2));print(json.dumps({'seconds':result['seconds'],'stmWorkingRelativeUncertainties':{x['label']:x['empiricalRelativeUncertainty'] for x in stm}}))
