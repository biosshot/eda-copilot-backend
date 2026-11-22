"""Saved-values audit: provenance replay, stratified timing, failures and savings."""
import os
os.environ['OPENBLAS_NUM_THREADS']='1'
import json,math,collections
from pathlib import Path
import numpy as np
from shapely.geometry import Polygon,shape
from trace import ROOT,FIX,OUT,OLD,NAMES,read,sha
def main():
 frozen=read(FIX/'measurement-preflight.json');samples=frozen['samples'];batch=[json.loads(l) for l in (OUT/'batch.jsonl').read_text().splitlines()]
 assert batch[-1]['kind']=='done' and batch[-1]['cases']==len(samples)
 measurements=[r for r in batch if r['kind']=='measurement'];assert len(measurements)==2*len(samples)
 assert all(r['instrumentedParity'] is not False for r in measurements)
 trace=read(OUT/'stm-trace.json');replay=[]
 for r in batch:
  if r['kind']!='geometry':continue
  old=next(t for t in trace['preparedPerSource'] if t['source']['source_id']==r['sourceId'])
  a=Polygon(old['polygon']['outer'],old['polygon']['holes']);b=Polygon(r['polygons'][0]['outer'],r['polygons'][0]['holes'])
  same=a.equals_exact(b,0);assert same
  replay.append({'sourceId':r['sourceId'],'exactCoordinatesMatch':same,'symmetricDifferenceMm2':a.symmetric_difference(b).area})
 prior_path=next((OLD/'magnetic-stability').glob('current-run-*/stdout.json'));prior=read(prior_path)['pairs']
 costrows=[];failures=[]
 for r in measurements:
  s=samples[r['sample']];counters=r['positiveCounters'];positive=counters[1]/counters[0] if counters[0] else None
  accepted='Ok' in r['forwardM5'] and 'Ok' in r['reverseM5']
  if s.get('reusePriorTiming'):
   i,j=s['indices'];label='stm668-556' if i!=j else 'stm'+str(i)+'-self'
   previous=next(p for p in prior if p['label']==label and p['xyOrder']==r['xyOrder'] and p['zOrder']==r['zOrder'])
   seconds=previous['seconds']*(.5 if r['self'] else 1);timing_source='reused accepted current-kernel timing, includes old preparation overhead'
  else:seconds=r['actualSeconds'];timing_source='new actual uninstrumented accepted kernel, preparation excluded'
  if s['indices'][0]==s['indices'][1]:category='self'
  else:
   a,b=[Polygon(c['triangleMm']) for c in s['cells']];za,zb=[c['zRangeMm'] for c in s['cells']]
   overlap=min(za[1],zb[1])-max(za[0],zb[0]);intersection=a.intersection(b)
   if overlap>=0 and not intersection.is_empty:
    if intersection.geom_type in ['Polygon','MultiPolygon']:category='overlap' if overlap>0 else 'shared_face'
    elif intersection.geom_type in ['LineString','MultiLineString']:category='shared_face' if overlap>0 else 'shared_edge'
    else:category='shared_edge' if overlap>0 else 'shared_vertex'
   else:
    gap=math.hypot(a.distance(b),max(za[0]-zb[1],zb[0]-za[1],0))
    category='close_disjoint' if gap<=max(za[1]-za[0],zb[1]-zb[0]) else 'separated_disjoint'
  cost={'category':category,'sample':r['sample'],'fixture':s['fixture'],'family':s['family'],'indices':s['indices'],'cohorts':s['cohorts'],'worstShapeBin':max(c['shapeBin'] for c in s['cells']),'xyOrder':r['xyOrder'],'zOrder':r['zOrder'],'directedCalls':r['directedCalls'],'secondsAllDirections':seconds,'secondsPerDirectedKernel':seconds/r['directedCalls'],'timingSource':timing_source,'acceptedIntegralReturned':accepted,'positiveFractionSuccessfulPointQueries':positive,'pointQueryCounters':counters,'instrumentationParity':r['instrumentedParity'],'forwardM5':r['forwardM5'],'reverseM5':r['reverseM5'],'sourceOwners':s['sourceOwners']}
  if accepted:
   f=r['forwardM5']['Ok'];v=r['reverseM5']['Ok'];cost['rawPairAsymmetry']=abs(f-v)/max(abs(f),abs(v))
  else:failures.append(cost)
  costrows.append(cost)
 census=read(OUT/'census.json');old_counts=read(OLD/'independent-current-counts.json')['rows']
 # Sample pools remain explicit, never apply pathological witness mean to all pairs.
 refined=[r for r in costrows if r['xyOrder']==24 and r['acceptedIntegralReturned']]
 successful=[r for r in refined if not r['indices'][0]==r['indices'][1] or r['family']=='axialSubprisms']
 normal=[r for r in successful if r['positiveFractionSuccessfulPointQueries']==0]
 normal_low=min(r['secondsPerDirectedKernel'] for r in normal);normal_high=max(r['secondsPerDirectedKernel'] for r in normal)
 far_meta=read(OLD/'magnetic-near-far/audit.json');far_unit=next(p['farPreparationSeconds']/p['plan']['farPairs'] for p in far_meta['policies'] if p['theta']==.4)
 roirows=[];categoryrows=[]
 for roi in census['rows']:
  name=roi['name'];base=next(r for r in old_counts if r['fixture']==name)
  n=sum(f['supports'] for f in roi['families']);near=sum(f['nearPairs'] for f in roi['families']);far=sum(f['farPairs'] for f in roi['families']);leaf=sum(f['whyCounts']['leaf_coarsening_only'] for f in roi['families']);ball=sum(f['whyCounts']['ball_overestimate'] for f in roi['families'])
  for f in roi['families']:
   for category,count in f['primaryCounts'].items():
    if not count:continue
    populations=[t for t in f['strata'] if t['category']==category]
    details=[];all_pool=[];unpriced=0;low_work=high_work=0.
    for t in populations:
     shape_bin=t['worstXYShapeBin'];self_kind=category=='self'
     pool=[r for r in refined if r['family']==f['family'] and r['category']==category and r['worstShapeBin']==shape_bin]
     borrowing='same family/category/shape; all three ROIs'
     if not pool:
      pool=[r for r in refined if r['family']==f['family'] and (r['category']=='self')==self_kind and r['worstShapeBin']==shape_bin]
      borrowing='same family/shape/self-vs-mutual; category unmeasured'
     directed=t['count'] if self_kind else 2*t['count']
     if not pool:
      unpriced+=t['count'];details.append({**t,'unpriced':True,'reason':'No same family/shape/self-vs-mutual sample. Do not spread a pathological timing across other shapes.'});continue
     lo=min(r['secondsPerDirectedKernel'] for r in pool);hi=max(r['secondsPerDirectedKernel'] for r in pool)
     low_work+=directed*lo;high_work+=directed*hi;all_pool+=pool
     details.append({**t,'unpriced':False,'coverage':borrowing,'sampleIndices':sorted({r['sample'] for r in pool}),'secondsPerDirectedKernelRange':[lo,hi]})
    failed=any(r['fixture']==name and r['family']==f['family'] and category=='self' for r in failures)
    fractions=[r['positiveFractionSuccessfulPointQueries'] for r in all_pool]
    categoryrows.append({'fixture':name,'family':f['family'],'category':category,'pairs':count,'directedEvaluations':count if category=='self' else 2*count,'fixedRule':[24,16],
      'sampleIndices':sorted({r['sample'] for r in all_pool}),'timingCoverage':'Explicit family/category/shape-bin strata; each borrowed pool labeled below.',
      'workingCostSecondsRangeSuccessfulOnly':[low_work,high_work],'pricedPairCount':count-unpriced,'unpricedPairCount':unpriced,'stratifiedDetails':details,
      'successfulPointPositiveFractionRange':[min(fractions),max(fractions)] if fractions else None,'hasObservedRefusal':failed,
      'populationPositiveFraction':'Not measured; point-query sample fraction is not a population estimate.','uncertainty':'Partial observed work range only. Unpriced strata and refusals prevent a full accepted-runtime bound.'})
  before=(2*near-n);after=before-2*leaf
  roirows.append({'fixture':name,'supports':n,'foilTriangles':base['foilVolumeCells'],'axialSectorDofs':base['axialSectorCurrentDofs'],'candidateCurrentDofs':base['currentDofs'],'candidatePotentialNodes':base['potentialNodes'],'rawConductorCdtTriangles':base['rawConductorOnlyFoil']['supportCount'],'rawCdtCounterfactualMeaning':'Same saved layer-domain unconstrained CDT count; removes barrel/common constraints and changes current space/interface conformity. Not an approved reduction.',
   'nearPairs':near,'farPairs':far,'leafOnlyPairs':leaf,'leafOnlyFraction':leaf/near,'extraBallOverestimatePairs':ball,'remainingNearPairsAfterLeafFilter':near-leaf,'directedNearCountBefore':before,'directedNearCountAfter':after,'countOnlyMaxSpeedup':before/after,'normalZeroFallbackWorkingSecondsBefore':[before*normal_low,before*normal_high],'normalZeroFallbackWorkingSecondsAfter':[after*normal_low+(far+leaf)*far_unit,after*normal_high+(far+leaf)*far_unit],'conditionalSecondsSaved':[2*leaf*normal_low-leaf*far_unit,2*leaf*normal_high-leaf*far_unit],
   'guaranteedSecondsSaved':0,'guaranteedRuntimeUpper':None,'limitation':'Working estimate conditional on successful normal-query cohort and future full physical aggregate bound. STM613 self remains exact-near and refuses; no full ROI accepted or timed.'})
 # Reuse already accepted leaf1 coupon operator: it is exactly the individual-q far set.
 control=read(OLD/'magnetic-near-far/audit.json');c=next(p for p in control['policies'] if p['theta']==.4)
 reuse={'inputSha256':sha(FIX.parent/'magnetic-near-far/coupon.json'),'referenceSha256':sha(FIX.parent/'magnetic-near-far/reference.json'),'sourceAuditSha256':sha(OLD/'magnetic-near-far/audit.json'),'candidateMatrixH':c['matrixH'],'referenceMatrixH':control['reference']['matrixH'],'analyticBoundMatrixH':c['analyticBoundMatrixH'],'fullScreen':c['fullScreen'],'farBudget':c['aggregateFarBudget'],'rawReciprocity':c['reciprocity'],'physicalEigenvaluesH':c['physicalEigenvaluesH'],'fullGate':.05,'farGate':.01,'rawReciprocityGate':1e-9,'meaning':'Reused native leaf1 policy equals all individual q<=theta pairs, same geometry/current space/kernel. Existing full closed-current energy/PSD/reciprocity/reference gates retained. No new integration/prototype approximation or ROI transfer.'}
 assert reuse['fullScreen']<.05 and reuse['farBudget']<.01 and reuse['rawReciprocity']<1e-9 and min(reuse['physicalEigenvaluesH'])>0
 result={'replay':replay,'caseRows':costrows,'failures':failures,'categoryRows':categoryrows,'roiRows':roirows,'normalSuccessfulQueryDirectedSecondsRange':[normal_low,normal_high],'reusedMomentPairSeconds':far_unit,'couponControlReuse':reuse,
 'recommendation':'ONE next change: stream individual existing Moment separation/remainder checks inside coarse near blocks, retaining exact self/failed pairs and the full physical aggregate budget. Keep source domain/current space unchanged. Do not implement or admit ROI here.',
 'limits':{'newKernelBatches':1,'frozenSamples':len(samples),'seconds':read(OUT/'run.json')['seconds'],'peakBytes':read(OUT/'run.json')['peakBytes'],'capBytes':512*1024**2,'capSeconds':120,'productionKernelMeshChanged':False,'fullROIOperatorBuilt':False,'newDependencies':False,'bigAddonRebuild':False},
 'uncertainty':'Timing strata are sparse deterministic witnesses, not confidence intervals. Population fallback fractions and accepted full ROI cost are unknown; no finite guaranteed runtime upper where refusal is observed.'}
 (OUT/'audit.json').write_text(json.dumps(result,indent=2));(FIX/'verification.json').write_text(json.dumps(result,indent=2));(FIX/'reused-coupon-control.json').write_text(json.dumps(reuse,indent=2))
 print(json.dumps({'replaysExact':len(replay),'parityFailures':sum(r['instrumentedParity'] is False for r in measurements),'refusalRows':len(failures),'normalDirectedMs':[1000*normal_low,1000*normal_high],'rois':[{k:r[k] for k in ['fixture','leafOnlyFraction','countOnlyMaxSpeedup','normalZeroFallbackWorkingSecondsBefore','normalZeroFallbackWorkingSecondsAfter']} for r in roirows]}))
if __name__=='__main__':main()
