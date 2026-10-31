import pathlib,json,matplotlib,os
matplotlib.use('Agg')
import matplotlib.pyplot as plt
root=pathlib.Path(__file__).resolve().parents[1]; m=json.loads((root/'tests/fixtures/analysis-p7-closure/manifest.json').read_text());out=root/'debugging/p7-closure';out.mkdir(exist_ok=True)
fig,axes=plt.subplots(2,3,figsize=(16,10))
for ax,c in zip(axes.flat,m['cases'][1:]):
 b=json.loads((root/c['board']).read_text());nets={p['net'] for pair in c['contacts'] for p in pair}
 for t in b['copper']['fixed']['tracks']:
  if t.get('net') in nets:
   pts=t['points'];ax.plot([p['x'] for p in pts],[p['y'] for p in pts],alpha=.6,lw=.6,color='gray' if t.get('net')=='GND' else 'blue')
 for i,pair in enumerate(c['contacts']):
  for p in pair:
   x,y=p['at']['x'],p['at']['y'];ax.scatter(x,y,c='red' if i==0 else 'green');ax.annotate(p['component']+'.'+p['number'],(x,y),fontsize=7)
 ax.set_title(c['id']+' selected channels/return tracks');ax.set_aspect('equal');ax.set_xlabel('mm');ax.set_ylabel('mm');ax.invert_yaxis()
axes.flat[-1].axis('off');axes.flat[-1].text(.02,.5,'Captured track geometry and selected contacts.\nAll branches retained. Plane polygons are in source board.\nConnectivity/admission and full EM response remain unestablished.\nBenchmark stack/materials are assumptions.',wrap=True)
fig.tight_layout();fig.savefig(out/'real-selected-geometry.png',dpi=130)

# Geometry API output includes all captured copper, return islands and physical contacts.
audit=root/'debugging/p7-closure/geometry-audit.json'
if audit.exists():
 data=json.loads(audit.read_text());fig,axes=plt.subplots(2,3,figsize=(16,10))
 for ax,c in zip(axes.flat,data[:5]):
  for r in c.get('regions',[]):
   ring=r['polygon']['outer'];ax.fill([p[0] for p in ring],[p[1] for p in ring],alpha=.2,color='gray' if r['source'].get('net')=='GND' else 'blue')
  case=next(x for x in m['cases'] if x['id']==c['id'])
  for pair in case['contacts']:
   for p in pair: ax.scatter(p['at']['x'],p['at']['y'],c='red');ax.annotate(p['component']+'.'+p['number'],(p['at']['x'],p['at']['y']),fontsize=7)
  ax.set_title(c['id']+' signal/return '+str(c.get('signalConnected'))+'/'+str(c.get('returnConnected')));ax.set_aspect('equal');ax.invert_yaxis()
 axes.flat[-1].axis('off');fig.tight_layout();fig.savefig(out/'real-copper-audit.png',dpi=120)
packet=root/os.environ.get('P7_COMPARISON_PACKET','debugging/p7-closure/1793432658079920300/comparison.json')
if packet.exists():
 e=json.loads(packet.read_text());fig,axes=plt.subplots(1,2,figsize=(12,4))
 rows=[x for x in e['comparisons'] if x['metric']=='complex S' and x['i']==0 and x['j']==3]
 for label in ['actual','reference']:axes[0].loglog([x['frequencyHz'] for x in rows],[abs(complex(x[label]['real'],x[label]['imaginary'])) for x in rows],label=label,marker='o')
 axes[0].set_title('Finite35um coupled stripline far-end transfer S03');axes[0].legend();axes[0].set_xlabel('Hz')
 axes[1].plot([x['error']*100 for x in e['comparisons']],'.');axes[1].axhline(6,color='red',label='6% error + 2% uncertainty within 8%');axes[1].set_ylabel('relative error %');axes[1].legend();fig.tight_layout();fig.savefig(out/'independent-ac-error.png',dpi=130)
