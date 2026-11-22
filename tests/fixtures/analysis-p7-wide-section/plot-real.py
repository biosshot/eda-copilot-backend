"""Render the retained real-board adapter packet; no solve."""
import json,pathlib,sys,numpy as np,matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.patches import Polygon
p=pathlib.Path(sys.argv[1]);inp=json.loads((p/'real-input.json').read_text());r=json.loads((p/'real-result.json').read_text());c=inp['selection'];sources=c['corridors'][-1]['wideReturnDomains'][0]['sources'];origin=np.array(c['origin']);d=np.array(c['direction']);t=np.array(c['transverseUnit']);length=c['lengthMm'];fig,ax=plt.subplots(2,3,figsize=(16,9))
for a in ax[0,:2]:
 for s in sources:
  poly=s['polygon'];a.add_patch(Polygon(poly['outer'],facecolor=('#e66' if s['layer']=='TOP' else '#69c'),edgecolor='none',alpha=.2))
  for h in poly['holes']:a.add_patch(Polygon(h,facecolor='white',edgecolor='none'))
 for w,col in zip(c['widthsMm'],['#c90','#fa0','#080']):
  pts=[origin+d*x+t*y for x,y in [(-length/2,-w),(length/2,-w),(length/2,w),(-length/2,w)]];a.add_patch(Polygon(pts,facecolor='none',edgecolor=col,lw=2,label=f'halfwidth {w:.1f}mm'))
 a.plot(*origin,'ko');a.set_aspect('equal');a.autoscale_view();a.set(xlabel='board x mm',ylabel='board y mm')
ax[0,0].set_title('Original full STM32 copper and chosen section')
board=json.loads(inp['input']['boardJson'])
for term in inp['contacts']:
 pad=board['pads'][int(term['sourcePath'][5:-1])];xy=pad['at'];ax[0,0].plot(xy['x'],xy['y'],'mo');ax[0,0].annotate(pad['component']+'.'+pad['number'],(xy['x'],xy['y']))
ax[0,1].set_xlim(origin[0]-.6,origin[0]+.6);ax[0,1].set_ylim(origin[1]-7,origin[1]+7);ax[0,1].set_title('Endpoint cuts; external original copper retained');ax[0,1].legend(fontsize=8)
geom=r['report']['input']['geometry']
for metal in geom['conductors']:ax[0,2].add_patch(Polygon(np.array(metal['contour']['outerM'])*1000,label=metal['id']))
ax[0,2].autoscale_view();ax[0,2].set(xlabel='transverse mm',ylabel='stack z mm',title='Native conductors: two signals + finite physical return');ax[0,2].legend()
for k,j,scale in [('C',0,1e12),('L',1,1e9)]:
 for z,label in [(0,'self'),(1,'mutual')]:ax[1,0].plot([a['observables'][k][z]*scale for a in r['audit']],'o-',label=k+' '+label+(' pF/m' if k=='C' else ' nH/m'))
ax[1,0].set(title='Complete joint matrices retained',xlabel='holdout/final step');ax[1,0].legend(fontsize=8)
for k in ['C','C0','L','Z','S']:ax[1,1].plot([1,2,3],[100*r['audit'][i]['errors'][k] for i in [1,3,5]],'o-',label=k)
ax[1,1].axhline(2,color='k',ls='--');ax[1,1].set(xticks=[1,2,3],xticklabels=['width','air','mesh'],ylabel='%',title='Independent sensitivity to SAME final configuration');ax[1,1].legend()
S=r['report']['network']['parameters'];f=r['report']['frequenciesHz']
for out,inc in [(2,0),(3,0),(1,0)]:ax[1,2].loglog(f,[abs(complex(x[out][inc]['real'],x[out][inc]['imaginary'])) for x in S],'o-',label=f'|S{out}{inc}|')
ax[1,2].set(xlabel='Hz',title='Actual finite-length coupled response');ax[1,2].legend();fig.tight_layout();fig.savefig(p/'real-section.png',dpi=150)
print('rendered',p/'real-section.png')
