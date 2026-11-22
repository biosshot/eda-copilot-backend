"""Render actual saved P6 arrays. Static PNG, no browser/interactive QA claim."""
import json
import math
import sys
from pathlib import Path
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

def sampled_reference(t):
    dc, high, tau, rise = 1/1.1, 1/1.6, 1.6/(2*math.pi*73*1.1), 1e-4
    if t <= rise:
        return high*t/rise+(dc-high)*(t-tau*(1-math.exp(-t/tau)))/rise
    return dc-(dc-high)*tau/rise*(1-math.exp(-rise/tau))*math.exp(-(t-rise)/tau)

def render_mixed(path, saved):
    reports=saved['result']['reports']
    completed={c['result']['id']:c['result'] for r in reports for c in r['calculations'] if c['phase']=='completed'}
    cases={l['data']['name']:completed[l['data']['resultId']]['solution'] for r in reports for l in r['logs'] if l['message']=='mixed.case'}
    audit=json.loads((path.parent/'audit.json').read_text(encoding='utf-8'))
    fig,axs=plt.subplots(4,2,figsize=(16,17),layout='constrained')
    fig.suptitle(f"{saved['metadata']['fixture']} - authored mixed delay + RLC\nBoard contacts are provenance; no trace extraction. Static saved-number QA.",fontsize=16)
    for ax,name in zip(axs.flat,['rc','rlc','reflections']):
        v=cases[name];fine=cases[name+' refined'];t=[x*1e9 for x in v['axis']]
        ax.plot(t,[r[0] for r in v['portVoltages']],label='input')
        ax.plot(t,[r[1] for r in v['portVoltages']],label='output')
        ax.plot(t,audit['cases'][name]['reference'],'k--',label='independent analytic reference')
        ax.set(title=name+' finite-ramp response',xlabel='Time [ns]',ylabel='Voltage [V]')
    ax=axs[1,1]
    for name in ['rc','rlc','reflections']:
        v=cases[name];ax.plot([x*1e9 for x in v['axis']],[r[0]*1e3 for r in v['driverCurrentsA']],label=name+' driver')
        ax.plot([x*1e9 for x in v['axis']],[r[1]*1e3 for r in v['portCurrentsA']],linestyle=':',label=name+' output inward')
    ax.set(title='Physical current signs: into network',xlabel='Time [ns]',ylabel='Current [mA]')
    v=cases['rlc'];t=[x*1e9 for x in v['axis']]
    for key,label in [('lumpedEnergyJ','C/L storage'),('delayEnergyJ','delay storage')]:axs[2,0].plot(t,[x*1e12 for x in v[key]],label=label)
    axs[2,0].set(title='Separate physical stored energy (RLC coupon)',xlabel='Time [ns]',ylabel='Energy [pJ]')
    for key,label in [('sourceWorkJ','source work'),('resistiveLossJ','R/driver/load loss'),('attenuationLossJ','delay attenuation'),('beNumericalDissipationJ','BE numerical')]:axs[2,1].plot(t,[x*1e12 for x in v[key]],label=label)
    axs[2,1].set(title='Independent work/loss ledger',xlabel='Time [ns]',ylabel='Energy [pJ]')
    for name in ['rc','rlc','reflections']:
        x=[x*1e9 for x in cases[name]['axis']];a=audit['cases'][name+' refined']
        axs[3,0].plot(x,[v*1e3 for v in a['error']],label=name+' reference error')
        axs[3,0].plot(x,[v*1e3 for v in a['refinement']],linestyle=':',label=name+' global refinement')
    axs[3,0].axhline(1.001,color='k',linestyle='--',label='reference bound 1.001mV');axs[3,0].axhline(.5,color='gray',linestyle='--',label='refinement bound .5mV')
    axs[3,0].set(title='Independent reference and time/history refinement',xlabel='Time [ns]',ylabel='Absolute error [mV]')
    names=['rc','rlc','reflections'];ax=axs[3,1]
    for offset,suffix,label in [(-.18,'','coarse'),(.18,' refined','refined')]:ax.bar([i+offset for i in range(3)],[audit['cases'][n+suffix]['normalizedEnergyBalance'] for n in names],width=.35,label=label)
    ax.axhline(1e-4,color='k',linestyle='--',label='fixed physical balance bound');ax.set_xticks(range(3),names);ax.set(title='Physical balance after separate BE dissipation',ylabel='Absolute balance / fixed energy norm')
    for ax in axs.flat:ax.grid(alpha=.25);ax.legend(fontsize=8)
    fig.savefig(path.parent/'electrical-mixed.png',dpi=150);plt.close(fig)
    fig,axes=plt.subplots(3,2,figsize=(15,12),layout='constrained')
    fig.suptitle(f"{saved['metadata']['fixture']} - all mixed two-port transmissions/reflections\nActual saved complex S at21 frequencies; independently audited against ABCD")
    for row,name in enumerate(['rc','rlc','reflections']):
        v=cases[name+' frequency'];frequencies=[f/1e6 for f in v['frequenciesHz']]
        for out in range(2):
            for incoming in range(2):
                z=[complex(m[out][incoming]['real'],m[out][incoming]['imaginary']) for m in v['parameters']]
                label=f'S{out+1}{incoming+1}'
                axes[row,0].plot(frequencies,[abs(x) for x in z],label=label)
                axes[row,1].plot(frequencies,[math.degrees(math.atan2(x.imag,x.real)) for x in z],label=label)
        axes[row,0].set(title=name+' amplitudes',xlabel='Frequency [MHz]',ylabel='S magnitude [1]')
        axes[row,1].set(title=name+' phases',xlabel='Frequency [MHz]',ylabel='Wrapped phase [deg]')
    for ax in axes.flat:ax.grid(alpha=.25);ax.legend()
    fig.savefig(path.parent/'electrical-mixed-frequency.png',dpi=150);plt.close(fig)

