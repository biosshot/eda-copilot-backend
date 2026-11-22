"""Saved-source geometry overlays and separate error/cost measurements."""
from pathlib import Path
import json
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import numpy as np
from shapely.geometry import shape,Polygon
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint/copper-context';OLD=OUT.parent
read=lambda p:json.loads(p.read_text());a=read(OUT/'audit.json')
names=['STM32G0_CAN_RS485','TPS63060DSCR','BandAmpV5']
fig,axes=plt.subplots(3,2,figsize=(12,12),layout='constrained')
for i,name in enumerate(names):
    old=read(OLD/'taxonomy'/(name+'-layer-contours.json'));new=read(OUT/(name+'-G1-opt.json'))['roi'][0]
    for j,layer in enumerate(['TOP','BOTTOM']):
        ax=axes[i,j];g=shape(old[layer]);parts=list(g.geoms) if hasattr(g,'geoms') else [g]
        for p in parts:
            for r in [p.exterior,*p.interiors]:ax.plot(*r.xy,color='#888888',lw=1.2,alpha=.6)
        for p in new[layer]:
            for ring in [p['outer'],*p['holes']]:
                r=ring+[ring[0]];ax.plot(*zip(*r),color='#126cab',lw=.65)
        ax.set_aspect('equal');ax.set_title(name+' / '+layer);ax.set_xlabel('x / mm');ax.set_ylabel('y / mm')
fig.suptitle('Grey: frozen experimental G0 source union; blue: G1 native shared-context union/clip\nAll sources retained; count changes belong to geometry, not G2 classification',fontsize=12)
fig.savefig(OUT/'geometry-overlays.png',dpi=150)

fig,axes=plt.subplots(2,2,figsize=(13,9),layout='constrained')
boards=a['boards'];labels=[r['name'] for r in boards];y=np.arange(len(boards))
for offset,g,color in [(-.17,'G0','#999999'),(.17,'G1','#1676b7')]:
    axes[0,0].barh(y+offset,[np.mean(r[g+'ColdWarmSeconds']) for r in boards],height=.3,label=g,color=color)
axes[0,0].set_yticks(y,labels);axes[0,0].set_xscale('log');axes[0,0].set_xlabel('mean preparation wall / s (cold + warm, one process)');axes[0,0].legend();axes[0,0].set_title('A correctness tradeoff; NanoScope +13.3%')
for offset,g,color in [(-.17,'G0','#999999'),(.17,'G1','#1676b7')]:
    axes[0,1].barh(y+offset,[r[g+'PeakBytes']/2**20 for r in boards],height=.3,label=g,color=color)
axes[0,1].set_yticks(y,labels);axes[0,1].set_xlabel('observed process peak / MiB');axes[0,1].set_title('All 8 frozen boards; no excluded source or new short')
x=np.arange(3)
for off,g,col in [(-.18,'G1','#999999'),(.18,'G2','#1676b7')]:
    axes[1,0].bar(x+off,[r[g]['nearPairs'] for r in a['plans']],width=.33,label=g,color=col)
axes[1,0].set_xticks(x,['STM','TPS','Band']);axes[1,0].set_ylabel('exact-near unordered pairs');axes[1,0].legend();axes[1,0].set_title('B: identical post-fix geometry, theta0.4 / leaf8')
for i,c in enumerate(a['coupon']):
    t=[r['wallSeconds'] for r in c['runs']];axes[1,1].scatter(np.full(3,i),t,s=45,label=c['configuration']);axes[1,1].plot([i-.15,i+.15],[np.mean(t)]*2,color='black')
axes[1,1].set_xticks([0,1],['G1 / all near','G2 / cached individual']);axes[1,1].set_ylabel('full kernel extraction + matrix/actions / s');axes[1,1].set_title('Measured mean gain %.3fx; 576 -> 294 exact evaluations'%a['measuredMeanSpeedup']);axes[1,1].set_ylim(bottom=0)
fig.suptitle('Geometry and classification measured separately; no full ROI operator runtime claim',fontsize=13)
fig.savefig(OUT/'cost-and-memory.png',dpi=160)

fig,axes=plt.subplots(2,2,figsize=(11,8),layout='constrained');ref=np.array(read(ROOT/'tests/fixtures/analysis-local-extraction/magnetic-near-far/reference.json')['levels'][-1]['matrixH']);w,v=np.linalg.eigh((ref+ref.T)/2);W=(v/np.sqrt(w))@v.T
for i,c in enumerate(a['coupon']):
    r=c['runs'][0];m=np.array(r['matrixH']);e=100*W@((m-ref)+(m-ref).T)/2@W
    im=axes[0,i].imshow(e,cmap='coolwarm',vmin=-.05,vmax=.05);axes[0,i].set_title(c['configuration']+' signed full-space error / %');fig.colorbar(im,ax=axes[0,i],shrink=.75)
    for j in range(4):
        for k in range(4):axes[0,i].text(k,j,'%.3f'%e[j,k],ha='center',va='center',fontsize=8)
axes[1,0].bar(['G1 full','G2 full','G2 far'],[100*a['coupon'][0]['runs'][0]['fullScreen'],100*a['coupon'][1]['runs'][0]['fullScreen'],100*a['coupon'][1]['runs'][0]['aggregateFarBudget']]);axes[1,0].axhline(5,color='red',label='full gate5%');axes[1,0].axhline(1,color='orange',ls='--',label='far gate1%');axes[1,0].set_ylabel('%');axes[1,0].legend();axes[1,0].set_title('Independent reference uncertainty retained separately')
axes[1,1].bar([str(r['postfixIndex']) for r in a['postfixNear']],[r['screen']*100 for r in a['postfixNear']]);axes[1,1].axhline(.2,color='red',label='near gate0.2%');axes[1,1].set_ylabel('error + empirical uncertainty / %');axes[1,1].set_title('Post-fix finite-prism self checks');axes[1,1].legend()
fig.suptitle('Raw reciprocity and PSD pass; empirical reference margin is not a continuum certificate',fontsize=12)
fig.savefig(OUT/'matrix-and-error.png',dpi=170)
