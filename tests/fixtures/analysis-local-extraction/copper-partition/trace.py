"""Read-only provenance witnesses from immutable CAD/preparation/support snapshots."""
from pathlib import Path
import json,hashlib,math,decimal
import shapely
from shapely.geometry import Polygon,Point,box,shape,mapping
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent;OUT=ROOT/'debugging/p7-design-checkpoint/copper-partition';OLD=ROOT/'debugging/p7-design-checkpoint'
NAMES=['STM32G0_CAN_RS485','TPS63060DSCR','BandAmpV5']
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
read=lambda p:json.loads(p.read_text(encoding='utf-8'))
def load(name):
 board_path=ROOT/f'tests/pcb-analyze/{name}/{name}.json';pre_path=OLD/f'{name}-whole-barrel-preflight.json';pre=read(pre_path)
 entry_path=ROOT.parent/f'analyze-geometry-validation/debugging/pcb-analyze/validation/{name}.json';entry=read(entry_path);saved_path=Path(entry['output'])/'validation.json';saved=read(saved_path)
 assert sha(board_path)==pre['boardSha256']==saved['metadata']['boardSha256'];assert sha(saved_path)==pre['acceptedPreparationSha256']
 sol=next(c['result']['solution'] for r in saved['result']['reports'] for c in r['calculations'] if c.get('result',{}).get('kind')=='geometry')
 paths=[board_path,pre_path,entry_path,saved_path,ROOT/f'tests/pcb-analyze/{name}/source-pcb.txt',ROOT/f'tests/pcb-analyze/{name}/fixture.json',OLD/'taxonomy'/f'{name}-layer-contours.json',OLD/'independent-current'/f'{name}-supports.json']
 return read(board_path),pre,sol,{str(p):sha(p) for p in paths}
def geometry(r):return Polygon(r['polygon']['outer'],r['polygon'].get('holes',[]))
def contours(g):
 if g.geom_type=='Polygon':return [list(g.exterior.coords)[:-1],*[list(r.coords)[:-1] for r in g.interiors]]
 return [ring for p in g.geoms for ring in contours(p)]
def quantiles(x):
 import numpy as np
 a=np.array(x);return {str(q):float(np.quantile(a,q)) for q in [0,.01,.1,.5,.9,.99,1]}
