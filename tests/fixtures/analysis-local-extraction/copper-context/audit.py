"""Saved-data-only G0/G1/G2 correctness, topology, error and timing audit."""
from pathlib import Path
import os,json,hashlib,collections,math
os.environ['OPENBLAS_NUM_THREADS']='1'
import numpy as np
from shapely.geometry import Polygon
from shapely.ops import unary_union
import shapely
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent
OUT=ROOT/'debugging/p7-design-checkpoint/copper-context';OLD=OUT.parent
read=lambda p:json.loads(p.read_text(encoding='utf-8'))
sym=lambda a:(a+a.T)/2
norm=lambda a:float(max(abs(np.linalg.eigvalsh(sym(a)))))
reference=read(FIX.parent/'magnetic-near-far/reference.json')
ref=np.array(reference['levels'][-1]['matrixH']);w,v=np.linalg.eigh(sym(ref));W=(v/np.sqrt(w))@v.T
uncertainty=read(FIX.parent/'magnetic-near-far/verification.json')['reference']['workingUncertainty']
dense=read(OLD/'magnetic-near-far/coupon-run/stdout.json');K=np.array(dense['kernelM5']);D=np.array([c['densities'] for c in read(FIX.parent/'magnetic-near-far/coupon.json')['cells']])
kernel_source=read(OUT/'kernel-build.json')['sources']
for path,h in kernel_source.items():assert hashlib.sha256((ROOT/path).read_bytes()).hexdigest()==h
matrix=[]
for g in ['G1','G2']:
    raw=read(OUT/(g+'-coupon.json'));runs=[]
    for r in raw['runs']:
        a,b=np.array(r['matrixH']),np.array(r['boundMatrixH']);delta=sym(a-np.array(dense['denseMatrixH']))
        error=norm(W@(a-ref)@W);beta=norm(W@b@W);recip=float(abs(a-a.T).max()/abs(a).max())
        assert error+uncertainty<=.05 and beta<=.01 and recip<=1e-9
        assert np.linalg.eigvalsh(sym(a)).min()>0
        assert min(np.linalg.eigvalsh(sym(b-delta)).min(),np.linalg.eigvalsh(sym(b+delta)).min())>=-512*np.finfo(float).eps*max(np.diag(a))
        action=[]
        for x in r['actions']:
            q=np.array(x['q']);J=np.einsum('nkd,k->nd',D,q);potential=1e-7*K@J;energy=.5*np.sum(J*potential)
            guard=512*np.finfo(float).eps*max(abs(potential).max(),1e-300)
            absolute=abs(x['energyJ']-energy)<=x['boundJ']+512*np.finfo(float).eps*abs(energy)
            field=bool(np.all(abs(np.array(x['integratedPotential'])-potential)<=np.array(x['potentialBounds'])+guard))
            assert absolute and field
            if np.linalg.norm(q)==0:assert x['energyJ']==x['boundJ']==0
            assert np.max(abs(np.array(x['response'])-a@q))<=512*np.finfo(float).eps*max(abs(a).max()*np.linalg.norm(q),1e-300)
            action.append({'q':q.tolist(),'absoluteEnergyChangeJ':abs(x['energyJ']-energy),'boundJ':x['boundJ'],'energyGate':absolute,'potentialGate':field})
        runs.append({'wallSeconds':r['wallSeconds'],'kernelEvaluations':r['kernelEvaluations'],'kernelSeconds':r['kernelSeconds'],'plan':r['plan'],
                     'fullError':error,'referenceWorkingUncertainty':uncertainty,'fullScreen':error+uncertainty,'aggregateFarBudget':beta,'rawReciprocity':recip,
                     'physicalEigenvaluesH':np.linalg.eigvalsh(sym(a)).tolist(),'matrixH':a.tolist(),'analyticBoundMatrixH':b.tolist(),'actions':action,'plannedSimultaneousBytes':r['plannedSimultaneousBytes']})
    matrix.append({'configuration':g,'runs':runs,'meanWallSeconds':float(np.mean([r['wallSeconds'] for r in runs])),'sampleStdWallSeconds':float(np.std([r['wallSeconds'] for r in runs],ddof=1)),'medianWallSeconds':float(np.median([r['wallSeconds'] for r in runs])),'wallRangeSeconds':[min(r['wallSeconds'] for r in runs),max(r['wallSeconds'] for r in runs)],'batch':read(OUT/(g+'-batch.json'))})
