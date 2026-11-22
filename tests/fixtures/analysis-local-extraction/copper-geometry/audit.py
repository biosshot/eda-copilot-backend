"""Saved geometry only: exact STM613 construction and rejected hull topology.

No native modification, tolerance, kernel evaluation or full ROI solve.
"""
from pathlib import Path
import collections, decimal, hashlib, json, math, sys
import shapely
from shapely.geometry import Polygon, Point, MultiPoint, box
from shapely.ops import unary_union

ROOT = Path(__file__).resolve().parents[4]
FIX = Path(__file__).resolve().parent
OUT = ROOT/'debugging/p7-design-checkpoint/copper-geometry'
OLD = ROOT/'debugging/p7-design-checkpoint'
read = lambda p: json.loads(p.read_text(encoding='utf-8'))
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
rows = {v['id']: v for v in map(json.loads, (OUT/'stages.jsonl').read_text().splitlines())}
frozen = read(FIX/'regressions.json')
tracks = {t['id']: t for t in frozen['tracks']}
sources = {r['source']['source_id']: r for r in frozen['prepared']}
pair = ['88d9932c804b2bb2', 'ddd0394190d6ae65']
tri = read(FIX.parent/'copper-partition/stm613-trace.json')['cell']['triangleMm']
round_int = lambda x: int(x + math.copysign(.5, x))
adapters = []
for sid in pair:
    row, t = rows[sid], tracks[sid]
    a, b = [(p['x'], p['y']) for p in t['points']]
    d = math.dist(a, b)
    x, y = -(b[1]-a[1])/d*t['widthMm']/2, (b[0]-a[0])/d*t['widthMm']/2
    ps = [p for c in row['rawCircles'] for p in c['outer']]
    ps += [[a[0]+x,a[1]+y],[b[0]+x,b[1]+y],[b[0]-x,b[1]-y],[a[0]-x,a[1]-y]]
    lo, hi = [min(p[k] for p in ps) for k in [0,1]], [max(p[k] for p in ps) for k in [0,1]]
    offset = [lo[k]+(hi[k]-lo[k])*.5 for k in [0,1]]
    scale = 2.**(29-round_int(math.log2(max(hi[k]-lo[k] for k in [0,1])*.5)))
    raw = min(ps, key=lambda p: math.dist(p,tri[0]))
    integer = [round_int((p-o)*scale) for p,o in zip(raw,offset)]
    restored = [i/scale+o for i,o in zip(integer,offset)]
    assert restored in row['actual'][0]['outer']
    adapters.append({'id':sid,'boundsMm':[lo,hi],'offsetMm':offset,'scale':scale,
                     'rawCapVertexMm':raw,'integer':integer,'restoredMm':restored})
assert adapters[0]['rawCapVertexMm'] == adapters[1]['rawCapVertexMm']
assert adapters[0]['restoredMm'] != adapters[1]['restoredMm']

decimal.getcontext().prec = 100
D = decimal.Decimal.from_float
cross = lambda u,v: u[0]*v[1]-u[1]*v[0]
sub = lambda u,v: [u[0]-v[0],u[1]-v[1]]
intersections = []
for j, expected in [(26,tri[1]),(27,tri[2])]:
    aa,bb = sources[pair[0]]['polygon']['outer'][20:22]
    cc,dd = sources[pair[1]]['polygon']['outer'][j:j+2]
    a,b,c,d = [[D(x) for x in p] for p in [aa,bb,cc,dd]]
    t = cross(sub(c,a),sub(d,c))/cross(sub(b,a),sub(d,c))
    q = [a[k]+t*(b[k]-a[k]) for k in [0,1]]
    assert [float(x) for x in q] == expected
    intersections.append({'facetA':[aa,bb],'facetB':[cc,dd],
                          'decimalIntersectionMm':list(map(str,q)),
                          'roundedIntersectionMm':list(map(float,q)),'savedVertexMm':expected,'ulpResidual':0})
polys = [Polygon(sources[s]['polygon']['outer']) for s in pair]
union = unary_union(polys)
vertices = list(union.exterior.coords)
assert all(tuple(p) in vertices for p in tri)
mesh = list(shapely.constrained_delaunay_triangles(union).geoms)
exact_triangle = [p for p in mesh if set(list(p.exterior.coords)[:3]) == set(map(tuple,tri))]
assert len(exact_triangle) == 1
replays = [{'id':sid,'exactCoordinates':rows[sid]['actual'][0] == sources[sid]['polygon'],
            'symmetricDifferenceMm2':Polygon(rows[sid]['actual'][0]['outer']).symmetric_difference(Polygon(sources[sid]['polygon']['outer'])).area}
           for sid in tracks]
