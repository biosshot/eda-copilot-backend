"""Inventory the complete frozen geometry corpus without any new export/solve.

There is no accepted candidate: differences below are source-hash invariance,
not a claim that phase A topology/accuracy acceptance has passed.
"""
from pathlib import Path
import collections, hashlib, json, time
from shapely.geometry import Polygon
from shapely import STRtree
ROOT=Path(__file__).resolve().parents[4]
OUT=ROOT/'debugging/p7-design-checkpoint/copper-geometry'
read=lambda p:json.loads(p.read_text(encoding='utf-8'))
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
reports=[];hashes={};start=time.perf_counter()
for fixture in sorted((ROOT/'tests/pcb-analyze').glob('*/fixture.json')):
    name=fixture.parent.name;bp=fixture.parent/(name+'.json');board=read(bp)
    ep=ROOT.parent/'analyze-geometry-validation/debugging/pcb-analyze/validation'/(name+'.json')
    entry=read(ep);sp=Path(entry['output'])/'validation.json';saved=read(sp)
    assert sha(bp)==saved['metadata']['boardSha256']
    hashes.update({str(p):sha(p) for p in [fixture,bp,ep,sp]})
    sol=next(c['result']['solution'] for r in saved['result']['reports'] for c in r['calculations'] if c.get('result',{}).get('kind')=='geometry')
    regions=sol['regions'];g=[Polygon(r['polygon']['outer'],r['polygon']['holes']) for r in regions]
    parent=list(range(len(g)))
    def root(i):
        while parent[i]!=i:parent[i]=parent[parent[i]];i=parent[i]
        return i
    edges=[];mixed=[]
    for layer in sorted(set(r['layer'] for r in regions)):
        ix=[i for i,r in enumerate(regions) if r['layer']==layer];tree=STRtree([g[i] for i in ix])
        for j,i in enumerate(ix):
            for k in tree.query(g[i],predicate='intersects'):
                other=ix[int(k)]
                if other<=i:continue
                if regions[i]['source']['net']==regions[other]['source']['net']:
                    parent[root(i)]=root(other);edges.append([i,other])
                else:mixed.append([i,other])
    counter=collections.Counter(len(t['points']) for owner in ['fixed','editable'] for t in board['copper'][owner]['tracks'])
    reports.append({'name':name,'boardSha256':sha(bp),'snapshotSha256':sha(sp),
                    'regions':len(regions),'sourceObjects':len(set(r['source']['path'] for r in regions)),
                    'sourceIds':[r['source'] for r in regions],
                    'contours':sum(1+len(p['polygon']['holes']) for p in regions),
                    'vertices':sum(len(p['polygon']['outer'])+sum(map(len,p['polygon']['holes'])) for p in regions),
                    'holes':sum(len(p['polygon']['holes']) for p in regions),
                    'invalidSavedPolygons':sum(not p.is_valid for p in g),
                    'sameLayerSameNetContactEdges':len(edges),'contactEdgeRegionIndices':edges,
                    'sameLayerCrossNetContacts':len(mixed),'crossNetEdgeRegionIndices':mixed,
                    'sameLayerSameNetComponents':len(set(root(i) for i in range(len(g)))),
                    'meaning':'Saved F64 polygon intersection graph, excludes through-layer barrel links; not a new native connectivity/DC solve.',
                    'trackPointCounts':dict(sorted(counter.items())),
                    'booleanErrorBoundMm':sol['boolean_error_bound_mm'],
                    'candidateGeometry':None,'afterAAcceptedBaseline':None})
sources={}
for name in ['areas','primitives']:
    p=ROOT/f'native/analyze/src/geometry/{name}.rs';baseline=OUT/(name+'-baseline.rs')
    assert sha(p)==sha(baseline)
    sources[str(p.relative_to(ROOT))]={'baseline':sha(baseline),'current':sha(p),'modified':False}
pre=read(ROOT/'debugging/p7-design-checkpoint/copper-partition/inventory.json')
counts=read(ROOT/'debugging/p7-design-checkpoint/independent-current-counts.json')
v={'status':'BASELINE_ONLY_NO_ACCEPTED_FIX','seconds':time.perf_counter()-start,
   'boards':reports,'inputs':hashes,'nativeSources':sources,'threeRoiBaselineInventory':pre,
   'threeRoiBaselineSupportCounts':counts,'afterA':None,
   'limits':'All frozen board sources inventoried. No modified production geometry exists; topology/near-self acceptance, rotation, differing-width, clipping/drill and conduction regressions remain unexecuted, not passed.'}
(OUT/'corpus-baseline.json').write_text(json.dumps(v,indent=2))
print(json.dumps({'seconds':v['seconds'],'boards':[{k:b[k] for k in ['name','regions','vertices','holes','sameLayerSameNetContactEdges','sameLayerSameNetComponents']} for b in reports]}))