def main():
 rows=[];hashes={};originals={}
 for name in NAMES:
  board,pre,sol,paths=load(name);hashes.update(paths);roi=box(*pre['scopeMm']);regions=[r for r in sol['regions'] if geometry(r).intersects(roi)]
  layers={k:shape(v) for k,v in read(OLD/'taxonomy'/f'{name}-layer-contours.json').items()}
  selected=[{'regionId':r['id'],'source':r['source'],'approximationMm':r['approximation_mm'],'polygon':r['polygon']} for r in regions];originals[name]=selected
  rings=[c for r in regions for c in [r['polygon']['outer'],*r['polygon'].get('holes',[])]]
  rows.append({'name':name,'scopeMm':pre['scopeMm'],'sourceObjectCount':len(set(r['source']['path'] for r in regions)),'fullBoardObjectCounts':{'pads':len(board['pads']),**{owner+'_'+kind:len(items) for owner in ['fixed','editable'] for kind,items in board['copper'][owner].items()}},'preparedRegionCount':len(regions),'preparedContourCount':len(rings),'preparedVertexCount':sum(map(len,rings)),'unionContourCount':sum(len(contours(g)) for g in layers.values()),'unionVertexCount':sum(sum(map(len,contours(g))) for g in layers.values()),'unionAreaMm2':{k:g.area for k,g in layers.items()},'booleanErrorBoundMm':sol['boolean_error_bound_mm'],'sourceApproximationMm':quantiles([r['approximation_mm'] for r in regions]),'sourceIds':[r['source'] for r in regions],'normalizationMeaning':'Immutable routing Board DTO + native per-source prepared polygons. Counts include entire intersecting source contours; clipped union counts are separate.'})
 name=NAMES[0];board,pre,sol,_=load(name);ids=['54abddda771675b4','bef1aae594813e2c','5ff3f2845c577b5b'];prepared={r['source']['source_id']:r for r in sol['regions'] if r['source'].get('source_id') in ids}
 tracks=[t for t in board['copper']['fixed']['tracks'] if t['id'] in ids];tangent=[-3.4541,12.425]
 rawlines=[]
 for line in (ROOT/f'tests/pcb-analyze/{name}/source-pcb.txt').read_text(encoding='utf-8').splitlines():
  if any('"'+i+'"' in line for i in ids):
   a,b=line.split('||');rawlines.append({'header':json.loads(a),'native':json.loads(b.rstrip('|'))})
 assert all(any(p['x']==-3.7041 and p['y']==12.425 for p in t['points']) and t['widthMm']==.5 for t in tracks)
 source_witness=[]
 for identifier in ids:
  r=prepared[identifier];p=geometry(r);vertices=r['polygon']['outer'];i=max(range(len(vertices)),key=lambda i:vertices[i][0]);v=vertices[i]
  source_witness.append({'source':r['source'],'regionId':r['id'],'rightmost':v,'differenceFromAuthoredTangentMm':[v[d]-tangent[d] for d in range(2)],'nearTangentVertices':[vertices[j%len(vertices)] for j in [i-1,i,i+1]],'polygon':r['polygon']})
 support=read(OLD/'independent-current'/f'{name}-supports.json')['foil'];witnesses=[{'index':i,**support[i]} for i in [668,556]]
 layer=shape(read(OLD/'taxonomy'/f'{name}-layer-contours.json')['TOP']);vertices=[v for c in contours(layer) for v in c];boundary_near=[list(v) for v in vertices if Point(v).distance(Point(tangent))<1e-7]
 for w in witnesses:
  w['boundaryVerticesExact']=[tuple(v) in vertices for v in w['triangleMm']]
  xy=w['triangleMm'];D=decimal.Decimal.from_float;decimal.getcontext().prec=80
  a,b,c=[[D(x) for x in p] for p in xy];area=abs((b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0]))/2
  w['exactF64CoordinateAreaMm2']=str(area);w['shortestEdgeMm']=min(math.dist(xy[i],xy[(i+1)%3]) for i in range(3));w['shortestEdgeInCoordinateUlps']=w['shortestEdgeMm']/max(math.ulp(x) for p in xy for x in p)
 horizontal=next(r for r in source_witness if r['source']['source_id']==ids[2]);segments=[];D=decimal.Decimal.from_float
 for v in [horizontal['nearTangentVertices'][0],horizontal['nearTangentVertices'][2]]:
  p=horizontal['rightmost'];x=D(tangent[0]);y=D(p[1])+(x-D(p[0]))*(D(v[1])-D(p[1]))/(D(v[0])-D(p[0]))
  nearest=min(boundary_near,key=lambda q:abs(q[1]-float(y)))
  segments.append({'segment':[p,v],'verticalBoundaryXmm':tangent[0],'exactF64LineIntersectionYmm':str(y),'nearestSavedUnionVertex':nearest,'differenceUlps':abs(float(y)-nearest[1])/math.ulp(nearest[1])})
 trace={'rawCadLines':rawlines,'normalizedTracks':tracks,'authoredSharedEndpointMm':[-3.7041,12.425],'idealGeneratedCardinalTangentMm':tangent,'preparedPerSource':source_witness,'savedUnionNearBoundaryVertices':boundary_near,'lineIntersectionWitnesses':segments,'supports':witnesses,'stages':[
 {'stage':'CAD and normalized Board','finding':'Three same-net tracks have exactly equal shared endpoint, width0.5mm; ideal radius0.25mm cardinal tangent identical.'},
 {'stage':'Native stroke Boolean union','finding':'Per-source circle/rect stroke union already shifts horizontal rightmostX+3.60608e-10mm and vertical rightmostY+2.78652e-10mm; sources use different local bounded integer adapters. No local drills intersect the witness.'},
 {'stage':'GEOS same-layer clipped union','finding':'Near-coincident cap facets intersect vertical boundary at Y offsets about4.12177e-9mm; exact line-intersection witnesses recover saved union vertices.'},
 {'stage':'Common constraints/CDT','finding':'Triangulation preserves those boundary vertices and produces long skinny668 and tiny556. It does not originate their tiny coordinate differences.'},
 {'stage':'Constant prism candidate current space','finding':'One exact finite35um prism per foil triangle, two XY current-density coefficients/cell before constraints. Census only: no assembled ROI physical current space exists.'}],
 'precisionMeaning':'pm conversion:1mm=1e9pm. F64 ULP, bounded Boolean conversion and CAD export precision are distinct. Preserve existing geometry; this audit is not a snap/filter policy.'}
 (OUT/'source-regions.json').write_text(json.dumps(originals,indent=2));(OUT/'inventory.json').write_text(json.dumps(rows,indent=2));(OUT/'stm-trace.json').write_text(json.dumps(trace,indent=2));(FIX/'stm-trace.json').write_text(json.dumps(trace,indent=2));(OUT/'input-hashes.json').write_text(json.dumps(hashes,indent=2))
 print(json.dumps({'inventories':[{k:r[k] for k in ['name','sourceObjectCount','preparedRegionCount','preparedVertexCount','unionVertexCount']} for r in rows],'intersectionUlps':[r['differenceUlps'] for r in segments]}))
if __name__=='__main__':main()
