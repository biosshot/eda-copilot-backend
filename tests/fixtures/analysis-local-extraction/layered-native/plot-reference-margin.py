"""Plot saved three-reference margin values; no solves."""
from pathlib import Path
import json,numpy as np,matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint/three-reference-margin';a=json.loads((OUT/'audit.json').read_text());rows=a['referenceRows'];n=[r['refinement'] for r in rows]
fig,ax=plt.subplots(2,2,figsize=(12,8),layout='constrained');fig.suptitle('Three-foil reference margin: same saved native matrix, one higher offline reference\nWorking screen passes; observed changes are not certified continuum bounds',fontsize=13)
ax[0,0].plot(n,[100*r['nativeFullError'] for r in rows],'o-',label='native full error');ax[0,0].plot(n[1:],[100*r['workingScreen'] for r in rows[1:]],'s--',label='error +2×last reference change');ax[0,0].axhline(5,color='firebrick',ls=':',label='unchanged5% gate');ax[0,0].set(xlabel='Reference refinement n',ylabel='Full quadratic error/screen (%)',xticks=n);ax[0,0].legend(fontsize=9);ax[0,0].grid(alpha=.3)
for r in rows[1:]:ax[0,0].annotate(f"{100*r['workingScreen']:.5f}%",(r['refinement'],100*r['workingScreen']),xytext=(0,7),textcoords='offset points',ha='center')
keys=['16->32','32->48','16->48'];values=[a['observedFullReferenceChanges'][k]*100 for k in keys];bars=ax[0,1].bar(keys,values,color=['#457b9d','#2a9d8f','#aaa']);ax[0,1].set(ylabel='Observed full reference change (%)',title='Sequential changes strictly decrease')
for b,v in zip(bars,values):ax[0,1].text(b.get_x()+b.get_width()/2,v+.004,f'{v:.6f}',ha='center')
e=np.array(rows[-1]['rawEntryRelativeErrors'])*100;im=ax[1,0].imshow(e,cmap='coolwarm',vmin=-4,vmax=4);fig.colorbar(im,ax=ax[1,0],label='Signed raw entry error (%)');ax[1,0].set(xticks=range(3),yticks=range(3),xlabel='Excitation conductor',ylabel='Charge conductor',title='Native vs n48 unsymmetrized reference')
for i in range(3):
 for j in range(3):ax[1,0].text(j,i,f'{e[i,j]:.3f}',ha='center',va='center')
resources=a['resources'];native=a['nativeReuse'];x=np.arange(2);ax[1,1].bar(x,[native['peakBytes']/1024**2,resources['peakBytes']/1024**2],color=['#457b9d','#e9a23b']);ax[1,1].plot(x,[512,1024],'rx',ms=10,label='respective explicit caps');ax[1,1].set(xticks=x,xticklabels=['Saved native','New reference n48'],ylabel='Measured peak MiB',ylim=(0,1100),title='Separate native512MiB / offline1024MiB limits')
for i,r in enumerate([native,resources]):ax[1,1].text(i,r['peakBytes']/1024**2+25,f"{r['seconds']:.3f} s",ha='center')
ax[1,1].legend();fig.savefig(OUT/'audit.png',dpi=150)
