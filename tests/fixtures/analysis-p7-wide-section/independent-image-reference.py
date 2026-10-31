"""Independent circular-wire / infinite-plane image Green-function collocation.
Point sources inside metal; no FEM mesh, production imports or finite-R model.
"""
import json,pathlib,numpy as np
from scipy.linalg import expm
EPS=8.8541878128e-12;MU=4*np.pi*1e-7

def solve(n):
 t=np.arange(n)*2*np.pi/n
 centers=np.array([[-.00015,.0003],[.00015,.0003]])
 ring=np.array([np.cos(t),np.sin(t)]).T
 boundary=np.concatenate([c+1e-5*ring for c in centers])
 source=np.concatenate([c+.6e-5*ring for c in centers]);image=source.copy();image[:,1]*=-1
 def operator(points):
  return np.log(np.linalg.norm(points[:,None,:]-image[None,:,:],axis=2)/np.linalg.norm(points[:,None,:]-source[None,:,:],axis=2))/(2*np.pi*EPS)
 rhs=np.zeros((2*n,2));rhs[:n,0]=1;rhs[n:,1]=1
 q=np.linalg.solve(operator(boundary),rhs)
 C=np.array([q[:n].sum(axis=0),q[n:].sum(axis=0)])
 test=np.concatenate([c+1e-5*np.array([np.cos(t+np.pi/n),np.sin(t+np.pi/n)]).T for c in centers])
 return dict(n=n,C=C.tolist(),residual=float(np.max(np.abs(operator(test)@q-rhs))))
if __name__=='__main__':
 runs=[solve(n) for n in [24,32,48]];C=np.array(runs[-1]['C']);diff=float(np.max(np.abs(C-np.array(runs[-2]['C']))/np.abs(C)))
 assert diff<.001 and runs[-1]['residual']<.001
 L=MU*EPS*np.linalg.inv(C);Z=[np.sqrt(MU*EPS)/(C[0,0]+C[0,1]),np.sqrt(MU*EPS)/(C[0,0]-C[0,1])]
 S=[];eye=np.eye(2);zero=np.zeros((2,2))
 for f in [1e6,1e7,1e8]:
  T=expm(-1j*2*np.pi*f*.01*np.block([[zero,L],[C,zero]]));V=np.block([[eye,zero],[T[:2,:2],T[:2,2:]]]);I=np.block([[zero,eye],[-T[2:,:2],-T[2:,2:]]]);s=(V-50*I)@np.linalg.inv(V+50*I);S.append([[dict(real=float(z.real),imaginary=float(z.imag)) for z in row] for row in s])
 pathlib.Path('tests/fixtures/analysis-p7-wide-section/image-reference.json').write_text(json.dumps(dict(S=S,runs=runs,C=C.tolist(),L=L.tolist(),Z=Z,refinementDifference=diff,uncertainty=.03,scope='infinite plane; exact circular cross-section image Green operator, collocation engineering estimate'),indent=2))
 print(diff,runs[-1]['residual'])

