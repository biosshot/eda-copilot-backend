"""Static scientific figures of saved native responses and independent audit."""
from pathlib import Path
import json,numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p6-p7-structural-gauges'
v=json.loads((OUT/'coupon.json').read_text());audit=json.loads((OUT/'audit.json').read_text());r=v['runs'][0];a=audit['runs'][0];f=np.array([s['frequencyHz'] for s in r['sweep']])/1e3
native=np.array([complex(*s['loopZOhm']) for s in r['sweep']]);ref=np.array([complex(*s['loopZOhm']) for s in a['references']]);reduced=np.array([complex(*s['reducedLoopZOhm']) for s in r['sweep']])
fig,axes=plt.subplots(2,1,figsize=(10,7),layout='constrained');fig.suptitle('Insulated two-foil coupon: all original 17 frequencies admitted\nIndependent descriptor with explicit common reference constraint')
axes[0].semilogx(f,abs(ref)*1e3,'k--',label='Independent reference');axes[0].semilogx(f,abs(native)*1e3,'o',label='Full');axes[0].semilogx(f,abs(reduced)*1e3,'+',label='Reduced');axes[0].set(ylabel='Loop |Z| [mOhm]');axes[0].legend()
for key,label in [('loopZ','Full loop Z'),('reducedLoopZ','Reduced loop Z'),('longitudinalZ','Physical two-port Z'),('crossFoilZ','Original cross-foil Z')]:axes[1].semilogx(f,[100*row[key] for row in a['rows']],'o-',label=label)
axes[1].axhline(1,color='crimson',ls=':',label='Unchanged 1% gate');axes[1].set(xlabel='Frequency [kHz]',ylabel='Independent error [%]');axes[1].legend(ncol=2)
for ax in axes:ax.grid(alpha=.3)
fig.savefig(OUT/'spectrum-reference-error.png',dpi=150);plt.close(fig)
fig,axes=plt.subplots(2,1,figsize=(10,7),layout='constrained');fig.suptitle('Full / reduced recovery in original physical coordinates')
for key,label in [('fullProjectedPotential','Potential'),('fullProjectedCurrent','Current'),('reducedLoopZ','Reduced / reference loop Z')]:axes[0].semilogx(f,[100*row[key] for row in a['rows']],'o-',label=label)
axes[0].set(ylabel='Normalized error [%]');axes[0].legend()
for k in range(4):axes[1].semilogx(f,[s['potentialsRealV'][k]*1e3 for s in r['sweep']],label='v'+str(k)+' full')
axes[1].set(xlabel='Frequency [kHz]',ylabel='Real potential [mV]');axes[1].legend(ncol=2)
for ax in axes:ax.grid(alpha=.3)
fig.savefig(OUT/'full-projected-recovery.png',dpi=150);plt.close(fig)
fig,axes=plt.subplots(2,1,figsize=(10,7),layout='constrained');fig.suptitle('Recovered physical foil currents and complete local losses')
for k in [0,2,4,6]:axes[0].semilogx(f,[s['currentsRealA'][k] for s in r['sweep']],'o-',label='Current DOF '+str(k))
axes[0].set(ylabel='Real current [A]');axes[0].legend(ncol=2)
for k in range(4):axes[1].semilogx(f,[s['cellLossesWPeakNorm'][k]*1e3 for s in r['sweep']],'o-',label='Triangle '+str(k))
axes[1].semilogx(f,[s['jouleLossWPeakNorm']*1e3 for s in r['sweep']],'k--',label='Total full loss');axes[1].semilogx(f,[s['lossW']*1e3 for s in a['references']],'+',label='Independent total')
axes[1].set(xlabel='Frequency [kHz]',ylabel='Loss, peak norm [mW]');axes[1].legend(ncol=2)
for ax in axes:ax.grid(alpha=.3)
fig.savefig(OUT/'physical-currents-losses.png',dpi=150);plt.close(fig)
fig,axes=plt.subplots(1,2,figsize=(11,5),layout='constrained');fig.suptitle('Real exterior capacitance stays physical, with no structural certificate')
ext=np.array([s['exteriorCapacitanceF'] for s in v['exteriorControls']]);energy=np.array([s['energyJ'] for s in v['exteriorControls']]);axes[0].loglog(ext,9*ext,'k--',label='Independent 9 Cext');axes[0].loglog(ext,energy,'o',label='Native retained common charge');axes[0].set(xlabel='Exterior C per conductor [F]',ylabel='Stored energy at common 3 V [J]');axes[0].legend()
for s in v['exteriorControls']:
 z=np.array([complex(sample[0][0]['real'],sample[0][0]['imaginary']) for sample in s['frequency']['Ok']['parameters']]);axes[1].loglog(f,abs(z),label='Cext %.0e F'%s['exteriorCapacitanceF'])
axes[1].set(xlabel='Frequency [kHz]',ylabel='Physical differential |Z| [Ohm]');axes[1].legend()
for ax in axes:ax.grid(alpha=.3)
fig.savefig(OUT/'real-exterior-capacitance.png',dpi=150);plt.close(fig)
print('Rendered 4 saved-value figures; no solver or expensive field reference rerun')