assert all(v['exactCoordinates'] and v['symmetricDifferenceMm2']==0 for v in replays)

# A real narrow pad bridge, positioned at the oblique straight broadside.
sid = pair[1]
t = tracks[sid]
a,b = [(p['x'],p['y']) for p in t['points']]
d = math.dist(a,b)
normal = (-(b[1]-a[1])/d,(b[0]-a[0])/d)
m = ((a[0]+b[0])/2,(a[1]+b[1])/2)
p = [m[k]+normal[k]*t['widthMm']/2 for k in [0,1]]
pad = box(p[0]-.0001,p[1]-.0001,p[0]+.0001,p[1]+.0001)
baseline, hull = Polygon(rows[sid]['actual'][0]['outer']), Polygon(rows[sid]['experimentalHull']['outer'])
assert baseline.intersection(pad).area>0 and hull.distance(pad)>0
bridge = {'sourceId':sid,'padSourceId':'authored-regression-pad','net':t['net'],
          'padPolygonMm':list(pad.exterior.coords)[:-1],'widthMm':.0002,
          'nativeOverlapAreaMm2':baseline.intersection(pad).area,'hullGapMm':hull.distance(pad),
          'broadsideSetbackMm':hull.distance(Point(p)),
          'conclusion':'Reject circle-only convex hull: loses genuine bridge. It is not an accepted geometry fix.'}
# Preserve exact straight-side rectangle corners as well as canonical cap facets.
# This is a useful local alternative, not a production Boolean replacement.
rect_hulls={}
for sid in pair:
    t=tracks[sid];a,b=[(p['x'],p['y']) for p in t['points']];d=math.dist(a,b)
    x,y=-(b[1]-a[1])/d*t['widthMm']/2,(b[0]-a[0])/d*t['widthMm']/2
    ps=[p for c in rows[sid]['rawCircles'] for p in c['outer']]
    ps += [[a[0]+x,a[1]+y],[b[0]+x,b[1]+y],[b[0]-x,b[1]-y],[a[0]-x,a[1]-y]]
    rect_hulls[sid]=MultiPoint(ps).convex_hull
rect_union=unary_union(list(rect_hulls.values()));rect_coords=list(rect_union.exterior.coords)
assert rect_hulls[pair[1]].intersection(pad).area>0
rect_prototype={'scope':'GEOS experimental hull of raw native endpoint circles plus exact straight rectangle; two-point tracks only, no Boolean re-rounding/clipping/drills',
                'polygonsMm':{sid:list(p.exterior.coords)[:-1] for sid,p in rect_hulls.items()},
                'bridgeOverlapMm2':rect_hulls[pair[1]].intersection(pad).area,
                'unionMinEdgeMm':min(math.dist(a,b) for a,b in zip(rect_coords,rect_coords[1:])),
                'accepted':False,'limitations':'General polyline union, different-width joints, clipping/drills and repeated overlays are unresolved. No new native prism/accuracy gate run.'}
audit = {'status':'A_BLOCKED_B_NOT_STARTED','kernelEvaluations':0,'nativeProductionModified':False,
         'replays':replays,'adapters':adapters,'intersections':intersections,
         'twoSourceUnionContainsAllThreeVertices':True,'twoSourceCdtReproduces613':True,
         'shortestEdgeMm':min(math.dist(tri[k],tri[(k+1)%3]) for k in range(3)),
         'hullUnionMinEdgeMm':min(math.dist(a,b) for a,b in zip(list(unary_union([Polygon(rows[s]['experimentalHull']['outer']) for s in pair]).exterior.coords),list(unary_union([Polygon(rows[s]['experimentalHull']['outer']) for s in pair]).exterior.coords)[1:])),
         'rejectedHullBridge':bridge,'rectanglePreservingPrototype':rect_prototype,'originalPrismSourceFailure':'Retained actual/instrumented refusal at12/8 and24/16 through positive order63; no retry/no zero contribution.',
         'contractProposal':'A source-preserving common overlay coordinate context across stroke assembly, clipping/drills and layer union; explicitly define unresolved-contact policy and bound before implementation. Do not add epsilon/snap/filter silently.'}
(OUT/'audit.json').write_text(json.dumps(audit,indent=2))
(FIX/'construction-613.json').write_text(json.dumps({'adapters':adapters,'intersections':intersections,'bridge':bridge},indent=2))
print(json.dumps({k:audit[k] for k in ['status','kernelEvaluations','shortestEdgeMm','hullUnionMinEdgeMm']}))
