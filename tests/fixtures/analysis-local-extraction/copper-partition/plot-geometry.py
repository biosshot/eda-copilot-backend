"""Saved geometry overlays, no mesh regeneration or integral evaluations."""
import os
os.environ['OPENBLAS_NUM_THREADS']='1'
from pathlib import Path
import json
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.collections import LineCollection
from trace import ROOT,FIX,OUT,OLD,NAMES,read,load,geometry,contours
def main():
 originals=read(OUT/'source-regions.json');fig,axes=plt.subplots(3,2,figsize=(14,15),constrained_layout=True)
 for row,name in enumerate(NAMES):
  supports=read(OLD/'independent-current'/f'{name}-supports.json')['foil'];pre=read(OLD/f'{name}-whole-barrel-preflight.json')
  layers=read(OLD/'taxonomy'/f'{name}-layer-contours.json')
  for col,kind in enumerate(['TOP','BOTTOM']):
   ax=axes[row,col]
   from shapely.geometry import shape
   g=shape(layers[kind]);polys=[g] if g.geom_type=='Polygon' else list(g.geoms)
   for p in polys:
    xy=np.array(p.exterior.coords);ax.fill(xy[:,0],xy[:,1],color='#c5dfca',alpha=.8)
    for hole in p.interiors:
     xy=np.array(hole.coords);ax.fill(xy[:,0],xy[:,1],color='white')
   lines=[];bad=[]
   for c in supports:
    if c['kind']!=kind:continue
    t=np.array(c['triangleMm']);closed=np.vstack([t,t[0]])
    (bad if c['aspectProxy']>1e6 else lines).append(closed)
   ax.add_collection(LineCollection(lines,colors='#35546b',linewidths=.35,alpha=.7));ax.add_collection(LineCollection(bad,colors='#c94431',linewidths=1))
   # Original per-source boundaries remain separate and are overlaid, not merged.
   board,_,sol,_=load(name)
   source_lines=[np.array(r['polygon']['outer']+[r['polygon']['outer'][0]]) for r in sol['regions'] if r['layer']==kind and r['source'].get('source_id') in {x['source'].get('source_id') for x in originals[name]}]
   ax.add_collection(LineCollection(source_lines,colors='#666666',linewidths=.35,alpha=.45,linestyles='dashed'))
   x0,y0,x1,y1=pre['scopeMm'];ax.set(xlim=(x0,x1),ylim=(y1,y0),aspect='equal',xlabel='x mm',ylabel='y mm',title=f'{name} {kind}: original sources + saved support edges')
   ax.text(.01,.01,'Green: frozen clipped Cu; dashed: prepared source boundaries\nBlue: saved triangles; red: aspect>1e6 (all retained)',transform=ax.transAxes,fontsize=8,va='bottom',bbox=dict(facecolor='white',alpha=.8))
 fig.savefig(OUT/'geometry-overlays.png',dpi=150);plt.close(fig)
 t=read(OUT/'stm-trace.json');fig,axes=plt.subplots(1,3,figsize=(16,5),constrained_layout=True)
 colors=['#3376a4','#228b60','#cc8030']
 ax=axes[0]
 for r,color in zip(t['preparedPerSource'],colors):
  xy=np.array(r['polygon']['outer']);ax.fill(xy[:,0],xy[:,1],color=color,alpha=.2);ax.plot(xy[:,0],xy[:,1],color=color,label=r['source']['source_id'][-6:])
 for w in t['supports']:
  xy=np.array(w['triangleMm']);xy=np.vstack([xy,xy[0]]);ax.plot(xy[:,0],xy[:,1],lw=1.3,label='support'+str(w['index']))
 ax.set(aspect='equal',xlim=(-4.05,-3.4),ylim=(12.8,12.1),xlabel='x mm',ylabel='y mm',title='Same-net T junction, source and support witnesses');ax.legend(fontsize=7)
 ax=axes[1];origin=np.array([-3.4541,12.425])
 for r,color in zip(t['preparedPerSource'],colors):
  xy=np.array(r['polygon']['outer'])-origin;xy=np.vstack([xy,xy[0]]);ax.plot(xy[:,0]*1e3,xy[:,1]*1e3,color=color)
 for w in t['supports']:
  xy=np.array(w['triangleMm'])-origin;xy=np.vstack([xy,xy[0]]);ax.plot(xy[:,0]*1e3,xy[:,1]*1e3,lw=1.2)
 ax.set(xlim=(-15,5),ylim=(20,-20),xlabel='x relative, um',ylabel='y relative, um',title='Micrometre view cannot resolve pm artifact');ax.grid(alpha=.2)
 ax=axes[2]
 for e in t['lineIntersectionWitnesses']:
  xy=np.array(e['segment'])-origin;ax.plot(xy[:,0]*1e9,xy[:,1]*1e9,color=colors[2])
 ax.axvline(0,color=colors[0],label='vertical track boundary')
 w=next(w for w in t['supports'] if w['index']==556);xy=(np.array(w['triangleMm'])-origin)*1e9;xy=np.vstack([xy,xy[0]]);ax.fill(xy[:,0],xy[:,1],alpha=.3,color='#c94431',label='retained support556')
 for p in t['savedUnionNearBoundaryVertices']:
  x,y=(np.array(p)-origin)*1e9;ax.plot(x,y,'ko',ms=4);ax.annotate(f'({x:.4f},{y:.4f})pm',(x,y),xytext=(5,3),textcoords='offset points',fontsize=8)
 ax.set(xlim=(-.1,.55),ylim=(5.2,-5.2),xlabel='x relative, pm',ylabel='y relative, pm',title='Per-source Boolean shifts -> GEOS intersections');ax.legend(fontsize=8,loc='lower right');ax.grid(alpha=.2)
 fig.savefig(OUT/'stm-origin-overlay.png',dpi=170);plt.close(fig)
if __name__=='__main__':main()
