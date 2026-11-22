"""Static numerical QA of bounded current reduction and explicit loss-band gap."""
from pathlib import Path
import json,sys,numpy as np,matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-final-integration'
if len(sys.argv)>1:OUT=Path(sys.argv[1]).resolve()
packet=json.loads((OUT/'automatic.json').read_text());audit=json.loads((OUT/'audit.json').read_text())
run=packet['runs'][0];check=audit['runs'][0]
fig,axes=plt.subplots(1,3,figsize=(15,4.5),layout='constrained')
z=run['fullZ']['Ok'];axis=np.array([r['frequencyHz'] for r in run['rows'] if r['drive']=='opposing'])
Z=np.array([[[complex(v['real'],v['imaginary']) for v in row] for row in sample] for sample in z['parameters']])
axes[0].loglog(axis,abs(Z[:,0,0]),'o-',label='full |Z11|')
nr=len(run['basis']);reduced=[]
for row in run['rows']:
 if row['drive']=='port0':
  b=np.array(row['recoveredState']);reduced.append(abs(complex(b[0]-b[1],b[12]-b[13])))
axes[0].loglog(axis,reduced,'x--',label='automatic |Z11|');axes[0].set(xlabel='Frequency (Hz)',ylabel='Impedance (Ohm)',title=f'Insulated foil coupon: 8 to {nr} currents');axes[0].legend()
for key,label in [('port','ports'),('current','all internal currents'),('localLoss','local losses')]:
 values=[max(r[key] for r in check['rows'] if r['frequencyHz']==f) for f in axis]
 axes[1].loglog(axis,np.maximum(values,1e-18),'o-',label=label)
axes[1].axhline(1e-6,color='red',ls='--',label='fixed reduction gate');axes[1].set(xlabel='Frequency (Hz)',ylabel='Normalized error',title='All unit-port drives; full/recovered');axes[1].legend(fontsize=8)
a=run['audit']
for key,label in [('port_errors','ports'),('state_errors','state'),('loss_errors','local loss')]:
 axes[2].semilogy(a['current_orders'],np.maximum(a[key],1e-18),'o-',label=label)
axes[2].axhline(1e-6,color='red',ls='--');axes[2].set(xlabel='Automatically selected current order',ylabel='Training error',title='Moment blocks; fixed admission gate');axes[2].legend()
for ax in axes:ax.grid(True,which='both',alpha=.3)
axes[1].text(.02,.02,'Exact zeros displayed at 1e-18',transform=axes[1].transAxes,fontsize=8)
axes[2].text(.02,.02,'Exact zeros displayed at 1e-18',transform=axes[2].transAxes,fontsize=8)
fig.savefig(OUT/'automatic-spectrum-recovery.png',dpi=160);plt.close(fig)

mu=4e-7*np.pi;sigma=58e6;t=35e-6
thin=1/(np.pi*mu*sigma*(3*t)**2);surface=1/(np.pi*mu*sigma*(t/10)**2)
coverage={'foilThicknessM':t,'conductivitySM':sigma,'thinDeltaThreeThicknessLimitHz':thin,'existingThinScreeningCapHz':1e5,
 'surfaceDeltaTenthThicknessMinimumHz':surface,'uncoveredHz':[min(thin,1e5),surface],
 'meaning':'Eligibility of existing accepted formulas, not proof of whole-ROI accuracy. No interpolation or new loss formula supplied.'}
(OUT/'band-coverage.json').write_text(json.dumps(coverage,indent=2))
fig,ax=plt.subplots(figsize=(9,3),layout='constrained');ax.set_xscale('log');ax.set_xlim(1,1e10);ax.set_ylim(-.5,2.5)
ax.plot([1,min(thin,1e5)],[2,2],lw=10,color='seagreen',label='thin-volume screening only')
ax.plot([surface,1e10],[0,0],lw=10,color='royalblue',label='high-frequency surface-loss eligibility')
ax.axvspan(min(thin,1e5),surface,color='tomato',alpha=.18,label='uncovered by accepted methods')
ax.set_yticks([0,2],['P7b surface','bounded volume']);ax.set(xlabel='Frequency (Hz)',title='Explicit 35 um copper coverage boundary; P7 remains open')
ax.grid(True,which='both',alpha=.25);ax.legend(loc='center right',fontsize=8);fig.savefig(OUT/'loss-band-gap.png',dpi=160);plt.close(fig)
