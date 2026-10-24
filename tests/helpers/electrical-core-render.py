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

for filename in sys.argv[1:]:
    path=Path(filename)
    saved=json.loads(path.read_text(encoding='utf-8'))
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
