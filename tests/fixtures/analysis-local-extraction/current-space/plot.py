"""Render saved DC/extraction and explicit AC refusal evidence; no solver rerun."""
from pathlib import Path
import json,numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from mpl_toolkits.mplot3d.art3d import Poly3DCollection
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-completion'
v=json.loads((OUT/'coupon.json').read_text());audit=json.loads((OUT/'audit.json').read_text());ref=json.loads((OUT/'references.json').read_text());r=v['runs'][0]
assert audit['status']=='partial-refusal-preserved'
fig=plt.figure(figsize=(13,9),layout='constrained');fig.suptitle('Spatial field trace and finite foil currents: PARTIAL implementation\nInsulated authored coupon; AC band REJECTED; no PCB, via or P7 completion')
ax=fig.add_subplot(221,projection='3d')
for k,f in enumerate(r['foils']):
 xy=np.array(f['xy_m'])*1000;z=sum(f['z_m'])/2*1000
 polys=[np.column_stack([xy[t],np.full(3,z)]) for t in f['triangles']]
 ax.add_collection3d(Poly3DCollection(polys,alpha=.6,facecolors=['#2963b7','#e38e29'][k],edgecolors='black'))
 ax.quiver(.25,.25,z,(1 if k==0 else -1)*.2,0,0,color='black',arrow_length_ratio=.25)
 for endpoint in [0,1]:
  x=endpoint*.5;ax.plot([x,x],[0,.5],[z,z],color='crimson',lw=3);ax.text(x,.52,z,str(2*k+endpoint),fontsize=10)
ax.set(xlim=(-.1,.6),ylim=(-.1,.7),zlim=(-.1,1.2),xlabel='x [mm]',ylabel='y [mm]',zlabel='z [mm]',title='Four finite electrode edges; two triangles per foil\nOpposing 1 A DC drive; no return via invented')
ax=fig.add_subplot(222);d=np.array(r['dcDensityAPerM2']);loss=np.array(r['dcCellLossesW'])*1000
ax.bar(np.arange(4)-.18,d[:,0]/1e6,width=.35,label='Jx');ax.bar(np.arange(4)+.18,d[:,1]/1e6,width=.35,label='Jy')
ax.set(xticks=range(4),xlabel='Triangle index',ylabel='Current density [MA/m^2]',title='Recovered DC; total loss %.6f mW'%sum(loss));ax.legend();ax.grid(alpha=.2)
ax=fig.add_subplot(223);C=np.zeros((4,4))
for i,j,x in r['model']['capacitanceF']:C[i,j]=x
im=ax.imshow(C*1e15,cmap='coolwarm');fig.colorbar(im,ax=ax,label='Capacitance [fF]')
for i in range(4):
 for j in range(4):ax.text(j,i,'%.3f'%(C[i,j]*1e15),ha='center',va='center',fontsize=10)
ax.set(xticks=range(4),yticks=range(4),xlabel='Potential coordinate',ylabel='Potential coordinate',title='Full spatial C from B^T K B\nAnalytic tensor scaled error %.2g'%audit['runs'][0]['CScaledError'])
ax=fig.add_subplot(224);labels=['field','magnetic + A/R','projection','complete path'];values=[r['electricSeconds'],r['currentAssemblySeconds'],r['reductionSeconds'],r['fullPathSeconds']]
ax.barh(labels,np.array(values)*1000);ax.set(xlabel='Wall time [ms]',title='Coupon only; no board timing extrapolation');ax.grid(axis='x',alpha=.2)
fig.savefig(OUT/'current-space-dc.png',dpi=150);plt.close(fig)
fig,axs=plt.subplots(2,2,figsize=(13,9),layout='constrained');fig.suptitle('AC band REJECTED: 8 of 17 required points fail\nNine passing diagnostic points do not establish a reusable frequency/transient model')
f=np.array([p['frequencyHz'] for p in ref['samples']]);z=np.array([complex(*p['loopZOhm']) for p in ref['samples']]);axs[0,0].semilogx(f/1000,abs(z)*1000,'k--',label='independent 6-state reference')
for s in r['sweep']:
 i=int(np.argmin(abs(f-s['frequencyHz'])));x=s['frequencyHz']/1000
 if s['nativePointAdmitted']:axs[0,0].plot(x,abs(complex(*s['loopZOhm']))*1000,'o',color='#2963b7')
 else:axs[0,0].plot(x,abs(z[i])*1000,'rx',ms=9)
axs[0,0].set(xlabel='Frequency [kHz]',ylabel='Loop |Z| [mOhm]',title='Red crosses: candidate unavailable at required frequencies');axs[0,0].legend()
rows=audit['runs'][0]['rows'];ok=[p for p in rows if p['status']!='rejected']
for key,label in [('independentLoopZError','loop Z'),('independentPhysicalCurrentError','current norm'),('independentLocalLossError','local loss')]:axs[0,1].semilogx([p['frequencyHz']/1000 for p in ok],[100*p[key] for p in ok],'o',label=label)
axs[0,1].axhline(1,color='crimson',ls=':');axs[0,1].set(xlabel='Frequency [kHz]',ylabel='Error [%]',title='Diagnostic passing points only; unchanged 1% gate');axs[0,1].legend()
axs[1,0].axis('off');axs[1,0].text(0,.98,'Structural null-mode blocker\n\nMathematical common C mode has zero energy.\nF64 assembled C retains roundoff in that mode.\nP6 can mistake it for tiny stored common charge.\nUnresolved gauge produces large potentials;\npower/residual/singularity guards reject.\n\nNo C clipping, hidden ground, discarded point,\nchanged loss term or relaxed guard is used.',va='top',fontsize=12)
roi=audit['realRoiRefusals'];x=np.arange(3);axs[1,1].bar(x,[p['foilSupports'] for p in roi],label='foil supports');axs[1,1].bar(x,[p['axialSupports'] for p in roi],bottom=[p['foilSupports'] for p in roi],label='axial supports (unsupported)');axs[1,1].axhline(64,color='crimson',ls=':',label='experimental all-foil cap64')
axs[1,1].set(xticks=x,xticklabels=['STM32G0','TPS63060','BandAmpV5'],ylabel='Saved experimental support count',title='Real ROI refusal; counts are not a valid current mesh');axs[1,1].legend(fontsize=8)
for ax in axs.flat:ax.grid(alpha=.2)
fig.savefig(OUT/'current-space-ac-refusal.png',dpi=150);plt.close(fig)
print('Rendered two static saved-value figures; visual inspection required')
