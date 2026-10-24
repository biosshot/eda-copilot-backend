"""Witness for the single failed worst-aspect self sample; no kernel retry."""
import math,json
from pathlib import Path
from shapely.geometry import Point,shape
from trace import ROOT,FIX,OUT,OLD,read,load,geometry,contours
board,pre,sol,_=load('STM32G0_CAN_RS485');cell=read(OLD/'independent-current/STM32G0_CAN_RS485-supports.json')['foil'][613];tri=cell['triangleMm'];ids=['88d9932c804b2bb2','ddd0394190d6ae65'];layer=shape(read(OLD/'taxonomy/STM32G0_CAN_RS485-layer-contours.json')['TOP']);vertices={tuple(p) for ring in contours(layer) for p in ring}
rows=[]
for r in sol['regions']:
 if r['source'].get('source_id') in ids:
  vs={tuple(p) for ring in [r['polygon']['outer'],*r['polygon']['holes']] for p in ring}
  rows.append({'source':r['source'],'polygon':r['polygon'],'originalVertexMembers':[tuple(p) in vs for p in tri],'boundaryDistancesMm':[Point(p).distance(geometry(r).boundary) for p in tri]})
v={'supportIndex':613,'cell':cell,'normalizedTracks':[t for t in board['copper']['fixed']['tracks'] if t['id'] in ids],'preparedOwners':rows,'allVerticesOnSavedLayerUnion':all(tuple(p) in vertices for p in tri),'shortestEdgeMm':min(math.dist(tri[i],tri[(i+1)%3]) for i in range(3)),'coordinateUlpsMm':[math.ulp(x) for x in tri[0]],'failure':'Both12/8 and24/16 refuse positive source convergence through order63 at1e-5; no retry/unchecked fallback.','conclusion':'Near-duplicate boundary vertices already present in saved same-layer union before CDT. Shared CAD endpoint is exact. Complete per-operation construction history for this extra witness is not reconstructed; no area/ULP-based removal is authorized.'}
(OUT/'stm613-trace.json').write_text(json.dumps(v,indent=2));(FIX/'stm613-trace.json').write_text(json.dumps(v,indent=2));print(json.dumps({k:v[k] for k in ['shortestEdgeMm','coordinateUlpsMm','allVerticesOnSavedLayerUnion']}))
