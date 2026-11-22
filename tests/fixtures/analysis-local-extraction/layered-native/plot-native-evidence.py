"""Plot saved native values only; no solves."""
from pathlib import Path
import json,numpy as np,matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
root=Path(__file__).resolve().parents[4];out=root/'debugging/p7-design-checkpoint';audit=json.loads((out/'layered-native-audit.json').read_text());rows=audit['rows']
names=['failed-d32','accepted-d64-domain','accepted-d64-mesh','accepted-d64-mesh-cdt','shifted-d64','three-d64-mesh']
labels=['D32 failed','D64 domain','D64 mesh','Native CDT','Shifted foil','Three foils']
fig,ax=plt.subplots(2,2,figsize=(13,9),layout='constrained');fig.suptitle('Internal native layered capacitance — bounded coupon evidence, not PCB acceptance')
errors=np.array([rows[n]['independentFullError']*100 for n in names]);bars=ax[0,0].bar(labels,errors,color=['#bf5656']+['#3b8098']*5)
ax[0,0].axhline(5,color='#9b2226',ls='--',label='unchanged 5% gate');ax[0,0].set(ylabel='Full quadratic error (%)',ylim=(0,6.5));ax[0,0].tick_params(axis='x',rotation=25)
for b,v in zip(bars,errors):ax[0,0].text(b.get_x()+b.get_width()/2,v+.07,f'{v:.3f}',ha='center')
ax[0,0].errorbar([4,5],errors[[4,5]],yerr=[audit['referenceUncertainty'][tag]['n16to32FullChange']*100 for tag in ['shifted','three']],fmt='none',color='black',capsize=4,label='observed reference change (not bound)');ax[0,0].legend(fontsize=8)
x=np.arange(len(names));ax[0,1].bar(x-.18,[rows[n]['peakBytes']/1024**2 for n in names],width=.36,label='measured peak');ax[0,1].bar(x+.18,[rows[n]['result']['plan']['planned_simultaneous_bytes']/1024**2 for n in names],width=.36,label='simultaneous plan');ax[0,1].axhline(512,color='#9b2226',ls='--');ax[0,1].set(xticks=x,xticklabels=labels,ylabel='MiB',title='Each request ≤60 s, measured max6.99 s');ax[0,1].tick_params(axis='x',rotation=25);ax[0,1].legend(fontsize=8)
for n,label in [('accepted-d64-mesh','two foil'),('three-d64-mesh','three foil')]:
 f=rows[n]['result']['fieldSlice'];z=np.array([v['positionM'][2] for v in f])*1e3;e=np.array([np.linalg.norm(v['commonE']) for v in f]);ax[1,0].semilogy(z,e,'o-',ms=3,label=label)
ax[1,0].set(xlim=(-3,4),xlabel='z (mm)',ylabel='|E| (V/m)',title='Common1V field: nearest centroid to x1mm,y0 per slab');ax[1,0].legend(fontsize=8);ax[1,0].grid(alpha=.3)
c=np.array(rows['three-d64-mesh']['result']['capacitanceF'])*1e15;im=ax[1,1].imshow(c,cmap='coolwarm');fig.colorbar(im,ax=ax[1,1],label='fF');ax[1,1].set(xticks=range(3),yticks=range(3),xlabel='Excited conductor',ylabel='Charge conductor',title='Full three-conductor C; no hidden ground')
for i in range(3):
 for j in range(3):ax[1,1].text(j,i,f'{c[i,j]:.3f}',ha='center',va='center')
fig.savefig(out/'layered-native-audit.png',dpi=150)
