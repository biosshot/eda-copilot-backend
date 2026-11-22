"""Plot saved closed-current magnetic fields/operator/gates; no solves."""
from pathlib import Path
import json,numpy as np,matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent;OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-stability/closed';a=json.loads((OUT/'audit.json').read_text());case=json.loads((FIX/'closed-coupon.json').read_text());partial=json.loads((OUT.parent/'audit.json').read_text());last=a['levels'][-1]
fig,ax=plt.subplots(2,3,figsize=(15,9),layout='constrained');fig.suptitle(f"Closed finite current coupon — entire2D physical magnetic operator\nFull energy error {100*last['fullPositiveRangeEnergyError']:.5f}% + separate working reference uncertainty {100*a['referenceWorkingUncertainty']:.5f}%: screen {100*last['referenceMarginScreen']:.5f}% <5%",fontsize=13)
for basis in range(2):
 for cell in case['cells']:
  t=np.array(cell['triangleM'])*1e3;loop=np.vstack([t,t[0]]);ax[0,basis].plot(loop[:,0],loop[:,1],color='#bbb',lw=.6)
 centers=np.array([np.mean(c['triangleM'],axis=0) for c in case['cells']])*1e3;j=np.array([c['densityPerAmpM2'][basis] for c in case['cells']]);scale=max(np.linalg.norm(j,axis=1));ax[0,basis].quiver(centers[:,0],centers[:,1],j[:,0]/scale,j[:,1]/scale,angles='xy',scale_units='xy',scale=5,color=['#457b9d','#e79435'][basis]);ax[0,basis].set(aspect='equal',xlabel='x (mm)',ylabel='y (mm)',title=f'Closed basis{basis+1}: J directions,35µm finite foil');ax[0,basis].text(.02,.98,'Interior normal jumps/exterior flux <1e−12\nArrows normalized to max |J|',transform=ax[0,basis].transAxes,va='top',fontsize=8)
matrix=np.array(last['rawLH'])*1e9;im=ax[0,2].imshow(matrix,cmap='coolwarm');fig.colorbar(im,ax=ax[0,2],label='nH');ax[0,2].set(xticks=range(2),yticks=range(2),xlabel='Closed current basis',ylabel='Closed current basis',title='Full L: all self and signed mutual terms')
for i in range(2):
 for j in range(2):ax[0,2].text(j,i,f'{matrix[i,j]:.7f}',ha='center',va='center')
idx=np.arange(2);ax[1,0].bar(idx-.15,np.array(last['eigenvaluesH'])*1e9,width=.3,label='native');ax[1,0].bar(idx+.15,np.array(last['referenceEigenvaluesH'])*1e9,width=.3,label='independent reference');ax[1,0].set(xticks=idx,xticklabels=['physical mode1','physical mode2'],ylabel='Eigenvalue (nH)',title='Entire physical-space spectrum positive');ax[1,0].legend(fontsize=8)
orders=[v['order'] for v in a['levels']];reciprocity=[v['reciprocity'] for v in a['levels']];ax[1,1].semilogy(orders,reciprocity,'o-',color='#457b9d');ax[1,1].axhline(1e-9,color='firebrick',ls='--',label='unchanged raw1e−9 gate');ax[1,1].set(xticks=orders,xlabel='Native volume Gauss order',ylabel='Raw reciprocity ratio',title='One justified order24 control; failures retained');ax[1,1].annotate('PASS',xy=(24,reciprocity[-1]),xytext=(-30,15),textcoords='offset points');ax[1,1].legend(fontsize=8);ax[1,1].grid(alpha=.2)
roi=partial['roiCostPreflight'];idx=np.arange(3);ax[1,2].bar(idx-.16,[v['oneDirectionCoarseSecondsEstimate'] for v in roi],width=.32,label='coarse one-direction estimate');ax[1,2].bar(idx+.16,[v['oneDirectionRefinedSecondsEstimate'] for v in roi],width=.32,label='refined one-direction estimate');ax[1,2].axhline(500,color='firebrick',ls='--',label='historical500s target');ax[1,2].set(yscale='log',xticks=idx,xticklabels=['BandAmpV5','STM32','TPS63060'],ylabel='Estimated seconds; no full-ROI run',title='Production dense cost remains blocked');ax[1,2].legend(fontsize=8)
fig.savefig(OUT/'audit.png',dpi=150)
