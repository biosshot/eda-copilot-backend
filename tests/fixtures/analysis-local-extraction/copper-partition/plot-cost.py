"""Saved-value near categories/cost/fallback plots; no new kernel work."""
import os
os.environ['OPENBLAS_NUM_THREADS']='1'
from pathlib import Path
import json
import numpy as np
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt
from trace import OUT,read
a=read(OUT/'audit.json');c=read(OUT/'census.json');names=[r['fixture'] for r in a['roiRows']];labels=['STM','TPS','Band'];x=np.arange(3)
fig,axes=plt.subplots(2,2,figsize=(14,9),constrained_layout=True)
ax=axes[0,0];bottom=np.zeros(3)
for category,color in [('self','#6a4c93'),('shared_face','#d56549'),('shared_edge','#dba64c'),('close_disjoint','#5e9f73'),('separated_disjoint','#678eae')]:
 vals=[100*sum(f['primaryCounts'][category] for f in r['families'])/sum(f['nearPairs'] for f in r['families']) for r in c['rows']]
 ax.bar(x,vals,bottom=bottom,label=category,color=color);bottom+=vals
ax.set(xticks=x,xticklabels=labels,ylabel='% of near pairs',title='Most near pairs are geometrically disjoint');ax.legend(fontsize=8)
ax=axes[0,1];bottom=np.zeros(3)
for why,color in [('retained_near','#5d7285'),('leaf_coarsening_only','#e4a14e'),('ball_overestimate','#72a496')]:
 vals=[100*sum(f['whyCounts'][why] for f in r['families'])/sum(f['nearPairs'] for f in r['families']) for r in c['rows']]
 ax.bar(x,vals,bottom=bottom,label=why,color=color);bottom+=vals
ax.set(xticks=x,xticklabels=labels,ylabel='% of near pairs',title='Same fixed theta0.4: coarse leaf8 causes false-near work');ax.legend(fontsize=8)
ax=axes[1,0];rows=[r for r in a['caseRows'] if r['xyOrder']==24];valid=[r for r in rows if r['acceptedIntegralReturned']]
sc=ax.scatter([r['sample'] for r in valid],[r['secondsPerDirectedKernel']*1000 for r in valid],c=[r['positiveFractionSuccessfulPointQueries']*100 for r in valid],vmin=0,vmax=100,cmap='viridis',s=45)
for r in rows:
 if not r['acceptedIntegralReturned']:
  ax.plot(r['sample'],2.8,'rx',ms=10);ax.annotate('STM613 REFUSAL\nnot a valid cost',(r['sample'],2.8),xytext=(23,7),textcoords='data',ha='right',fontsize=8,color='r',arrowprops=dict(arrowstyle='->',color='r',lw=.8))
ax.set(yscale='log',xlabel='frozen sample index',ylabel='ms per directed kernel',title='Fixed XY24/z16: normal vs positive-tail timings')
fig.colorbar(sc,ax=ax,label='positive fallback % of successful point queries')
ax=axes[1,1]
for offset,key,color,label in [(-.16,'normalZeroFallbackWorkingSecondsBefore','#698ba6','current leaf8 normal-work proxy'),(.16,'normalZeroFallbackWorkingSecondsAfter','#7baa81','individual-filter conditional proxy')]:
 vals=[r[key] for r in a['roiRows']];mid=np.array([sum(v)/2 for v in vals]);err=np.array([(v[1]-v[0])/2 for v in vals]);ax.bar(x+offset,mid,.3,yerr=err,color=color,label=label,capsize=4)
ax.axhline(500,color='r',ls='--',label='historical500s target');ax.set(xticks=x,xticklabels=labels,yscale='log',ylabel='extrapolated seconds',title='No ROI solve; even conditional savings do not meet target');ax.legend(fontsize=8)
fig.suptitle('P7 copper partition audit | all source geometry/current spaces retained | one frozen batch')
fig.savefig(OUT/'categories-cost.png',dpi=165);plt.close(fig)