speed=matrix[0]['meanWallSeconds']/matrix[1]['meanWallSeconds'];assert speed>1
near=[];nr={r['postfixIndex']:r for r in read(OUT/'postfix-reference.json')['rows']}
for c in read(OUT/'postfix-self.json')['cells']:
    r=nr[c['postfixIndex']];a,b=[x['resultM5']['Ok'] for x in c['values']]
    error=abs(b/r['referenceM5']-1);screen=error+r['relativeWorkingUncertainty'];assert screen<=.002
    near.append({'postfixIndex':c['postfixIndex'],'refinementChange':abs(a/b-1),'error':error,'referenceWorkingUncertainty':r['relativeWorkingUncertainty'],'screen':screen,'gate':.002,'accepted':True})

def groups(report):
    groups=collections.defaultdict(list)
    for r in report['regions']:groups[(r['source']['path'],r['layer'])].append(r)
    return groups
def key(report):return {r['id']:(r['source']['path'],r['layer']) for r in report['regions']}
def islands(report):
    k=key(report)
    return collections.Counter((tuple(sorted(set(k[i] for i in x['region_ids']))),tuple(x['nets']),x['has_unknown_net']) for x in report['islands'])
def contacts(report):
    k=key(report)
    return collections.Counter((tuple(sorted((k[x['a']],k[x['b']]))),x['kind'],x['reliable'],x['drill_id']) for x in report['contacts'])
def polys(rs):return unary_union([Polygon(r['polygon']['outer'],r['polygon']['holes']) for r in rs])
def boundary_vertices(g):
    parts=list(g.geoms) if hasattr(g,'geoms') else [g]
    rings=[np.array(r.coords) for p in parts for r in [p.exterior,*p.interiors]]
    return np.vstack([r[:-1] for r in rings]),np.vstack([np.stack([r[:-1],r[1:]],axis=1) for r in rings])
def discrete_hausdorff(a,b):
    # Same vertex-to-boundary discrete metric as GEOS default Hausdorff, with
    # segment indexing instead of quadratic all-segment scans on large pours.
    av,ae=boundary_vertices(a);bv,be=boundary_vertices(b)
    d=[]
    for vertices,edges in [(av,be),(bv,ae)]:
        _,distance=shapely.STRtree(shapely.linestrings(edges)).query_nearest(shapely.points(vertices),return_distance=True,all_matches=False)
        d.append(float(distance.max()))
    return max(d)
