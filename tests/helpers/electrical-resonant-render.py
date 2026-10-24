"""Static scientific figures from saved production arrays and analytic references."""
import json,pathlib,sys
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
p=pathlib.Path(sys.argv[1] if len(sys.argv)>1 else 'debugging/p6-resonant-runtime/direct')
r=json.loads((p/'result.json').read_text())['result'];ref=json.loads((p/'reference.json').read_text())
done=[c['result'] for report in r['reports'] for c in report['calculations'] if c['phase']=='completed']
coarse,fine,ac=[x['solution'] for x in done];fit=fine['fit']['sourceFits'][0]
def complex_matrix(rows):return np.array([[[complex(z['real'],z['imaginary']) for z in row] for row in mat] for mat in rows])
f=np.array(ac['frequenciesHz']);ya=complex_matrix(ac['parameters']);truth=np.array(ref['admittanceS']);truth=truth[...,0]+1j*truth[...,1]
xf=np.array(fit['frequenciesHz'])[1:];yf=complex_matrix(fit['fittedAdmittanceS'])[1:]
fig,axes=plt.subplots(2,2,figsize=(11,7))
for i in range(2):
    for j in range(2):
        ax=axes[i,j]
        ax.semilogx(f,truth[:,i,j].real,label='physical analytic Re');ax.semilogx(f,truth[:,i,j].imag,label='physical analytic Im')
        ax.semilogx(f,ya[:,i,j].real,'o',ms=2,label='public AC Re');ax.semilogx(f,ya[:,i,j].imag,'+',ms=3,label='public AC Im')
        ax.semilogx(xf,yf[:,i,j].real,'--',lw=.8,label='native RLC fit Re');ax.semilogx(xf,yf[:,i,j].imag,':',lw=.8,label='native RLC fit Im')
        ax.set(title=f'Y{i+1}{j+1}',xlabel='Hz',ylabel='S');ax.grid();ax.legend(fontsize=6)
fig.tight_layout();fig.savefig(p/'ac-all-entries.png',dpi=150);plt.close(fig)
t=np.array(fine['axis'])*1e6;v=np.array(fine['portVoltages']);current=np.array(fine['driverCurrentsA'])
vr=np.array([a['voltage'] for a in ref['time']]);ir=np.array([a['current'] for a in ref['time']]);er=np.array([a['energy'] for a in ref['time']])
fig,axes=plt.subplots(2,2,figsize=(11,7))
for i in range(2):
    axes[0,i].plot(t,vr[:,i],label='analytic');axes[0,i].plot(t,v[:,i],'--',label='production BE');axes[0,i].set(ylabel='V',title=f'port {i+1} voltage')
    axes[1,i].plot(t,ir[:,i],label='analytic');axes[1,i].plot(t,current[:,i],'--',label='production BE');axes[1,i].plot(t,np.array(coarse['driverCurrentsA'])[:,i],':',label='coarser BE');axes[1,i].set(ylabel='A',title=f'port {i+1} inward current')
for ax in axes.flat:ax.set(xlabel='microseconds',xlim=(0,2));ax.grid();ax.legend(fontsize=8)
fig.tight_layout();fig.savefig(p/'transient.png',dpi=150);plt.close(fig)
fig,axes=plt.subplots(3,1,figsize=(10,9));E=np.array(fine['energyJ']);q=fine['quality']
axes[0].plot(t,er,label='analytic state energy');axes[0].plot(t,E,'--',label='production storage');axes[0].set(ylabel='J',xlim=(0,2));axes[0].legend()
axes[1].plot(t,E-er);axes[1].set(ylabel='storage error (J)',xlabel='microseconds')
keys=['workJ','dissipatedJ','storedEnergyChangeJ','beNumericalDissipationJ','rawEnergyBalanceJ','energyBalanceJ']
axes[2].bar(range(len(keys)),[q[k] for k in keys]);axes[2].set_xticks(range(len(keys)),['work','R loss','change E','BE loss','raw balance','gated defect']);axes[2].set(ylabel='J',title=f"Final ledger; raw={q['rawEnergyBalanceJ']:.4e} J; gated={q['energyBalanceJ']:.4e} J")
for ax in axes:ax.grid(axis='y')
fig.tight_layout();fig.savefig(p/'energy.png',dpi=150);plt.close(fig)
print('Saved AC all entries, transient and energy PNGs from saved evidence')
