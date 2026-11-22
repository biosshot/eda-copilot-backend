"""Plot completed numerical response and the separately extracted full coupon."""
import json,sys
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
folder=Path(sys.argv[1]);data=json.loads((folder/'result.json').read_text())
solutions=[c['result']['solution'] for r in data['reports'] for c in r['calculations'] if c['phase']=='completed'];s=solutions[-1]
comparison=json.loads((folder/'whole-reference-comparison.json').read_text());rows=comparison['rows'];f=[r['frequencyHz'] for r in rows]
fig,ax=plt.subplots(2,2,figsize=(12,7),layout='constrained')
ax[0,0].plot(f,[r['composedDelayS']*1e12 for r in rows],'o-',label='Corridor + local FEM');ax[0,0].plot(f,[r['fullDelayS']*1e12 for r in rows],'s--',label='Saved whole-volume reference');ax[0,0].set(ylabel='Phase delay (ps)',title='Actual '+json.loads((folder/'identity.json').read_text()).get('variant','straight')+' finite-return coupon');ax[0,0].legend()
ax[0,1].plot(f,[r['composedCuLossW']*1e3 for r in rows],'o-',label='Composed');ax[0,1].plot(f,[r['fullCuLossW']*1e3 for r in rows],'s--',label='Whole-volume');ax[0,1].set(ylabel='Copper loss (mW)',title='Original passive power balance');ax[0,1].legend();ax[0,1].ticklabel_format(axis='y',useOffset=False)
ax[1,0].bar(['Complex S','Phase delay','Cu loss'],[comparison['maxRelativeComplexSError']*100,comparison['maxRelativePhaseDelayError']*100,comparison['maxRelativeCuLossError']*100]);ax[1,0].axhline(8,color='red',linestyle='--',label='8% target');ax[1,0].set(ylabel='Difference (%)',title='Reference uncertainty remains separate');ax[1,0].legend()
checks=[];labels=[]
for v in s['verification']:
 labels.append('mesh d'+str(v['domainScale']));checks.append(v['spatial']['maxRelativeChange']*100)
 for k in ('domain','boundary'):
  if k in v:labels.append(k);checks.append(v[k]['maxRelativeChange']*100)
ax[1,1].bar(labels,checks);ax[1,1].axhline(2,color='red',linestyle='--',label='unchanged 2% gate');ax[1,1].set(ylabel='Maximum response change (%)',title='Spatial/domain/boundary sensitivity');ax[1,1].legend()
for a in ax[0]:a.set_xscale('log');a.set_xlabel('Frequency (Hz)');a.grid(alpha=.2)
fig.suptitle('Completed low-band coupon output — no STM32/all-DSL certificate',fontweight='bold');fig.savefig(folder/'corridor-response.png',dpi=160)
print(folder/'corridor-response.png')