boards=[]
for fixture in sorted((ROOT/'tests/pcb-analyze').glob('*/fixture.json')):
    name=fixture.parent.name;aa=read(OUT/(name+'-G0-opt.json'));bb=read(OUT/(name+'-G1-opt.json'));a,b=aa['report'],bb['report']
    ga,gb=groups(a),groups(b);assert set(ga)==set(gb);assert a['sources']==b['sources'] and a['outline']==b['outline']
    assert not a['excluded'] and not b['excluded'];assert a['statistics']['shorts']==b['statistics']['shorts']
    ia,ib=islands(a),islands(b);ca,cb=contacts(a),contacts(b)
    changes=[]
    for k in ga:
        p,q=polys(ga[k]),polys(gb[k]);distance=discrete_hausdorff(p,q);difference=p.symmetric_difference(q).area
        allowance=2*max(a['boolean_error_bound_mm'],b['boolean_error_bound_mm'])
        assert distance<=allowance,(name,k,distance,allowance)
        changes.append({'sourcePath':k[0],'layer':k[1],'source':ga[k][0]['source'],'hausdorffMm':distance,'symmetricDifferenceMm2':difference,'beforeRegions':len(ga[k]),'afterRegions':len(gb[k])})
    for g,vv in [('G0',aa),('G1',bb)]:
        rr=vv['report'];vv['counts']={'regions':len(rr['regions']),'islands':len(rr['islands']),'vertices':sum(len(r['polygon']['outer'])+sum(map(len,r['polygon']['holes'])) for r in rr['regions']),'contours':sum(1+len(r['polygon']['holes']) for r in rr['regions'])}
    boards.append({'name':name,'G0Counts':aa['counts'],'G1Counts':bb['counts'],'G0ColdWarmSeconds':aa['coldWarmPreparationSeconds'],'G1ColdWarmSeconds':bb['coldWarmPreparationSeconds'],
                   'wallRatioG1OverG0':sum(bb['coldWarmPreparationSeconds'])/sum(aa['coldWarmPreparationSeconds']),
                   'G0PeakBytes':read(OUT/(name+'-G0-opt-run.json'))['peakBytes'],'G1PeakBytes':read(OUT/(name+'-G1-opt-run.json'))['peakBytes'],
                   'plannedSourceCoordinatePayloadUpperBytes':2_000_000*64+8*(fixture.parent/(name+'.json')).stat().st_size,
                   'memoryPlanMeaning':'Vertex/input payload envelope only; library overlay allocator peak is separately bounded by512MiB outer process monitor, not claimed as a certified full heap bound.',
                   'islandSourceMembershipSame':ia==ib,'removedIslandSignatures':str(ia-ib),'addedIslandSignatures':str(ib-ia),
                   'removedContactClassifications':str(ca-cb),'addedContactClassifications':str(cb-ca),
                   'maxHausdorffMm':max(x['hausdorffMm'] for x in changes),'maxAllowedDifferenceMm':2*max(a['boolean_error_bound_mm'],b['boolean_error_bound_mm']),
                   'sources':changes,'geometryStatus':[a['status'],b['status']],'BooleanBoundsMm':[a['boolean_error_bound_mm'],b['boolean_error_bound_mm']]})
plans=[]
for name in ['STM32G0_CAN_RS485','TPS63060DSCR','BandAmpV5']:
    rows=read(OUT/(name+'-plans.json'))['plans'];assert all('refusal' not in r for r in rows)
    sums={}
    for g in ['G1','G2']:
        p=[r['plan'] for r in rows if r['configuration']==g]
        sums[g]={k:sum(x[k] for x in p) for k in ['nearPairs','farPairs','promotedPairs','plannedBytes','nearCacheUpperBytes','constructionSeconds','moments','blocks']}
    assert sums['G1']['nearPairs']-sums['G2']['nearPairs']==sums['G2']['promotedPairs']
    plans.append({'name':name,**sums,'sameGeometry':True,'physicalFullROIBound':'Unavailable; no constrained ROI operator/solve admitted.'})
result={'boards':boards,'meshCounts':read(OUT/'mesh-counts.json'),'postfixNear':near,'coupon':matrix,'measuredMeanSpeedup':speed,
        'speedupNonoverlapLowerRatio':matrix[0]['wallRangeSeconds'][0]/matrix[1]['wallRangeSeconds'][1],
        'plans':plans,'referenceNature':'Reused independent full physical matrix; empirical reference uncertainty, not certified continuum enclosure.',
        'ROIReady':False,'fullP7Closed':False}
(OUT/'audit.json').write_text(json.dumps(result,indent=2,default=lambda x:x.item()))
print(json.dumps({'speedup':speed,'nearScreens':[r['screen'] for r in near],'fullScreens':[r['runs'][0]['fullScreen'] for r in matrix],'topologySame':[r['islandSourceMembershipSame'] for r in boards],'contactClassChanges':[(r['name'],r['removedContactClassifications'],r['addedContactClassifications']) for r in boards]}))
