"""Source and topology evidence; no new numerical operator."""
from pathlib import Path
import json,sys
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Polygon as Patch
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint/copper-geometry'
audit=json.loads((OUT/'audit.json').read_text());frozen=json.loads((Path(__file__).parent/'regressions.json').read_text())
regions={r['source']['source_id']:r for r in frozen['prepared']};pair=[v['id'] for v in audit['adapters']]
fig,axes=plt.subplots(2,2,figsize=(13,9),layout='constrained')
for sid,color in zip(pair,['#136cb2','#ed702b']):
 p=regions[sid]['polygon']['outer'];axes[0,0].add_patch(Patch(p,facecolor=color,alpha=.3,label=sid));axes[0,0].plot(*zip(*(p+[p[0]])),color=color,lw=.8)
axes[0,0].autoscale();axes[0,0].set_aspect('equal');axes[0,0].legend(fontsize=7);axes[0,0].set_title('Exact native source replay: two shared-endpoint tracks');axes[0,0].set_xlabel('x / mm');axes[0,0].set_ylabel('y / mm')
origin=audit['adapters'][0]['restoredMm']
for sid,color in zip(pair,['#136cb2','#ed702b']):
 p=regions[sid]['polygon']['outer'];x=[(q[0]-origin[0])*1e9 for q in p+[p[0]]];y=[(q[1]-origin[1])*1e9 for q in p+[p[0]]];axes[0,1].plot(x,y,color=color,lw=1)
for v,label in [(audit['adapters'][0]['restoredMm'],'original vertex'),(audit['intersections'][0]['roundedIntersectionMm'],'GEOS intersection')]:
 axes[0,1].scatter((v[0]-origin[0])*1e9,(v[1]-origin[1])*1e9,s=22,label=label)
axes[0,1].set_xlim(-.000004,.000001);axes[0,1].set_ylim(-.000001,.000004);axes[0,1].ticklabel_format(style='sci',axis='both',scilimits=(0,0));axes[0,1].set_title('STM613: GEOS creates 2.84e-6 pm edge before CDT');axes[0,1].set_xlabel('dx / pm');axes[0,1].set_ylabel('dy / pm');axes[0,1].legend(fontsize=8)
bridge=audit['rejectedHullBridge'];sid=bridge['sourceId'];stages={r['id']:r for r in map(json.loads,(OUT/'stages.jsonl').read_text().splitlines())};pad=bridge['padPolygonMm'];cx=sum(p[0] for p in pad)/len(pad);cy=sum(p[1] for p in pad)/len(pad)
for ps,color,label in [(stages[sid]['actual'][0]['outer'],'#136cb2','native straight side'),(stages[sid]['experimentalHull']['outer'],'#d84646','rejected circle-only hull'),(audit['rectanglePreservingPrototype']['polygonsMm'][sid],'#248c56','rectangle-preserving prototype'),(pad,'black','real 0.2 um pad bridge')]:
 p=ps+[ps[0]];axes[1,0].plot([(q[0]-cx)*1e3 for q in p],[(q[1]-cy)*1e3 for q in p],color=color,label=label)
axes[1,0].set_xlim(-1.3,1.3);axes[1,0].set_ylim(-1.3,1.3);axes[1,0].set_aspect('equal');axes[1,0].set_xlabel('dx / um');axes[1,0].set_ylabel('dy / um');axes[1,0].legend(fontsize=7);axes[1,0].set_title('Do not remove a real bridge to remove an artifact')
boards=json.loads((OUT/'corpus-baseline.json').read_text())['boards'];axes[1,1].barh([b['name'] for b in boards],[b['regions'] for b in boards]);axes[1,1].set_xlabel('saved prepared source regions');axes[1,1].set_title('All 8 frozen boards inventoried; no accepted after-A baseline')
fig.suptitle('Phase A incomplete — prototypes only; Phase B not started',fontsize=15)
fig.savefig(OUT/'geometry-evidence.png',dpi=170)
