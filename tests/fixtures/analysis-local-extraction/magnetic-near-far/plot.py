"""Saved-value numerical error/spectra/cost plots; no new solve."""
from pathlib import Path
import json
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-near-far'
a=json.loads((OUT/'audit.json').read_text());p=a['policies'];theta=[str(x['theta']) for x in p]
fig,axes=plt.subplots(2,2,figsize=(13,8),constrained_layout=True)
ax=axes[0,0];x=np.arange(3)
ax.bar(x-.2,[100*t['fullScreen'] for t in p],.4,label='full error + empirical reference uncertainty')
ax.bar(x+.2,[100*t['aggregateFarBudget'] for t in p],.4,label='analytic far bound / reference energy')
ax.axhline(5,color='r',ls='--',label='full gate5%');ax.axhline(1,color='k',ls=':',label='far budget1%')
ax.set_xticks(x,theta);ax.set_xlabel('fixed theta');ax.set_ylabel('%');ax.legend(fontsize=8);ax.set_title('Coarse far budget fails; full operator screens pass')
ax=axes[0,1]
for row in p:ax.plot(range(1,5),np.array(row['physicalEigenvaluesH'])*1e9,'o-',label='theta '+str(row['theta']))
ax.plot(range(1,5),np.array(a['reference']['eigenvaluesH'])*1e9,'kx',label='independent reference')
ax.set_xlabel('physical eigenmode');ax.set_ylabel('nH');ax.set_title('All four physical energy modes, no clipping');ax.legend(fontsize=8)
ax=axes[1,0]
for name in ['BandAmpV5','STM32G0_CAN_RS485','TPS63060DSCR']:
 rows=[r for r in a['rois'] if r['roi']==name+'-supports'];ax.plot([r['theta'] for r in rows],[r['bothDirectionsNearPlusFarSeconds'] for r in rows],'o-',label=name)
ax.axhline(500,color='r',ls='--',label='historical500s target');ax.set_yscale('log');ax.set_xlabel('fixed theta');ax.set_ylabel('extrapolated seconds');ax.set_title('ROI cost blocker remains (NO ROI solve)');ax.legend(fontsize=8)
ax=axes[1,1]
for name in ['BandAmpV5','STM32G0_CAN_RS485','TPS63060DSCR']:
 rows=[r for r in a['rois'] if r['roi']==name+'-supports'];ax.plot([r['theta'] for r in rows],[100*r['farPairs']/(r['nearPairs']+r['farPairs']) for r in rows],'o-',label=name)
ax.set_xlabel('fixed theta');ax.set_ylabel('far fraction, %');ax.set_title('Full geometry retained; hierarchical plan only');ax.legend(fontsize=8)
fig.suptitle('P7 bounded magnetic near/far diagnostic | CPU F64 | full ROI not admitted')
fig.savefig(OUT/'audit.png',dpi=160)
