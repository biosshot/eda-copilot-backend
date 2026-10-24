"""Offline reproducible frozen inputs only; no solver/runtime dependency.
Requires NumPy/SciPy/Shapely versions recorded in manifest. Native tests load
small planar JSON inputs directly; generated volume matrices are never tracked.
"""
from pathlib import Path
import json,hashlib
import numpy as np
import scipy
from scipy.spatial import Delaunay
import shapely
from shapely.geometry import Polygon

def base_plane(n, domain, kind, refine_edge_rings=False):
    rings = [r for r in [0.5, 0.505, 0.52, 0.58, 0.82, 1.78, 4.0, 8.0, 16.0, 32.0, 64.0] if r <= domain]
    assert rings[-1] == domain
    xy = []
    index = {}
    ring_ids = []

    def add(p):
        key = tuple((0.0 if v == 0 else float(v) for v in p))
        if key not in index:
            index[key] = len(xy)
            xy.append(key)
        return index[key]
    for radius in rings:
        ids = []
        ring_n = 2 * n if refine_edge_rings and radius in [0.5, 0.505] else n
        for side in range(4):
            for k in range(ring_n):
                s = -radius + 2 * radius * k / ring_n
                ids.append(add([(s, -radius), (radius, s), (-s, radius), (-radius, -s)][side]))
        ring_ids.append(ids)
    seed_ids = []
    for j in range(1, n):
        for i in range(1, n):
            offset = 0.17 / n if kind == 'TOP' else 0.0
            seed_ids.append(add((-0.5 + i / n + offset, -0.5 + j / n + offset)))
    original_xy = np.array(xy)
    source_ids = ring_ids[0] + seed_ids
    source_tri = Delaunay(original_xy[source_ids]).simplices
    triangles = [[source_ids[int(i)] for i in t] for t in source_tri]
    ownership = ['source-square'] * len(triangles)
    for k in range(1, len(rings)):
        region = Polygon(original_xy[ring_ids[k]], holes=[original_xy[ring_ids[k - 1]]])
        for triangle in shapely.constrained_delaunay_triangles(region).geoms:
            coordinates = list(triangle.exterior.coords)[:-1]
            ids = [index[tuple(p)] for p in coordinates]
            assert len(ids) == 3 and region.covers(triangle)
            triangles.append(ids)
            ownership.append('annulus-' + str(k))
    xy = np.array(xy)
    triangles = np.array(triangles, dtype=np.int32)
    edges = {}
    polygons = []
    worst = 0.0
    source = Polygon(xy[ring_ids[0]])
    for k, t in enumerate(triangles):
        a, b, c = xy[t]
        cross = float(np.cross(b - a, c - a))
        assert cross != 0
        if cross < 0:
            triangles[k] = t = t[[0, 2, 1]]
        worst = max(worst, max((np.linalg.norm(xy[t[i]] - xy[t[(i + 1) % 3]]) ** 2 for i in range(3))) / abs(cross))
        polygon = Polygon(xy[t])
        polygons.append(polygon)
        if ownership[k] == 'source-square':
            assert source.covers(polygon)
        else:
            assert source.intersection(polygon).area == 0
        for i in range(3):
            edge = tuple(sorted((int(t[i]), int(t[(i + 1) % 3]))))
            edges[edge] = edges.get(edge, 0) + 1
    all_ring_edges = [tuple(sorted((ids[k], ids[(k + 1) % len(ids)]))) for ids in ring_ids for k in range(len(ids))]
    assert all((edge in edges for edge in all_ring_edges))
    outer = set((tuple(sorted((ring_ids[-1][k], ring_ids[-1][(k + 1) % len(ring_ids[-1])]))) for k in range(len(ring_ids[-1]))))
    assert all((count == (1 if edge in outer else 2) for edge, count in edges.items()))
    assert set(triangles.ravel()) == set(range(len(xy)))
    assert np.array_equal(xy, original_xy)
    full = Polygon(xy[ring_ids[-1]])
    union = shapely.union_all(polygons)
    area_sum = sum((p.area for p in polygons))
    assert abs(area_sum - full.area) <= full.area * 1e-12
    assert full.symmetric_difference(union).area <= full.area * 1e-14
    assert abs(area_sum - union.area) <= full.area * 1e-12
    result = {'xy': xy, 'triangles': triangles, 'polygons': polygons, 'worstAspectProxy': worst, 'kind': kind, 'allConstraintsExactEdges': True}
    return result

