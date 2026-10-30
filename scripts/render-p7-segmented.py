"""Render actual computed channel evidence, retaining failed accuracy captions."""
import json,sys,math
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
folder=Path(sys.argv[1]);packet=json.loads((folder/'result.json').read_text(encoding='utf-8'));solution=next(c['result']['solution'] for r in packet['reports'] for c in r['calculations'] if c['phase']=='completed');comparison=json.loads((folder/'whole-reference-comparison.json').read_text(encoding='utf-8')) if (folder/'whole-reference-comparison.json').exists() else None
fig,axs=plt.subplots(2,2,figsize=(13,8));ax=axs[0,0]
for b in solution['audit']['slabs']:
 ax.axvspan(b['a'],b['b'],color='#e4a65a' if b['kind']=='local' else '#6ab7e7',alpha=.6);ax.text((b['a']+b['b'])/2,.7,b['kind'],ha='center')
ax.plot([0,10],[.4,.4],color='red',lw=3,label='TOP signal');ax.plot([0,10],[.2,.2],color='blue',lw=3,label='BOTTOM finite return');ax.scatter([0,10],[.4,.4],color='black');ax.set(ylim=(0,1),xlabel='Longitudinal coordinate (mm)',yticks=[],title='Automatic physical ownership / 9.32 mm section');ax.legend(loc='lower center',ncol=2)
f=solution['result']['frequenciesHz'];mat=solution['result']['parameters'];axs[0,1].semilogx(f,[abs(complex(m[1][0]['real'],m[1][0]['imaginary'])) for m in mat],'o-',label='composed');axs[0,1].set(title='Actual loaded transmission',xlabel='Frequency (Hz)',ylabel='|S21|');values=[abs(complex(m[1][0]['real'],m[1][0]['imaginary']))for m in mat];centre=(max(values)+min(values))/2;width=max(.001,max(values)-min(values));axs[0,1].set_ylim(centre-width,centre+width);axs[0,1].ticklabel_format(axis='y',style='plain',useOffset=False)
if comparison:
 rows=comparison['rows'];axs[1,0].semilogx([r['frequencyHz'] for r in rows],[r['composedDelayS']*1e12 for r in rows],'o-',label='section/local');axs[1,0].semilogx([r['frequencyHz'] for r in rows],[r['fullDelayS']*1e12 for r in rows],'s-',label='whole-volume control');axs[1,0].set(title='Phase delay: independent whole-volume control',xlabel='Frequency (Hz)',ylabel='Phase delay (ps)');axs[1,0].legend()
 axs[1,1].bar(['complex S','Cu loss','phase delay'],[comparison['maxRelativeComplexSError']*100,comparison['maxRelativeCuLossError']*100,comparison['maxRelativePhaseDelayError']*100]);axs[1,1].axhline(5,color='red',ls='--',label='5% target');axs[1,1].set(ylabel='Relative error (%)',title='Composition vs whole-volume control');axs[1,1].legend()
for a in axs.flat:a.grid(alpha=.2)
fig.suptitle('Actual numerical section + local FEM + physical P6 channel; 1-10 kHz',fontsize=14)
fig.text(.04,.015,'Conditional coupon evidence. Whole-volume XY convergence is pending; delay currently fails5%.\nCache mode and cold/warm wall are recorded in the measurement manifest. No real-board/HF/transient certificate.',color='darkred',fontsize=10)
fig.tight_layout(rect=[0,.075,1,.95]);fig.savefig(folder/'channel-evidence.png',dpi=140);print((folder/'channel-evidence.png').resolve())
