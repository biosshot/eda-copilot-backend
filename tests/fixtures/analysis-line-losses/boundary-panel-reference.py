"""Independent 2D boundary-panel collocation reference: rectangle over PEC ground.
Analytic logarithmic Green integrals; infinite-ground image kernel. No FEM import.
Reference generator only; no production coefficient correction or fitted values.
"""
import os
os.environ['OPENBLAS_NUM_THREADS']='1'
import numpy as np
from scipy.linalg import solve
from numpy.polynomial.legendre import leggauss
import math,json,time
EPS0=8.8541878128e-12;MU0=1.25663706212e-6

def panels(w,h,t,n):
    corners=np.array([[-w/2,h],[w/2,h],[w/2,h+t],[-w/2,h+t],[-w/2,h]])
    aa=[];bb=[]
    for a,b in zip(corners[:-1],corners[1:]):
        m=max(12,round(n*math.sqrt(np.linalg.norm(b-a)/w)))
        u=.5*(1-np.cos(np.linspace(0,np.pi,m+1)))
        pts=a[None,:]+u[:,None]*(b-a)[None,:]
        aa.extend(pts[:-1]);bb.extend(pts[1:])
    return np.array(aa),np.array(bb)

def primitive(u,d):
    r=np.hypot(u,d)
    return u*np.log(np.maximum(r,1e-300))-u+d*np.arctan2(u,np.maximum(d,1e-300))

def potential(x,a,b):
    vec=b-a;length=np.linalg.norm(vec,axis=1);tangent=vec/length[:,None]
    delta=x[:,None,:]-a[None,:,:]
    along=np.einsum('ijk,jk->ij',delta,tangent)
    perp=np.abs(delta[:,:,0]*tangent[None,:,1]-delta[:,:,1]*tangent[None,:,0])
    return (primitive(length[None,:]-along,perp)-primitive(-along,perp))/length[None,:]

def reference(w,h,t,f,n):
    return solve_panels(*panels(w,h,t,n),f,n,dict(widthM=w,heightM=h,thicknessM=t))

def solve_panels(a,b,f,n,geometry):
    start=time.time();mid=(a+b)/2;length=np.linalg.norm(b-a,axis=1)
    ai=a.copy();bi=b.copy();ai[:,1]*=-1;bi[:,1]*=-1
    A=(potential(mid,ai,bi)-potential(mid,a,b))/(2*np.pi)
    charge=solve(A,np.ones(len(a)),assume_a='gen',check_finite=True)
    cap=charge.sum();density=charge/length/cap
    signal=np.dot(density*density,length)
    xg,wg=leggauss(8);points=(a[:,None,:]+(xg[None,:,None]+1)/2*(b-a)[:,None,:]).reshape(-1,2)
    weights=(charge[:,None]*wg[None,:]/2/cap).reshape(-1)
    ground=0.
    for begin in range(0,len(points),128):
        p=points[begin:begin+128];Y=p[:,None,1]+points[None,:,1];X=p[:,None,0]-points[None,:,0]
        ground+=np.dot(weights[begin:begin+128],(Y/(np.pi*(X*X+Y*Y)))@weights)
    rs=math.sqrt(math.pi*f*MU0/58e6);z=math.sqrt(MU0/EPS0)/cap
    return dict(panels=len(a),n=n,**geometry,frequencyHz=f,capacitanceFPerM=cap*EPS0,impedanceOhm=z,signalGramPerM=signal,groundGramPerM=ground,resistanceOhmPerM=rs*(signal+ground),alphaNpPerM=rs*(signal+ground)/(2*z),linearResidual=float(np.max(np.abs(A@charge-1))),wallSeconds=time.time()-start)

def paired_reference(n,gap=.0002):
    w,h,t,f=.0003,.00018,.000035,1e9
    a,b=panels(w,h,t,n);count=len(a)
    left=np.array([-(w+gap)/2,0]);right=-left
    a=np.concatenate([a+left,a+right]);b=np.concatenate([b+left,b+right]);mid=(a+b)/2;length=np.linalg.norm(b-a,axis=1)
    ai=a.copy();bi=b.copy();ai[:,1]*=-1;bi[:,1]*=-1
    matrix=(potential(mid,ai,bi)-potential(mid,a,b))/(2*np.pi)
    rhs=np.zeros((len(a),2));rhs[:count,0]=1;rhs[count:,1]=1
    raw=solve(matrix,rhs,assume_a='gen');c=np.array([raw[:count].sum(axis=0),raw[count:].sum(axis=0)])
    current=raw@np.linalg.inv(c);density=current/length[:,None]
    contributions=[density[:count].T@(density[:count]*length[:count,None]),density[count:].T@(density[count:]*length[count:,None])]
    xg,wg=leggauss(8);points=(a[:,None,:]+(xg[None,:,None]+1)/2*(b-a)[:,None,:]).reshape(-1,2)
    weights=(current[:,None,:]*wg[None,:,None]/2).reshape(-1,2)
    ground=np.zeros((2,2))
    for begin in range(0,len(points),128):
        p=points[begin:begin+128];Y=p[:,None,1]+points[None,:,1];X=p[:,None,0]-points[None,:,0]
        ground+=weights[begin:begin+128].T@(Y/(np.pi*(X*X+Y*Y)))@weights
    contributions.append(ground);total=sum(contributions);rs=math.sqrt(math.pi*f*MU0/58e6)
    assert np.max(np.abs(np.array([current[:count].sum(axis=0),current[count:].sum(axis=0)])-np.eye(2)))<1e-12
    return dict(n=n,panels=len(a),line=dict(kind='coupled-microstrip',widthMm=w*1000,dielectricHeightMm=h*1000,copperThicknessMm=t*1000,gapMm=gap*1000,relativePermittivity=1),frequencyHz=f,capacitanceFPerM=(c*EPS0).tolist(),surfaceGramPerM=[v.tolist()for v in contributions],totalGramPerM=total.tolist(),resistanceOhmPerM=(rs*total).tolist(),modeResistanceOhmPerM=dict(even=rs*(total[0,0]+total[0,1]),odd=rs*(total[0,0]-total[0,1])),linearResidual=float(np.max(np.abs(matrix@raw-rhs))))

