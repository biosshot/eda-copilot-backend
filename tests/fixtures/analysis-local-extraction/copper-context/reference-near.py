"""Reused independent positive4D/exact-double-z method, new fixed supports only.

This is inside the ONE G1 batch; never repeats the accepted full matrix solve.
"""
from pathlib import Path
import json,time,os
os.environ['OPENBLAS_NUM_THREADS']='1'
import numpy as np
from scipy.stats import qmc
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint/copper-context'
assert not (OUT/'postfix-reference.json').exists()
native=json.loads((OUT/'postfix-self.json').read_text());rows=[];start=time.perf_counter()
def triangle(p,u):
    s=np.sqrt(u[:,0,None]);return p[0]+s*(p[1]-p[0])+s*u[:,1,None]*(p[2]-p[1])
for cell in native['cells']:
    t=np.array(cell['triangleM']);t-=t[0].copy();h=cell['zM'][1]-cell['zM'][0]
    area=abs(np.cross(t[1]-t[0],t[2]-t[0]))/2;levels=[]
    for power in [16,18,20]:
        values=[]
        for seed in [20261024,20261025,20261026,20261027]:
            u=qmc.Sobol(4,scramble=True,seed=seed).random_base2(power)
            rho=np.linalg.norm(triangle(t,u[:,:2])-triangle(t,u[:,2:]),axis=1);assert rho.min()>0
            k=h/rho;kernel=2*h*(np.arcsinh(k)-k/(np.hypot(1,k)+1));values.append(float(area**2*np.mean(kernel)))
        levels.append({'power':power,'valuesM5':values,'meanM5':float(np.mean(values)),'seM5':float(np.std(values,ddof=1)/2)})
    last,prev=levels[-1],levels[-2];uncertainty=2*abs(last['meanM5']-prev['meanM5'])+6*last['seM5']
    rows.append({'postfixIndex':cell['postfixIndex'],'levels':levels,'referenceM5':last['meanM5'],'absoluteWorkingUncertaintyM5':uncertainty,
                 'relativeWorkingUncertainty':uncertainty/last['meanM5'],'nature':'Empirical/statistical working uncertainty, not a certified continuum enclosure.'})
(OUT/'postfix-reference.json').write_text(json.dumps({'rows':rows,'seconds':time.perf_counter()-start,'method':'Frozen positive4D exact-double-z; no native face/source-potential imports'},indent=2))
