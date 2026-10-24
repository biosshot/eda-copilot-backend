"""Render two static figures from independently audited saved physical coupons."""
import json, sys
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
p=Path(sys.argv[1]);saved=json.loads(p.read_text(encoding='utf-8'))
audit=json.loads((p.parent/'audit.json').read_text(encoding='utf-8'));assert audit['status']=='pass'
reports=saved['result']['reports']
completed={c['result']['id']:c['result'] for r in reports for c in r['calculations'] if c['phase']=='completed'}
cases={l['data']['name']:completed[l['data']['resultId']]['solution'] for r in reports for l in r['logs'] if l['message']=='physical.case'}
fig,axs=plt.subplots(2,2,figsize=(13,9),layout='constrained')
fig.suptitle('Explicit physical contacts: both versus positive only\nAuthored full C/R/L blocks; intermodel mutual C/L is not extracted')
for col,name in enumerate(['both','single']):
 v=cases[name];t=[x*1e6 for x in v['axis']];ref=audit['cases'][name]['reference']
 for j,label in enumerate(['p','n1','n2'][:len(v['conductorPotentialsV'][0])]):
  axs[0,col].plot(t,[x[j] for x in v['conductorPotentialsV']],label=label)
  axs[0,col].plot(t,[x[j] for x in ref],'k:',alpha=.5)
 axs[0,col].set(title=name+' contact weld; dotted = independent reference',xlabel='Time [µs]',ylabel='Conductor potential [V]')
 for suffix in ['', ' refined']:
  u=cases[name+suffix];r=audit['cases'][name+suffix]['reference']
  axs[1,col].plot(t,[max(abs(a-b) for a,b in zip(x,y))*1e6 for x,y in zip(u['conductorPotentialsV'],r)],label=suffix.strip() or 'base')
 axs[1,col].set(title='Global reference error / timestep refinement',xlabel='Time [µs]',ylabel='Max node error [µV]')
for ax in axs.flat:ax.grid(alpha=.3);ax.legend()
fig.savefig(p.parent/'electrical-physical-contacts.png',dpi=150);plt.close(fig)
fig,axs=plt.subplots(2,2,figsize=(13,9),layout='constrained')
fig.suptitle('Physical common charge is stored energy, not a mathematical gauge')
for name in ['both','single']:
 v=cases[name];t=[x*1e6 for x in v['axis']]
 axs[0,0].plot(t,[sum(q)*1e9 for q in v['conductorChargesC']],label=name+' total Q')
 axs[0,1].plot(t,[u*1e9 for u in v['energyJ']],label=name+' U')
 c=cases[name+' common'];axs[1,0].plot(t,[sum(q)*1e9 for q in c['conductorChargesC']],label=name+' constant common Q')
 axs[1,1].plot(t,[u*1e9 for u in c['energyJ']],label=name+' constant common U')
axs[0,0].set(title='Conserved Q=15 nC',ylabel='Charge [nC]');axs[0,1].set(title='R/L/C decay on a charged rail',ylabel='Energy [nJ]')
axs[1,0].set(title='No hidden ground: Q=20 nC',ylabel='Charge [nC]');axs[1,1].set(title='Nonzero common energy U=20 nJ',ylabel='Energy [nJ]')
axs[0,0].set_ylim(14.99,15.01);axs[1,0].set_ylim(19.99,20.01);axs[1,1].set_ylim(19.99,20.01)
for ax in axs.flat:ax.set_xlabel('Time [µs]');ax.grid(alpha=.3);ax.legend();ax.ticklabel_format(axis='y',style='plain',useOffset=False)
fig.savefig(p.parent/'electrical-physical-charge-energy.png',dpi=150);plt.close(fig)
print('Rendered two PNGs from saved physical arrays and independent audit.')