def refine_plane(n, domain, kind, lower=0.82, upper=1.78, base_factory=None, partial_longest=False):
    old = (base_factory or g['plane'])(n, domain, kind)
    xy = list(map(tuple, old['xy']))
    tri = old['triangles']
    chosen = []
    owners = {}
    for i, p in enumerate(old['polygons']):
        r = max((abs(v) for v in p.representative_point().coords[0]))
        selected = lower < r < upper
        if selected:
            chosen.append(i)
        for k in range(3):
            owners.setdefault(tuple(sorted((int(tri[i, k]), int(tri[i, (k + 1) % 3])))), []).append(i)
    selected = set(chosen)
    split = {}
    candidates=[edge for edge,adj in owners.items() if len(adj)==2 and all(i in selected for i in adj)]
    if partial_longest:
        candidates=sorted(candidates,key=lambda e:(-np.linalg.norm(old['xy'][e[1]]-old['xy'][e[0]]),tuple(sorted(map(tuple,old['xy'][list(e)])))))[:8]
    for edge in candidates:
        split[edge]=len(xy)
        xy.append(tuple((old['xy'][edge[0]]+old['xy'][edge[1]])/2))
    assert len(chosen) == 64 and len(split) == (8 if partial_longest else 64)
    triangles = []
    parents = []
    for parent, t in enumerate(tri):
        mids = {tuple(sorted((int(t[k]), int(t[(k + 1) % 3])))): split[tuple(sorted((int(t[k]), int(t[(k + 1) % 3]))))] for k in range(3) if tuple(sorted((int(t[k]), int(t[(k + 1) % 3])))) in split}
        if len(mids) == 0:
            children = [list(t)]
        elif len(mids) == 1:
            e = next(iter(mids))
            a, b = e
            c = next((int(v) for v in t if v not in e))
            p = mids[e]
            children = [[a, p, c], [p, b, c]]
        elif len(mids) == 3:
            a, b, c = map(int, t)
            p = mids[tuple(sorted((a, b)))]
            q = mids[tuple(sorted((b, c)))]
            r = mids[tuple(sorted((c, a)))]
            children = [[a, p, r], [p, b, q], [r, q, c], [p, q, r]]
        elif len(mids) == 2:
            e1, e2 = list(mids)
            s = next(iter(set(e1) & set(e2)))
            a = next((v for v in e1 if v != s))
            b = next((v for v in e2 if v != s))
            p = mids[e1]
            q = mids[e2]
            children = [[s, p, q], [p, a, b], [p, b, q]]
        else:
            raise AssertionError('unexpected split count')
        for child in children:
            triangles.append(child)
            parents.append(parent)
    xy = np.array(xy)
    triangles = np.array(triangles, dtype=np.int32)
    polygons = []
    edges = {}
    worst = 0.0
    nesting_error = 0.0
    for k, t in enumerate(triangles):
        a, b, c = xy[t]
        cross = float(np.cross(b - a, c - a))
        if cross < 0:
            triangles[k] = t = t[[0, 2, 1]]
            cross = -cross
        assert cross > 0
        p = Polygon(xy[t])
        polygons.append(p)
        parent = old['polygons'][parents[k]]
        nesting_error = max(nesting_error, p.difference(parent).area)
        assert p.difference(parent).area < max(p.area * 1e-12, 1e-20)
        worst = max(worst, max((np.linalg.norm(xy[t[i]] - xy[t[(i + 1) % 3]]) ** 2 for i in range(3))) / cross)
        for j in range(3):
            e = tuple(sorted((int(t[j]), int(t[(j + 1) % 3]))))
            edges[e] = edges.get(e, 0) + 1
    assert all((v in [1, 2] for v in edges.values()))
    for edge, count in owners.items():
        if edge not in split:
            assert edge in edges
    assert np.array_equal(xy[:len(old['xy'])], old['xy'])
    assert abs(sum((p.area for p in polygons)) - 4 * domain ** 2) < 1e-10
    assert shapely.union_all(polygons).symmetric_difference(Polygon([(-domain, -domain), (domain, -domain), (domain, domain), (-domain, domain)])).area < 1e-10
    max_affine_error = 0.0
    for t, parent in zip(triangles, parents):
        old_t = old['xy'][tri[parent]]
        basis = np.linalg.inv(np.c_[np.ones(3), old_t])
        weights = np.c_[np.ones(3), xy[t]] @ basis
        max_affine_error = max(max_affine_error, float(np.max(abs(weights @ old_t - xy[t]))))
    assert max_affine_error < 1e-13
    return {'xy': xy, 'triangles': triangles, 'polygons': polygons, 'worstAspectProxy': worst, 'kind': kind, 'allConstraintsExactEdges': True}

def base_zlevels(domain, fine):
    air = [0.005, 0.02, 0.08, 0.32, 1.28, 4.0, 8.0, 16.0]
    air = [v for v in air if v <= domain]
    lower = [-0.0] + [-v for v in air]
    upper = [1.035] + [1.035 + v for v in air]
    core = [0.035] + [0.035 + v for v in [0.005, 0.02, 0.08, 0.24, 0.4825]] + [1.0 - v for v in [0.24, 0.08, 0.02, 0.005]] + [1.0]
    foil0 = [0.0, 0.035]
    foil1 = [1.0, 1.035]
    if fine:
        split = lambda a: sorted(set(a + [(a[i] + a[i + 1]) / 2 for i in range(len(a) - 1)]))
        lower = split(sorted(lower))
        upper = split(sorted(upper))
        core = split(sorted(core))
        foil0 = np.linspace(0, 0.035, 5).tolist()
        foil1 = np.linspace(1.0, 1.035, 5).tolist()
    return sorted(set(lower + upper + core + foil0 + foil1))

