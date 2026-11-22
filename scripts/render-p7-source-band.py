"""Plot the direct TPS compatibility scenario and its absolute HF-tail bound."""
from pathlib import Path
import json,numpy as np
import matplotlib;matplotlib.use('Agg')
import matplotlib.pyplot as plt
root=Path('debugging/p7-channel-plan');folder=sorted(root.glob('tps-source-spectrum-*'))[-1];r=json.loads((folder/'report.json').read_text());solution=next(c['result']['solution'] for report in r['reports'] for c in report['calculations'] if c['phase']=='completed');e=solution['sourceSpectrumEvidence'];plan=e['drivers'][0];proof=e['outputTailBounds'];assert proof['status']=='conditional-passivity-bound'
t=np.array(solution['axis']);v=np.array(solution['portVoltages']);f=np.geomspace(1000,1e7,400);tail=proof['outputs'][0]['absoluteDifferenceBoundV']*proof['upperHz']/f
fig,ax=plt.subplots(1,2,figsize=(14,6.5));fig.subplots_adjust(top=.8,bottom=.23,left=.07,right=.97,wspace=.3)
ax[0].plot(t*1000,np.minimum(t/2e-4,1),color='#64748b',label='Source [mV]');ax[0].plot(t*1000,v[:,1]*1000,color='#0369a1',label='Output [mV]');ax[0].axhline(.5,color='#4d7c0f',ls=':',label='Loaded steady check0.5mV');ax[0].set(xlabel='Time [ms]',ylabel='Voltage [mV]',title='Supplied actual TPS finite-ROI RLC');ax[0].legend(fontsize=9);ax[0].grid(alpha=.2)
ax[1].loglog(f,tail*1e6,color='#b91c1c',label='Passive difference HF-tail bound');ax[1].axvline(proof['upperHz'],color='#334155',label='Declared upper band100kHz');ax[1].axvline(plan['requiredUpperHz'],ls='--',color='#16a34a',label='1% input-tail requirement50.66kHz');ax[1].axhline(25,color='#7e22ce',ls=':',label='Scenario check tolerance25µV');ax[1].scatter([proof['upperHz']],[proof['outputs'][0]['absoluteDifferenceBoundV']*1e6],color='#b91c1c');ax[1].set(xlabel='Frequency cutoff [Hz]',ylabel='Absolute omitted-tail bound [µV]',title='Finite-R source/load, zero-energy start');ax[1].legend(fontsize=8);ax[1].grid(alpha=.2,which='both')
fig.suptitle('TPS: finite200µs ramp and quantitative source-band evidence\nExplicit developer DC-extension hypothesis; conditional compatibility result',fontsize=15)
fig.text(.07,.14,f"At100kHz, input-tail and passive output-difference bound: {proof['outputs'][0]['absoluteDifferenceBoundV']*1e6:.3f} µV. DC coverage is separately required.",fontsize=10)
fig.text(.07,.095,'Passivity bound assumes the actual and supplied networks are passive with the declared finite source/load resistances and corresponding zero-energy starts.',fontsize=9)
fig.text(.07,.05,'In-band physical extraction, spatial validity and global time error remain separate. This plot does not grant a5% physical transient certificate.',fontsize=9)
fig.savefig(folder/'source-band-response.png',dpi=150);plt.close(fig);print(folder/'source-band-response.png')
