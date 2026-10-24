"""Export all category counts/cost witnesses without hiding missing coverage."""
import csv,json,collections
from trace import OUT,FIX,read
a=read(OUT/'audit.json');rows=a['categoryRows']
with (OUT/'category-costs.csv').open('w',newline='') as f:
 w=csv.writer(f);w.writerow(['fixture','family','category','pairs','directed_evals','xy','z','priced_pairs','unpriced_pairs','sample_indices','pool_coverage','successful_work_low_s','successful_work_high_s','sample_positive_query_low','sample_positive_query_high','observed_refusal'])
 for r in rows:w.writerow([r['fixture'],r['family'],r['category'],r['pairs'],r['directedEvaluations'],*r['fixedRule'],r['pricedPairCount'],r['unpricedPairCount'],';'.join(map(str,r['sampleIndices'])),r['timingCoverage'],*r['workingCostSecondsRangeSuccessfulOnly'],*(r['successfulPointPositiveFractionRange'] or [None,None]),r['hasObservedRefusal']])
merged=[]
for name in [r['fixture'] for r in a['roiRows']]:
 for category in ['self','shared_face','shared_edge','close_disjoint','separated_disjoint']:
  rs=[r for r in rows if r['fixture']==name and r['category']==category]
  merged.append({'fixture':name,'category':category,'pairs':sum(r['pairs'] for r in rs),'workingSuccessfulCostRangeSeconds':[sum(r['workingCostSecondsRangeSuccessfulOnly'][k] for r in rs) for k in range(2)],'samplePositiveQueryFractionRange':[min((r['successfulPointPositiveFractionRange'][0] for r in rs if r['successfulPointPositiveFractionRange']),default=None),max((r['successfulPointPositiveFractionRange'][1] for r in rs if r['successfulPointPositiveFractionRange']),default=None)],'refusal':any(r['hasObservedRefusal'] for r in rs),'pricedPairCount':sum(r['pricedPairCount'] for r in rs),'unpricedPairCount':sum(r['unpricedPairCount'] for r in rs),'populationFallbackUnknown':True})
(OUT/'category-summary.json').write_text(json.dumps(merged,indent=2));print(json.dumps(merged))