g={'plane':base_plane}
ROOT=Path(__file__).resolve().parent

def refined(n,domain,kind,ranges):
 factory=base_plane
 for lower,upper in ranges:
  previous=factory
  factory=lambda n,d,k,p=previous,l=lower,u=upper:refine_plane(n,d,k,lower=l,upper=u,base_factory=p)
 return factory(n,domain,kind)

def generate(name,domain,ranges,shift=0.,three=False,partial=False):
 meshes=[refined(8,domain,kind,ranges) for kind in ['BOTTOM','TOP']]
 if partial:
  previous={kind:mesh for kind,mesh in zip(['BOTTOM','TOP'],meshes)}
  meshes=[refine_plane(8,domain,kind,lower=.58,upper=.82,base_factory=lambda n,d,k:previous[k],partial_longest=True) for kind in ['BOTTOM','TOP']]
 if shift:
  a=meshes[1];mask=np.max(abs(a['xy']),axis=1)<=4.;a['xy'][mask,0]+=shift
 z=sorted(set(base_zlevels(16,True)+([-32.,-24.,25.035,33.035] if domain>=32 else [])+([-64.,-48.,49.035,65.035] if domain>=64 else [])))
 if three:z=sorted(set([v for v in z if v<=0]+[v+k for k in range(3) for v in z if 0<=v<=.035]+[v+k for k in range(2) for v in z if .035<v<1]+[v+1 for v in z if v>=1.035]))
 planes=[int(.5<v<1.5) if three else int(v>.5) for v in z]
 metals=[]
 for k in range(3 if three else 2):
  offset=shift if k==1 else 0.
  metals.append({'source_id':f'finite-foil-{k}','polygon_m':[[(x+offset)*1e-3,y*1e-3] for x,y in [(-.5,-.5),(.5,-.5),(.5,.5),(-.5,.5)]],'z0_m':float(k)*1e-3,'z1_m':(.035+k)*1e-3})
 data={'meshes':[{'kind':'exact','source_id':'frozen-'+kind.lower(),'xy_m':(a['xy']*1e-3).tolist(),'triangles':a['triangles'].tolist()} for a,kind in zip(meshes,['BOTTOM','TOP'])], 'planes':planes,'z_m':[v*1e-3 for v in z], 'xy_bounds_m':[[-domain*1e-3]*2,[domain*1e-3]*2],'metals':metals,'dielectrics':[{'source_id':'explicit-vacuum','z0_m':z[0]*1e-3,'z1_m':z[-1]*1e-3,'epsilon_r':1.}], 'exterior':'InfinityBox'}
 path=ROOT/(name+'.json');path.write_text(json.dumps(data,separators=(',',':')),encoding='utf8')
 return {'case':name,'inputSha256':hashlib.sha256(path.read_bytes()).hexdigest(),'nodes':sum(len(data['meshes'][p]['xy_m']) for p in planes),'xyNodes':[len(a['xy']) for a in meshes],'zPlanes':len(z),'domainHalfWidthMm':domain,'nestedAnnuliMm':ranges,'shiftMm':shift,'conductors':len(metals),'partialLongestInteriorEdges':partial,'partialAddedXYVerticesMm':[a['xy'][-8:].tolist() for a in meshes] if partial else []}

if __name__=='__main__':
 rows=[generate('failed-d32',32,[[.82,1.78],[1.78,4.]]),generate('accepted-d64-domain',64,[[.82,1.78],[1.78,4.]]),generate('accepted-d64-mesh',64,[[.82,1.78],[1.78,4.],[.58,.82]]),generate('shifted-d64',64,[[.82,1.78],[1.78,4.],[.58,.82]],shift=.25),generate('three-d64',64,[[.82,1.78],[1.78,4.]],three=True),generate('shifted-d32-coarse',32,[[.82,1.78],[1.78,4.]],shift=.25),generate('shifted-d64-coarse',64,[[.82,1.78],[1.78,4.]],shift=.25),generate('three-d32',32,[[.82,1.78],[1.78,4.]],three=True),generate('three-d64-mesh',64,[[.82,1.78],[1.78,4.]],three=True,partial=True)]
 (ROOT/'geometry-manifest.json').write_text(json.dumps({'cases':rows,'generatorSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'numpy':np.__version__,'scipy':scipy.__version__,'shapely':shapely.__version__},indent=2))
 print(json.dumps(rows))
