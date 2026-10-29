"""Render only captured native/DSL results. No solver or synthetic current map."""
from pathlib import Path
import json,sys,textwrap
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.colors import LogNorm
from matplotlib.patches import Rectangle
root=Path(__file__).resolve().parents[4];base=root/'debugging/p6-p7-integration'/sys.argv[1]
names=sys.argv[2:] or ['STM32G0_CAN_RS485','TPS63060DSCR','BandAmpV5']
summary={}
for name in names:
 folder=base/name;run=json.loads((folder/'cold/run.json').read_text());result=json.loads((folder/'cold/result.json').read_text());assert result['status']=='complete';assert not run.get('inputsChangedDuringRun',False)
 reports=result['reports'];plots=[p for r in reports for p in r['plots']];checks=[c for r in reports for c in r['checks']]
 plot=lambda title:next(p['data'] for p in plots if p['title'].startswith(title))
 restart=json.loads((folder/'restart/run.json').read_text()) if (folder/'restart/run.json').exists() else None
 fig,axes=plt.subplots(2,2,figsize=(13,9),layout='constrained');ax=axes.ravel()
 if name!='BandAmpV5':
  data=plot('Selected ROI')['data'];warm=plot('Same-worker')['data'];after=plot('Check-only')['data'];solution=plot('Cold stage')
  points=np.array(data['nodesM'])*1000;points[:,:2]+=np.array(data['originMm']);cells=np.array(data['copperCells']);centers=points[cells].mean(axis=1);loss=np.array(data['samples'][0]['cellLossW']);assert len(loss)==len(cells)
  positive=loss>0
  if positive.any():
   values=loss[positive];scatter=ax[0].scatter(centers[positive,0],centers[positive,1],c=values,s=8,norm=LogNorm(vmin=values.min(),vmax=max(values.max(),values.min()*1.01)),cmap='inferno');fig.colorbar(scatter,ax=ax[0],label='mean Joule W / tetrahedron (RMS drive)')
  for e,nodes in zip(data['request']['electrodes'],data['electrodeNodes']):
   xyz=points[nodes];ax[0].scatter(xyz[:,0],xyz[:,1],s=10,marker='x',label=e['name'])
  ax[0].set(xlabel='board x (mm)',ylabel='board y (mm)',title='Actual finite Cu: all-z projection; recovered cell losses');ax[0].invert_yaxis();ax[0].set_aspect('equal');ax[0].legend(fontsize=8)
  for title,label in [('Input impedance','Z input'),('Transfer impedance','Z transfer')]:
   curve=plot(title);ax[1].loglog(curve['axis']['values'],curve['values'],label=label)
  ax[1].set(xlabel='Hz',ylabel='Ohm',title='Exterior charge retained; direct port Z');ax[1].legend()
  order=np.argsort(loss)[::-1];ax[2].semilogy(np.arange(len(loss)),np.maximum(loss[order],1e-30));ax[2].set(xlabel='tetrahedra sorted by loss',ylabel='mean Joule W / tetrahedron',title=f"10 kHz total copper loss = {loss.sum():.6g} W")
  labels=['family cold','same-worker warm','after plot/check'];times=[run['wallSeconds'],warm['wallSeconds'],after['wallSeconds']]
  if restart:labels+=['fresh-process restore'];times+=[restart['wallSeconds']]
  ax[3].bar(labels,times);ax[3].set_yscale('log');ax[3].set(ylabel='measured seconds (log)',title='Measured model reuse; observed timings');ax[3].tick_params(axis='x',labelrotation=15)
  for i,t in enumerate(times):ax[3].text(i,t,f'{t:.4g}',ha='center',va='bottom',fontsize=9)
  summary[name]=dict(cold=run,warmCache=warm['cache'],warmSolve=warm.get('solveStageCounts'),warmSeconds=warm['wallSeconds'],afterCheck=after['cache'],nativeColdSeconds=solution['prepared']['candidate']['coldPrepareSeconds'],restart=restart,totalLossW=float(loss.sum()),checks=checks,localVolumeStatus='complete-conditional')
 else:
  cold=plot('Supported section cold');warm=plot('Supported section warm');curve=plot('Supported P7b RF section S21');rlgc=plot('RF section supported')['data'];failure=plot('Local ground channel refused')
  board=json.loads((root/'tests/pcb-analyze'/name/(name+'.json')).read_text());track=next(t for t in board['copper']['fixed']['tracks'] if t['id']=='dcc17162579803b7');xy=np.array([[p['x'],p['y']] for p in track['points']]);ax[0].plot(xy[:,0],xy[:,1],lw=6,label='captured RF_IN_PORT straight track')
  polygon=failure['roi']['outer'];xs=[p['x'] for p in polygon];ys=[p['y'] for p in polygon];ax[0].add_patch(Rectangle((min(xs),min(ys)),max(xs)-min(xs),max(ys)-min(ys),fill=False,edgecolor='red',label='local ground ROI: rejected, no solution'))
  for contact in failure['contacts']:
   via=next(v for v in board['copper']['fixed']['vias'] if v['id']==contact['viaId']);ax[0].scatter(via['at']['x'],via['at']['y'],c='red',marker='x')
  ax[0].set(xlabel='board x (mm)',ylabel='board y (mm)',title='Captured selections; RF section has explicit stack hypothesis');ax[0].invert_yaxis();ax[0].legend(fontsize=8)
  ax[1].plot(np.array(curve['axis']['values'])/1e9,curve['values'],'o-');ax[1].set(xlabel='GHz',ylabel='|S21| at 50 Ohm',title='Supported weak-loss P7b section; no full RF-channel claim')
  frequencies=np.array(rlgc['frequenciesHz'])/1e9;mode=[s['modes'][0] for s in rlgc['samples']];ax[2].plot(frequencies,[m['resistanceOhmPerM'] for m in mode],'o-',label='conductor R');ax[2].set(xlabel='GHz',ylabel='Ohm/m',title='Conductor and regional dielectric losses');twin=ax[2].twinx();twin.plot(frequencies,[m['conductanceSPerM'] for m in mode],'s-',c='orange',label='dielectric G');twin.set_ylabel('S/m');ax[2].legend(loc='upper left');twin.legend(loc='lower right')
  ax[3].axis('off');text='Local ground-via model: REJECTED\n'+failure['diagnostic']+'\n\nRF model cache: cold='+cold['cache']['status']+', warm='+warm['cache']['status']+'\nRF warm assembly/training: '+str(warm['cache']['assembly'])+'/'+str(warm['cache']['training'])+'\nFamily cold: '+format(run['wallSeconds'],'.4f')+' s / '+format(run['peakRssBytes']/1048576,'.2f')+' MiB\n'+failure['fullRfChannel'];text='\n'.join(textwrap.fill(line,76) for line in text.split('\n'));ax[3].text(0,1,text,va='top',fontsize=9)
  summary[name]=dict(cold=run,warmCache=warm['cache'],restart=restart,checks=checks,localVolumeStatus='rejected-2GiB',localDiagnostic=failure['diagnostic'],rfBandHz=[500000000,1000000000],rfTransmissionMin=min(curve['values']))
 fig.suptitle(name+' — direct DSL / CPU F64 / conditional applicability',fontsize=14);path=folder/'integration-report.png';fig.savefig(path,dpi=150);plt.close(fig);summary[name]['visual']=str(path.relative_to(root))
(base/'integration-summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps({n:{'wall':s['cold']['wallSeconds'],'rssMiB':s['cold']['peakRssBytes']/1048576,'warm':s['warmCache']['status'],'volume':s['localVolumeStatus']} for n,s in summary.items()},indent=2))
