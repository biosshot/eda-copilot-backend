"""Geometry-only near census; no integral/operator construction. Run after heavy release."""
import os
os.environ['OPENBLAS_NUM_THREADS']='1';os.environ['OMP_NUM_THREADS']='1'
import json,time,math,collections,hashlib
from pathlib import Path
import numpy as np
import shapely
from shapely.geometry import Polygon,Point
from shapely.strtree import STRtree
from trace import ROOT,FIX,OUT,OLD,NAMES,read,sha,load,geometry,quantiles
THETA=.4
LABELS=['self','shared_face','shared_edge','shared_vertex','overlap','close_disjoint','separated_disjoint','topology_indeterminate']
WHY=['retained_near','leaf_coarsening_only','ball_overestimate']
class Tree:
 def __init__(self,rows,native):
  self.xy=np.array([c['triangleM'] for c in native['cells']]);self.z=np.array([c['zM'] for c in native['cells']])
  self.c=np.array([m['center'] for m in native['moments']]);self.r=np.array([m['radius'] for m in native['moments']])
  self.order=native['order'];self.nodes=native['nodes'];self.blocks=native['blocks']
 def mask(self):
  n=len(self.order);mask=np.zeros((n,n),dtype=bool);near=far=0
  for a,b,f in self.blocks:
   aa=self.nodes[a];bb=self.nodes[b];i=self.order[aa['s']:aa['e']];j=self.order[bb['s']:bb['e']];count=len(i)*(len(i)+1)//2 if a==b else len(i)*len(j)
   if f:far+=count
   else:near+=count;mask[np.ix_(i,j)]=True;mask[np.ix_(j,i)]=True
  assert near+far==n*(n+1)//2
  return mask,near,far
