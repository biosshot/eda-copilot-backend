"""Saved magnetic numeric evidence only; never runs kernels/references."""
from pathlib import Path
import json,numpy as np,matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-stability';a=json.loads((OUT/'audit.json').read_text());fig,ax=plt.subplots(2,2,figsize=(13,9),layout='constrained');fig.suptitle('Finite magnetic kernel coupon — STM stability and full partial-L operator\nFull4×4 basis and entire3D terminal-conserving space; ROI production cost remains blocked',fontsize=13)
labels=['668/556 forward','668/556 reverse','668 self','556 self'];x=np.arange(4)
for order,offset,color in [(12,-.1,'#457b9d'),(24,.1,'#2a9d8f')]:
 selected=[next(p for p in a['pairs'] if p['label']==label and p['xyOrder']==order) for label in ['stm668-556','stm668-556','stm668-self','stm556-self']];directions=['forwardM5','reverseM5','forwardM5','forwardM5'];values=[p['signedRelativeErrors'][d]*100 for p,d in zip(selected,directions)];errors=[p['referenceRelativeUncertainty']*100 for p in selected];ax[0,0].errorbar(x+offset,values,yerr=errors,fmt='o',capsize=4,color=color,label=f'XY{order}/z{8 if order==12 else 16}')
ax[0,0].axhline(.2,color='firebrick',ls='--');ax[0,0].axhline(-.2,color='firebrick',ls='--');ax[0,0].set(xticks=x,xticklabels=labels,ylabel='Signed integral error (%)',ylim=(-.22,.22),title='Independent references with separate uncertainty');ax[0,0].tick_params(axis='x',rotation=15);ax[0,0].legend();ax[0,0].grid(alpha=.2)
final=a['operator'][-1];matrix=np.array(final['rawLH'])*1e9;im=ax[0,1].imshow(matrix,cmap='viridis');fig.colorbar(im,ax=ax[0,1],label='nH');ax[0,1].set(xticks=range(4),yticks=range(4),xlabel='Current basis',ylabel='Current basis',title=f"Full4×4 L; full quadratic error {final['fullPositiveRangeError']*100:.3g}%")
for i in range(4):
 for j in range(4):ax[0,1].text(j,i,f'{matrix[i,j]:.5f}',ha='center',va='center',color='white' if matrix[i,j]<.2 else 'black')
vals=final['physicalEigenvaluesH'];ref=final['physicalReferenceEigenvaluesH'];idx=np.arange(3);ax[1,0].bar(idx-.15,np.array(vals)*1e9,width=.3,label='native');ax[1,0].bar(idx+.15,np.array(ref)*1e9,width=.3,label='independent reference');ax[1,0].set(xticks=idx,xticklabels=['mode1','mode2','mode3'],ylabel='Physical-space eigenvalue (nH)',title=f"All physical modes positive; energy-norm error {final['physicalPositiveRangeError']*100:.3g}%");ax[1,0].legend()
roi=a['roiCostPreflight'];idx=np.arange(3);ax[1,1].bar(idx-.16,[v['oneDirectionCoarseSecondsEstimate'] for v in roi],width=.32,label='coarse one-direction estimate');ax[1,1].bar(idx+.16,[v['oneDirectionRefinedSecondsEstimate'] for v in roi],width=.32,label='refined one-direction estimate');ax[1,1].axhline(500,color='firebrick',ls='--',label='historical500s ROI target');ax[1,1].set(yscale='log',xticks=idx,xticklabels=['BandAmpV5','STM32','TPS63060'],ylabel='Estimated seconds (not full-ROI measurements)',title='Preflight only: no large ROI run');ax[1,1].legend(fontsize=8)
fig.savefig(OUT/'audit.png',dpi=150)