for filename in sys.argv[1:]:
    path=Path(filename)
    saved=json.loads(path.read_text(encoding='utf-8'))
    if any(l['message']=='mixed.case' for r in saved['result']['reports'] for l in r['logs']):
        render_mixed(path,saved)
        continue
    reports=saved['result']['reports']
    completed={c['result']['id']:c['result'] for r in reports for c in r['calculations'] if c['phase']=='completed'}
    cases={l['data']['name']:completed[l['data']['resultId']]['solution'] for r in reports for l in r['logs'] if l['message']=='core.case'}
    fig,axs=plt.subplots(4,2,figsize=(15,16),layout='constrained')
    fig.suptitle(f"{saved['metadata']['fixture']} — explicit authored electrical models\nBoard provenance only; no PCB extraction. Sampled-to-time results are conditional.",fontsize=17)
    ac=cases['ac']; f=ac['axis']
    for node,label in [('in','input'),('out','output')]:
        axs[0,0].semilogx(f,[math.hypot(v['real'],v['imaginary']) for v in ac['nodeVoltages'][node]],label=label)
    axs[0,0].set(title='Loaded circuit AC amplitudes',xlabel='Frequency [Hz]',ylabel='Peak phasor magnitude [V]')
    transfer=[complex(v['real'],v['imaginary'])/complex(w['real'],w['imaginary']) for v,w in zip(ac['nodeVoltages']['out'],ac['nodeVoltages']['in'])]
    axs[0,1].semilogx(f,[abs(v) for v in transfer],label='|Vout / Vin|')
    phase=axs[0,1].twinx();phase.semilogx(f,[math.degrees(math.atan2(v.imag,v.real)) for v in transfer],color='tab:orange',label='phase');phase.set_ylabel('Phase [deg]',color='tab:orange')
    axs[0,1].set(title='Own circuit transfer (not S21)',xlabel='Frequency [Hz]',ylabel='Ratio [1]')
    coarse,fine=cases['sampled'],cases['sampled refined'];t=fine['axis'];vf=[r[0] for r in fine['portVoltages']];vc=[r[0] for r in coarse['portVoltages']]
    ref=[sampled_reference(at) for at in t]
    axs[1,0].plot([at*1e3 for at in t],ref,'k--',label='independent RC ramp')
    axs[1,0].plot([at*1e3 for at in t],vc,label='fit + BE coarse',alpha=.75)
    axs[1,0].plot([at*1e3 for at in t],vf,label='fit + BE refined',linewidth=1)
    axs[1,0].set(title='Imported sampled nonconstant RC → time',xlabel='Time [ms]',ylabel='Voltage [V]',xlim=(0,8))
    axs[1,1].plot([at*1e3 for at in t],[abs(a-b)*1e3 for a,b in zip(vf,ref)],label='refined vs independent reference')
    axs[1,1].plot([at*1e3 for at in t],[abs(a-b)*1e3 for a,b in zip(vf,vc)],label='separate time refinement')
    axs[1,1].axhline(1.001,color='k',linestyle=':',label='reference bound 1.001 mV')
    axs[1,1].set(title='Model/time errors are separate',xlabel='Time [ms]',ylabel='Absolute difference [mV]',xlim=(0,8))
    for ax,name in zip(axs[2],['matched','reflected']):
        s=cases[name]
        for i,label in enumerate(['input','output']):ax.plot([at*1e9 for at in s['axis']],[row[i] for row in s['portVoltages']],label=label)
        for at in [2,6,10]:ax.axvline(at,color='gray',linestyle=':',alpha=.4)
        ax.set(title=f'{name}: exact 2 ns wave delay, explicit loads',xlabel='Time [ns]',ylabel='Voltage [V]')
    s=cases['cascade frequency']
    for i in range(2):
        for j in range(2):axs[3,0].plot(s['frequenciesHz'],[math.hypot(m[i][j]['real'],m[i][j]['imaginary']) for m in s['parameters']],linestyle=['-',':','--','-.'][2*i+j],label=f'S{i+1}{j+1}')
    axs[3,0].set(title='Passive pad + delay cascade: all four coefficients',xlabel='Frequency [Hz]',ylabel='Power-wave magnitude [1]',ylim=(-.05,.65))
    last=fine['fit']['refinement'][-1]
    text=(f"Saved arrays and source/addon hashes retained\n\n"
          f"Fit train max: {last['trainingMaximum']:.3g}\nFit holdout max: {last['holdoutMaximum']:.3g}\nModel-order change: {last['orderMaximumChange']:.3g}\n"
          f"Time pair max: {max(abs(a-b) for a,b in zip(vf,vc)):.3g} V\n"
          f"Wave energy residual: {cases['reflected']['quality']['energyBalanceJ']:.3g} J\n\n"
          "Conditional between/extraband samples:\nunseen resonance is not excluded.\n\n"
          "Static PNG inspected separately.\nInteractive HTML controls were not tested.")
    axs[3,1].axis('off');axs[3,1].text(.03,.95,text,va='top',fontsize=12)
    for ax in axs.flat:
        if ax.axison:ax.grid(True,alpha=.25);ax.legend(fontsize=9)
    out=path.with_name('verification.png');fig.savefig(out,dpi=150);plt.close(fig)
    print(out.resolve())