def main():
 started=time.perf_counter();native=read(OLD/'magnetic-near-far/plan-run/stdout.json');expected={(Path(r['source']).stem.removesuffix('-supports'),g['kind']):g['plan'] for r in native['rois'] for g in r['groups'] if g['theta']==THETA}
 native_plans={(Path(r['source']).stem.removesuffix('-supports'),r['family']):r for r in [json.loads(line) for line in (OUT/'plans.jsonl').read_text().splitlines()]}
 reports=[];cohorts={};source_rows={};case_map={};source_polys={}
 def offer(name,label,ii,jj,score,rows,family):
  if not len(ii):return
  k=int(np.argmin(score));item=(int(score[k]),family,int(ii[k]),int(jj[k]));key=(name,label)
  cohort=cohorts.setdefault(key,{'count':0,'candidate':item});cohort['count']+=len(ii)
  if item<cohort['candidate']:cohort['candidate']=item
 for name in NAMES:
  board,pre,sol,_=load(name);raw=read(OLD/'independent-current'/f'{name}-supports.json');families=[]
  from shapely.geometry import box
  roi=box(*pre['scopeMm']);source_polys[name]=[(r['source'],r['layer'],geometry(r)) for r in sol['regions'] if geometry(r).intersects(roi)]
  for family in ['foil','axialSubprisms']:
   rows=raw[family];source_rows[name,family]=rows;n=len(rows);assert n<4096
   tree=Tree(rows,native_plans[name,family]);near_mask,near,far=tree.mask();match=expected[name,family];assert near==match['nearPairs'] and far==match['farPairs'],(name,family,near,match['nearPairs'])
   xy=np.array([r['triangleMm'] for r in rows]);z=np.array([r['zRangeMm'] for r in rows]);h=z[:,1]-z[:,0];v=xy[:,1]-xy[:,0];w=xy[:,2]-xy[:,0]
   area=abs(v[:,0]*w[:,1]-v[:,1]*w[:,0])/2;assert np.all(area>0)
   aspect=np.array([r['aspectProxy'] for r in rows]);bins=np.array([r['shapeBin'] for r in rows]);polys=np.array([Polygon(t) for t in xy],dtype=object)
   edges=collections.Counter()
   for i,t in enumerate(xy):
    for e in range(3):edges[(rows[i]['kind'],tuple(sorted([tuple(t[e]),tuple(t[(e+1)%3])])))] += 1
   contacts=np.full((n,n),255,dtype=np.uint8);np.fill_diagonal(contacts,0)
   ia,ib=STRtree(polys).query(polys,predicate='intersects');sel=ib>ia;ia=ia[sel];ib=ib[sel]
   za=z[ia];zb=z[ib];zover=np.minimum(za[:,1],zb[:,1])-np.maximum(za[:,0],zb[:,0]);hit=zover>=0;ia=ia[hit];ib=ib[hit];zover=zover[hit]
   dims=shapely.get_dimensions(shapely.intersection(polys[ia],polys[ib]))
   # Extruded XY side ->3D face; XY vertex ->vertical3D edge.
   codes=np.where(dims==2,np.where(zover>0,4,1),np.where(dims==1,np.where(zover>0,1,2),np.where(zover>0,2,3))).astype(np.uint8)
   codes[dims<0]=7
   contacts[ia,ib]=codes;contacts[ib,ia]=codes
   table=collections.Counter();why_counts=collections.Counter();strata=collections.Counter();gap_bins=np.zeros(4,dtype=np.int64)
   hist_edges=np.array([0,.1,.5,2,np.inf]);center=tree.c;relative=tree.xy-center[:,None,:2];salt=np.uint64(int(sha(OLD/'independent-current'/f'{name}-supports.json')[:8],16))
   for first in range(0,n,96):
    stop=min(first+96,n);i,j=np.nonzero(near_mask[first:stop]& (np.arange(n)[None,:]>=np.arange(first,stop)[:,None]));i=i+first
    if not len(i):continue
    cat=contacts[i,j].copy();dis=cat==255
    gapz=np.maximum(np.maximum(z[i,0]-z[j,1],z[j,0]-z[i,1]),0);gapxy=np.zeros(len(i))
    gapxy[dis]=shapely.distance(polys[i[dis]],polys[j[dis]])
    gap=np.hypot(gapxy,gapz);close=gap<=np.maximum(h[i],h[j]);cat[dis]=np.where(close[dis],5,6)
    dist=np.hypot(np.hypot(center[i,0]-center[j,0],center[i,1]-center[j,1]),center[i,2]-center[j,2])
    with np.errstate(divide='ignore',invalid='ignore'):q=(tree.r[i]+tree.r[j])/dist*(1+128*np.finfo(float).eps)
    maxr=np.zeros(len(i));zh=(tree.z[i,1]-tree.z[i,0]+tree.z[j,1]-tree.z[j,0])/2
    for a in range(3):
     for b in range(3):
      dx=relative[i,a,0]-relative[j,b,0];dy=relative[i,a,1]-relative[j,b,1]
      maxr=np.maximum(maxr,np.hypot(np.hypot(dx,dy),zh))
    with np.errstate(divide='ignore',invalid='ignore'):qsupport=maxr/dist*(1+128*np.finfo(float).eps)
    reason=np.where(q<=THETA,1,np.where(qsupport<=THETA,2,0));worstbin=np.maximum(bins[i],bins[j])
    score=(i.astype(np.uint64)*np.uint64(73856093))^(j.astype(np.uint64)*np.uint64(19349663))^salt
    for c,label in enumerate(LABELS):
     take=cat==c;count=int(take.sum());table[label]+=count
     offer(name,label,i[take],j[take],score[take],rows,family)
     for r,why in enumerate(WHY):
      both=take&(reason==r);why_counts[why]+=int(both.sum())
      for b in range(5):strata[label,why,b]+=int(np.count_nonzero(both&(worstbin==b)))
    for r in [1,2]:
     take=reason==r;offer(name,WHY[r],i[take],j[take],score[take],rows,family)
    if family=='axialSubprisms':
     for c,label in [(0,'axial_self'),(5,'axial_close')]:
      take=cat==c;offer(name,label,i[take],j[take],score[take],rows,family)
    diameter=np.maximum(np.linalg.norm(np.ptp(xy[i],axis=1),axis=1),np.linalg.norm(np.ptp(xy[j],axis=1),axis=1))
    gap_bins+=np.histogram(gap[dis]/diameter[dis],bins=hist_edges)[0]
   assert sum(table.values())==near and sum(why_counts.values())==near
   tail=int(np.argmax(aspect));offer(name,family+'_worst_aspect_self',np.array([tail]),np.array([tail]),np.array([0]),rows,family)
   families.append({'family':family,'supports':n,'nearPairs':near,'farPairs':far,'primaryCounts':dict(table),'whyCounts':dict(why_counts),'strata':[{'category':c,'why':r,'worstXYShapeBin':b,'count':count} for (c,r,b),count in sorted(strata.items()) if count],
      'statistics':{'minimumAltitudeMm':quantiles(np.sqrt(2*area/aspect)),'shortestEdgeMm':quantiles(np.min(np.linalg.norm(xy-np.roll(xy,1,axis=1),axis=2),axis=1)),'areaMm2':quantiles(area),'volumeM3':quantiles(area*h*1e-9),'xyAspect':quantiles(aspect),'edgeMultiplicityCounts':dict(collections.Counter(edges.values())),'uniqueEdges':len(edges),'sharedExactEdges':sum(v==2 for v in edges.values()),'minCoordinateUlpMm':min(math.ulp(x) for t in xy for p in t for x in p),'worstIndex':tail,'worstSupport':rows[tail],'belowBooleanWidthContract':int(np.count_nonzero(np.sqrt(2*area/aspect)<next(r for r in read(OUT/'inventory.json') if r['name']==name)['booleanErrorBoundMm']))},
      'disjointGapOverMaxXYBboxDiameterBins':{'edges':[0,.1,.5,2,'inf'],'counts':gap_bins.tolist()},'planMatchesFrozenNativeCounts':True})
  reports.append({'name':name,'families':families})
 samples=[]
 for (name,label),c in sorted(cohorts.items()):
  _,family,i,j=c['candidate'];key=(name,family,min(i,j),max(i,j))
  if key not in case_map:
   case_map[key]={'fixture':name,'family':family,'indices':[min(i,j),max(i,j)],'cohorts':[],'cells':[source_rows[name,family][k] for k in [min(i,j),max(i,j)]]};samples.append(case_map[key])
  case_map[key]['cohorts'].append({'label':label,'populationCount':c['count']})
 # Known accepted STM witnesses: new telemetry only; prior native timings reused.
 for i,j in [(668,556),(668,668),(556,556)]:
  key=(NAMES[0],'foil',min(i,j),max(i,j))
  if key not in case_map:
   case_map[key]={'fixture':NAMES[0],'family':'foil','indices':[min(i,j),max(i,j)],'cohorts':[],'cells':[source_rows[NAMES[0],'foil'][k] for k in [min(i,j),max(i,j)]]};samples.append(case_map[key])
  case_map[key]['reusePriorTiming']=True;case_map[key]['cohorts'].append({'label':'known_stm_witness','populationCount':None})
 for sample in samples:
  sample['sourceOwners']=[]
  for cell in sample['cells']:
   p=Polygon(cell['triangleMm']);point=p.representative_point()
   sample['sourceOwners'].append([source for source,layer,poly in source_polys[sample['fixture']] if layer==cell['kind'] and poly.covers(point)] if sample['family']=='foil' else [{'sectorId':cell.get('sectorId')}])
 samples.sort(key=lambda s:(max(c['aspectProxy'] for c in s['cells'])>1e6,s['fixture'],s['family'],s['indices']))
 assert len(samples)<=48
 frozen={'selection':'Before all new measurements: min deterministic index hash per nonempty primary/false-near cohort; explicit axial controls, per-family worst-aspect self and known STM telemetry witnesses. Duplicates reused. No time-based reselection.',
 'fixedOrders':[[12,8],[24,16]],'maxSamples':48,'samples':samples,'capSeconds':120,'capBytes':512*1024**2,'maximumPointQueriesActualPlusTelemetry':len(samples)*2*(12*12*8+24*24*16)*2,'sourceKernelUnchanged':True,'noLargeROIOperator':True}
 (OUT/'census.json').write_text(json.dumps({'theta':THETA,'closeDefinition':'Exact3D prism distance <=max thickness of the pair. XY contact dimensions translated to extruded3D dimension; foil/axial vector-orthogonal pairs excluded as in frozen model.','whyMeaning':'leaf_coarsening_only: individual ball q<=theta; ball_overestimate: exact convex support-difference radius q<=theta but sum-ball q>theta. Diagnostic counts only; aggregate energy gate still needed.','rows':reports,'seconds':time.perf_counter()-started},indent=2))
 (FIX/'measurement-preflight.json').write_text(json.dumps(frozen,indent=2));(OUT/'measurement-preflight.json').write_bytes((FIX/'measurement-preflight.json').read_bytes())
 print(json.dumps({'seconds':time.perf_counter()-started,'frozenSamples':len(samples),'rows':[{'name':r['name'],'families':[{k:f[k] for k in ['family','supports','nearPairs','primaryCounts','whyCounts']} for f in r['families']]} for r in reports]}))
if __name__=='__main__':main()
