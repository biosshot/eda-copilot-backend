import json,pathlib,sys,numpy as np,matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from scipy.linalg import expm
root=pathlib.Path(sys.argv[1]);r=json.loads((root/'result.json').read_text());ref=json.loads(pathlib.Path('tests/fixtures/analysis-p7-wide-section/image-reference.json').read_text());n=r['report']['network'];C=np.array(ref['C']);L=np.array(ref['L']);eye=np.eye(2);zero=np.zeros((2,2));Ss=[]
for f in [1e6,1e7,1e8]:
 # Transmission matrix [Vout;Iout]=T[Vin;Iin], I positive along z.
 T=expm(-1j*2*np.pi*f*.01*np.block([[zero,L],[C,zero]]));V=np.block([[eye,zero],[T[:2,:2],T[:2,2:]]]);I=np.block([[zero,eye],[-T[2:,:2],-T[2:,2:]]]);Ss.append((V-50*I)@np.linalg.inv(V+50*I))
actual=np.array([[[complex(z['real'],z['imaginary']) for z in row]for row in matrix]for matrix in n['parameters']]);reference=np.array(Ss);error=float(np.max(np.abs(actual-reference)/np.maximum(np.abs(reference),1e-4)));p=json.loads((root/'comparison.json').read_text());p['errors']['complexS']=error;p['status']='pass' if max(p['errors'].values())+.03<=.08 else 'fail';(root/'comparison.json').write_text(json.dumps(p,indent=2));print(p['errors'],n['ports'])
fig,ax=plt.subplots(2,2,figsize=(11,7));inp=json.loads((root/'input.json').read_text())
for c in inp['steps'][-1]['request']['line']['geometry']['conductors']:
 xy=np.array(c['contour']['outerM'])*1000;ax[0,0].fill(xy[:,0],xy[:,1],label=c['id'])
ax[0,0].set(xlabel='transverse mm',ylabel='mm',title='Physical copper; finite return');ax[0,0].legend();ax[0,0].set_ylim(-.1,.4)
for key,index in [('C',0),('L',1)]:
 a=[v['observables'][key] for v in r['audit']];scale=1e12 if key=='C' else 1e9;ax[0,1].plot([v[index]*scale for v in a],label=key+(' self pF/m' if index==0 else ' mutual nH/m'))
ax[0,1].set(title='Joint coefficients by refinement',xlabel='step');ax[0,1].legend()
for k in ['C','L','Z','S']:ax[1,0].plot([100*v.get('errors',{}).get(k,0) for v in r['audit']],label=k)
ax[1,0].axhline(2,color='k',ls='--');ax[1,0].set(title='Successive sensitivity %',xlabel='step');ax[1,0].legend()
ax[1,1].loglog([1e6,1e7,1e8],np.abs(actual[:,3,0]),'o-',label='native mutual S30');ax[1,1].loglog([1e6,1e7,1e8],np.abs(reference[:,3,0]),'x--',label='independent');ax[1,1].set(xlabel='Hz',title='Nonzero coupled response');ax[1,1].legend();fig.tight_layout();fig.savefig(root/'wide-section.png',dpi=140)