def power_error(a,b):
    eigen,u=np.linalg.eigh(b)
    assert eigen.min()>0
    whiten=u@np.diag(1/np.sqrt(eigen))@u.T
    return float(max(abs(np.linalg.eigvalsh(whiten@(a-b)@whiten))))

if __name__=='__main__':
    results=dict(method='Independent constant boundary panels, midpoint collocation, analytic log Green integrals with exact infinite-plane images; no FEM imports',conventions='exp(+j omega t); unit total longitudinal signal current; Rs=sqrt(pi f mu0/sigma); alpha=R/(2 Z0)',dependencies=dict(numpy=np.__version__,scipy=__import__('scipy').__version__),sigmaSM=58e6,referenceThicknessM=.000035,geometryUnchanged=True,circle=[],square=[],fixedCases=[])
    H=.0005;a=.0001;B=math.sqrt(H*H-a*a)
    exact=dict(capacitanceFPerM=2*np.pi*EPS0/math.acosh(H/a),signalGramPerM=H/(2*np.pi*a*B),groundGramPerM=1/(2*np.pi*B))
    for n in [128,256,512]:
        theta=np.linspace(0,2*np.pi,n+1);points=np.column_stack([a*np.cos(theta),H+a*np.sin(theta)])
        r=solve_panels(points[:-1],points[1:],1e9,n,dict(kind='cylinder over plane',radiusM=a,centerHeightM=H));r['exact']=exact;r['relativeErrors']={key:r[key]/value-1 for key,value in exact.items()};results['circle'].append(r)
        assert max(abs(x) for x in r['relativeErrors'].values())<.002
    # Wheeler 1955 polygon rule: square of side w has the isolated skin loss of
    # its inscribed circle radius w/2. H/w=1000 makes ground perturbation tiny.
    for n in [160,320,640]:
        r=reference(.001,1.,.001,1e9,n);r['isolatedSignalExactGramPerM']=1/(np.pi*.001);r['relativeSignalError']=r['signalGramPerM']/(1/(np.pi*.001))-1;results['square'].append(r)
    for w,h,t,f in [(.3e-3,.18e-3,.005e-3,20e9),(1e-3,.3e-3,.01e-3,5e9)]:
        history=[reference(w,h,t,f,n) for n in [160,320,640,1280]]
        previous,last=history[-2:];estimate={key:last[key]+(last[key]-previous[key])/(2**(2/3)-1) for key in ['signalGramPerM','groundGramPerM','resistanceOhmPerM','alphaNpPerM']}
        observed=math.log((history[-2]['signalGramPerM']-history[-3]['signalGramPerM'])/(history[-1]['signalGramPerM']-history[-2]['signalGramPerM']),2)
        results['fixedCases'].append(dict(history=history,finest=last,cornerRichardsonEstimate=estimate,observedCornerOrder=observed,estimatedRelativeUncertainty=.01,uncertaintyMeaning='Conservative estimate from last panel changes and r^(-1/3) rectangular-corner asymptotics with cosine grading; not a rigorous bound. No FEM output or engineering result used.'))
    results['pairedCase']=[paired_reference(n) for n in [80,160,320]]
    results['pairedCases']=[]
    for gap in [.000035,.0006]:
        history=[paired_reference(n,gap) for n in [80,160,320,640]]
        a=np.array(history[-1]['resistanceOhmPerM']);b=np.array(history[-2]['resistanceOhmPerM']);limit=a+(a-b)/(2**(2/3)-1)
        results['pairedCases'].append(dict(gapMm=gap*1000,estimatedCornerLimitRelativeUncertainty=.015,uncertaintyMeaning='Estimated corner-limit uncertainty, not a rigorous bound; nominal reference remains finest raw panels; per-entry remaining tail shown separately',history=history,cornerRichardsonResistanceOhmPerM=limit.tolist(),finestResidualEstimatePerEntry=((limit-a)/limit).tolist(),finestToCornerPowerResidualEstimate=power_error(a,limit),powerRefinementChanges=[power_error(np.array(x['resistanceOhmPerM']),np.array(y['resistanceOhmPerM'])) for x,y in zip(history[1:],history[:-1])]))
    from pathlib import Path
    Path('tests/fixtures/analysis-line-losses/boundary-panel-references.json').write_text(json.dumps(results,indent=2)+'\n',encoding='utf8')
    print(json.dumps(results,indent=2))
