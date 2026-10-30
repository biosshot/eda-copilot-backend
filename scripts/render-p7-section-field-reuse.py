from pathlib import Path
import json,sys,numpy as np,matplotlib.pyplot as plt
folder=Path(sys.argv[1]);r=json.loads((folder/'evidence.json').read_text());groups=r['evidence'];rows=[s for g in groups for s in g['samples']]
f=np.array([s['frequenciesHz'][0]/1e6 for s in rows]);z=[complex(s['network']['parameters'][0][1][0]['real'],s['network']['parameters'][0][1][0]['imaginary']) for s in rows]
fig,axes=plt.subplots(1,3,figsize=(17,6));fig.suptitle('Explicit PEC microstrip: one transverse preparation, new lengths and requested samples',fontsize=15)
axes[0].plot(f,np.abs(z),'o-');axes[0].set(xlabel='Requested frequency [MHz]',ylabel='|S21|, including mismatch',ylim=(.95,1.005));axes[0].grid(alpha=.2)
axes[1].plot(f,np.angle(z,deg=True),'o-');axes[1].set(xlabel='Requested frequency [MHz]',ylabel='S21 phase [degree]');axes[1].grid(alpha=.2)
for ax in axes[:2]:
 for x,y,s in zip(f,np.abs(z) if ax is axes[0] else np.angle(z,deg=True),rows):ax.annotate(str(s['input']['lengthMm'])+'mm',(x,y),xytext=(5,8),textcoords='offset points')
axes[2].axis('off');labels=['Cold field','Same worker, new length/axis','New process, new length/axis'];body=[]
for label,s in zip(labels,rows):body.append([label,s['cache']['field']['status'],str(s['cache']['assembly']),f"{s['callWallSeconds']:.3f}",f"{s['resources']['peakRssBytes']/2**20:.1f}"])
t=axes[2].table(cellText=body,colLabels=['Call','Field cache','Prep','Call s','RSS MiB'],loc='center',cellLoc='center',colWidths=[.6,.23,.13,.16,.21]);t.auto_set_font_size(False);t.set_fontsize(9);t.scale(1,2)
axes[2].set_title('Public direct DSL; cache entries native-validated')
fig.text(.03,.195,'Preparation includes native mesh/domain convergence solves; a zero count means no FEM reassembly.',fontsize=10)
fig.text(.03,.15,'Each point uses its labelled length; these are three requested samples, not a broadband validation curve.',fontsize=10)
fig.text(.03,.10,'Call time excludes worker startup. First scenario (cold + warm): %.3fs; restart scenario: %.3fs.'%(groups[0]['wallSeconds'],groups[1]['wallSeconds']),fontsize=10)
fig.text(.03,.055,'RSS is worker OS high-water, including preceding calls. Explicit uniform PEC model; no real-board RF-channel or5% accuracy certificate.',fontsize=10)
fig.subplots_adjust(bottom=.36,wspace=.4);fig.savefig(folder/'field-reuse-report.png',dpi=140)
