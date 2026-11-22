from pathlib import Path
import json,sys,numpy as np,matplotlib.pyplot as plt
folder=Path(sys.argv[1]);r=json.loads((folder/'evidence.json').read_text());rows=r['convergence'];n=[x['nodes'] for x in rows];num=lambda k:[np.nan if x.get(k)is None else 100*x[k] for x in rows]
fig,axes=plt.subplots(1,2,figsize=(13,6));fig.suptitle('Actual BandAmp transverse section: numerical admission REFUSED',fontsize=16)
axes[0].plot(n,num('maxMatrixRelativeChange'),'o-');axes[0].axhline(1,color='red',linestyle='--',label='C/C0/L gate1%');axes[0].set(xlabel='Mesh nodes',ylabel='Matrix change [%]',ylim=(0,1.2));axes[0].legend();axes[0].grid(alpha=.2)
axes[1].plot(n,num('lossPowerRelativeChangeBound'),'o-',label='Physical R/G energy bound');axes[1].plot(n,num('unweightedIntegralChangeDiagnostic'),'o:',label='Raw integral diagnostic');axes[1].axhline(2,color='red',linestyle='--',label='Loss-energy gate2%');axes[1].set(xlabel='Mesh nodes',ylabel='Change [%]');axes[1].legend();axes[1].grid(alpha=.2)
fig.text(.04,.16,'Captured copper, explicit fixed material hypotheses; physical total/component energy floor1e-3 of largest total self term.',fontsize=10)
fig.text(.04,.11,'Native %.3fs, OS peak %.2fMiB; final%d nodes. No threshold or resource cap was relaxed.'%(r['wallSeconds'],r['peakRssBytes']/2**20,n[-1]),fontsize=10)
fig.text(.04,.055,'One transverse plane; environment/end transitions and complete real RF channel remain unverified. No5% physical certificate.',fontsize=10)
fig.subplots_adjust(bottom=.28,wspace=.3);fig.savefig(folder/'convergence-report.png',dpi=140)
