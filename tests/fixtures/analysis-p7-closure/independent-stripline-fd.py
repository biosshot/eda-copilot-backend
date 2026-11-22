"""Independent finite-difference electrostatic reference; no production imports."""
import numpy as np
from scipy.sparse import diags
from scipy.sparse.linalg import spsolve
import json,time,pathlib

def solve(h,half_width):
    # mm; exact node-aligned finite conductor boundaries and ground planes.
    nx=round(2*half_width/h)+1; ny=round(.635/h)+1
    x=np.linspace(-half_width,half_width,nx); y=np.linspace(0,.635,ny)
    xx,yy=np.meshgrid(x,y); ids=np.arange(nx*ny).reshape(ny,nx)
    c=[(xx>=a-1e-9)&(xx<=b+1e-9)&(yy>=.3-1e-9)&(yy<=.335+1e-9) for a,b in [(-.45,-.15),(.15,.45)]]
    fixed=c[0]|c[1]; fixed[[0,-1],:]=True;fixed[:,[0,-1]]=True
    free=~fixed.ravel(); n=nx*ny
    a=diags([-np.ones(n-nx),-np.ones(n-1),4*np.ones(n),-np.ones(n-1),-np.ones(n-nx)],[-nx,-1,0,1,nx],format='csr')
    potentials=[]; residual=[]
    for mask in c:
        v=mask.astype(float).ravel(); rhs=-(a@v)[free];af=a[free][:,free]
        v[free]=spsolve(af,rhs);residual.append(float(np.linalg.norm((a@v)[free])/np.linalg.norm(rhs)));potentials.append(v.reshape(ny,nx))
    # Bilinear discrete Dirichlet energy = charge matrix; uniform square grid metric cancels.
    cap=np.zeros((2,2));eps=8.8541878128e-12*4
    for i in range(2):
        for j in range(2):
            cap[i,j]=eps*sum(np.sum(np.diff(potentials[i],axis=k)*np.diff(potentials[j],axis=k)) for k in [0,1])
    return dict(hMm=h,halfWidthMm=half_width,nodes=n,capacitanceFPerM=cap.tolist(),normalizedResidual=max(residual))
if __name__=='__main__':
    t=time.time(); runs=[solve(.005,1.5),solve(.0025,1.5),solve(.0025,2.)]
    c=np.array(runs[-1]['capacitanceFPerM']); mesh=float(np.max(np.abs(c-np.array(runs[0]['capacitanceFPerM']))/np.abs(c))); domain=float(np.max(np.abs(c-np.array(runs[1]['capacitanceFPerM']))/np.abs(c)))
    packet=dict(method='independent five-point Laplace Dirichlet energy on Cartesian grid; no native/producer import',geometry=dict(widthMm=.3,gapMm=.3,copperThicknessMm=.035,groundPlanesYMm=[0,.635],conductorYMm=[.3,.335],relativePermittivity=4),runs=runs,data=dict(capacitanceFPerM=c.tolist(),inductanceHPerM=(4/299792458**2*np.linalg.inv(c)).tolist(),impedanceOhm=[2/299792458/(c[0,0]+c[0,1]),2/299792458/(c[0,0]-c[0,1])],effectivePermittivity=[4,4]),uncertainty=dict(relativeBound=.02,kind='engineering numerical estimate, not rigorous bound',meshDifference=mesh,domainDifference=domain,policy='preassigned 2% allowance; mesh and domain differences each must be below 1%; included additively within 8% acceptance'),wallSeconds=time.time()-t)
    assert mesh<.01 and domain<.01,(mesh,domain)
    pathlib.Path('tests/fixtures/analysis-p7-closure/stripline-fd-reference.json').write_text(json.dumps(packet,indent=2))
    print(json.dumps({k:packet[k] for k in ['uncertainty','wallSeconds','data']}))
