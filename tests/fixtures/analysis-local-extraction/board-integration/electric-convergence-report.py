"""Summarize the four supervised electric-only packets; never starts a solver."""
from pathlib import Path
import json,math
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np
root=Path(__file__).resolve().parents[4]
fixture=Path(__file__).parent
raw=root/'debugging/p7-board-integration'
names=['coarse','candidate','grid','boundary']
packets={n:raw/('electric-convergence-v1-'+n)/'direct-fem' for n in names}
runs={n:json.loads((p/'run.json').read_text()) for n,p in packets.items()}
data={n:json.loads((p/'stdout.json').read_text()) for n,p in packets.items() if runs[n]['exitCode']==0}
def cmat(d):
 m=np.zeros((3,3))
 for i,j,v in d['componentCapacitanceF']:m[i,j]=v
 return m
def z(d):return np.array([[[complex(v['real'],v['imaginary']) for v in row] for row in sample] for sample in d['refined']['parameters']])
summary={'kind':'bounded-electric-convergence-v1','physicalAdmission':False,'runs':runs,'cases':{},'comparisons':{}}
for n,d in data.items():
 c=cmat(d)
 summary['cases'][n]={'componentCapacitanceF':c.tolist(),'componentEnergiesJ':d['componentEnergiesJ'],'commonExcitationEnergyJ':float(.5*c.sum()),'nodes':d['fieldNodes'],'tetrahedra':d['fieldTetrahedra'],'frequenciesHz':d['refined']['frequenciesHz'],'parameters':d['refined']['parameters']}
if 'candidate' in data:
 ref=data['candidate'];cr=cmat(ref);zr=z(ref)
 for n in ['coarse','grid','boundary']:
  if n not in data:continue
  d=data[n];delta=np.abs(z(d)-zr)/np.maximum(np.abs(zr),1e-6)
  summary['comparisons'][n]={'maxZChange':float(delta.max()),'all68ZChanges':delta.tolist(),'componentMatrixMaxNormChange':float(np.max(np.abs(cmat(d)-cr))/np.max(np.abs(cr))),'commonExcitationEnergyRelativeChange':float(abs(cmat(d).sum()/cr.sum()-1)), 'independentEnergyRelativeChanges':(np.abs(np.diag(cmat(d))-np.diag(cr))/np.abs(np.diag(cr))).tolist()}
 if 'boundary' in data:
  # Exact coordinate comparison in candidate near box (ROI +/- 1 mm).
  lo=[-.001,-.001,-.001];hi=[.001726,.001352,.0026]
  def near(d):
   nodes=d['geometryNodesM'];inside=lambda p:all(lo[k]<=p[k]<=hi[k] for k in range(3))
   vertices={tuple(p) for p in nodes if inside(p)}
   cells={tuple(sorted(tuple(nodes[i]) for i in t)) for t in d['geometryTetrahedra'] if all(inside(nodes[i]) for i in t)}
   return vertices,cells
  a,b=near(ref),near(data['boundary'])
  summary['nearRegionAudit']={'boundsM':[lo,hi],'candidateVertices':len(a[0]),'candidateTetrahedra':len(a[1]),'removedVertices':len(a[0]-b[0]),'addedVertices':len(b[0]-a[0]),'removedTetrahedra':len(a[1]-b[1]),'addedTetrahedra':len(b[1]-a[1]),'mixed':a!=b}
out=raw/'electric-convergence-v1-visuals';out.mkdir(exist_ok=True)
fig,axs=plt.subplots(1,2,figsize=(12,4))
for n,d in data.items():
 axs[0].plot(np.diag(cmat(d))*1e15,'o-',label=n)
 axs[1].loglog(d['refined']['frequenciesHz'],np.abs(z(d)[:,0,0]),label=n)
axs[0].set(xlabel='Connected conductor index',ylabel='C diagonal (fF)');axs[1].set(xlabel='Hz',ylabel='|Z00| (Ohm)')
for ax in axs:ax.legend();ax.grid(True)
fig.suptitle('Electric convergence in fixed magnetic subspace; differences are not true error');fig.tight_layout();fig.savefig(out/'capacitance-z.png',dpi=150)
fig,ax=plt.subplots(figsize=(7,5))
for n in ['candidate','boundary']:
 if n in data:
  pts=np.unique(data[n]['geometryXyM'],axis=0);ax.scatter(pts[:,0]*1e3,pts[:,1]*1e3,s=5,label=n)
ax.set(xlabel='local x (mm)',ylabel='local y (mm)',title='8 / 16 mm boundaries: actual FEM vertices');ax.axis('equal');ax.legend();fig.tight_layout();fig.savefig(out/'geometry-boundary.png',dpi=150)
(fixture/'electric-convergence-result-v1.json').write_text(json.dumps(summary,indent=2)+'\n')
print(json.dumps({n:{k:v for k,v in x.items() if k!='all68ZChanges'} for n,x in summary['comparisons'].items()},indent=2))
