"""Experimental count-only extrusion, consuming production shared-grid layers.

The existing frozen angular/interface hypothesis is retained. GEOS precision
noding uses exactly the native dyadic grid, not a new tolerance. No ROI solve.
"""
from pathlib import Path
import json,math,runpy,hashlib
import numpy as np
import shapely
from shapely.geometry import Polygon,LineString,shape,box
from shapely.ops import polygonize
ROOT=Path(__file__).resolve().parents[4];OLD=ROOT/'debugging/p7-design-checkpoint';OUT=OLD/'copper-context'
source=(OLD/'independent-current-counts.py').read_text();helpers={"__file__":str(OLD/'independent-current-counts.py')};exec(source.split('reports=[]')[0],helpers)
describe=helpers['describe'];partition=helpers['partition_rings'];bores=helpers['helper']['bores'];circle=helpers['helper']['circle']
read=lambda p:json.loads(p.read_text());snapshots=lambda ps:shapely.union_all([Polygon(p['outer'],p['holes']) for p in ps])
rows=[];witnesses=[]
for name in ['STM32G0_CAN_RS485','TPS63060DSCR','BandAmpV5']:
    native=read(OUT/(name+'-G1-opt.json'));pre=read(OLD/(name+'-whole-barrel-preflight.json'))
    board=read(ROOT/f'tests/pcb-analyze/{name}/{name}.json');ps,metadata=native['roi'];origin,step,error=metadata
    precision=lambda g:shapely.set_precision(g,step)
    layers={k:precision(snapshots(v)) for k,v in ps.items()}
    field=precision(box(*pre['fieldBoundsMm']));roi=precision(box(*pre['scopeMm']));common=[field.boundary]
    outline=Polygon([(p['x'],p['y']) for p in board['outline']], [[(p['x'],p['y']) for p in r] for r in board.get('cutouts',[])])
    common.append(precision(outline).boundary.intersection(field));barrels=[]
    for bore in bores(board):
        inner=precision(circle(bore['x'],bore['y'],bore['r']));outer=precision(circle(bore['x'],bore['y'],bore['r']+.025))
        if bore['plated'] and outer.intersects(roi):
            points,_=partition(bore)
            # Identical analytic registry; shared integer context at its boundary.
            integer=lambda v:np.copysign(np.floor(abs(v/step)+.5),v)*step
            points=[(integer(a),integer(b)) for a,b in points];barrels.append((bore,points))
            common.extend([LineString([*(tuple(a) for a,b in points),tuple(points[0][0])]),LineString([*(tuple(b) for a,b in points),tuple(points[0][1])])])
        elif inner.intersects(field):common.append(inner.boundary.intersection(field))
    foil=[];stats={}
    for layer in ['BOTTOM','TOP']:
        faces=list(polygonize(shapely.union_all(common+[layers[layer].boundary],grid_size=step)))
        tris=[t for f in faces if field.covers(f.representative_point()) for t in shapely.constrained_delaunay_triangles(f).geoms if layers[layer].covers(t.representative_point())]
        cells=describe(tris,[0.,.035] if layer=='BOTTOM' else [1.565,1.6],layer);foil+=cells
        area=sum(t.area for t in tris);difference=abs(area-layers[layer].area)
        assert difference<=20*step*(layers[layer].length+1), (name,layer,difference)
        stats[layer]={'triangles':len(tris),'potentialNodes':len({tuple(x) for c in cells for x in c['triangleMm']}),'contours':sum(1+len(p.interiors) for p in (layers[layer].geoms if hasattr(layers[layer],'geoms') else [layers[layer]])),
                      'vertices':sum(len(p.exterior.coords)-1+sum(len(r.coords)-1 for r in p.interiors) for p in (layers[layer].geoms if hasattr(layers[layer],'geoms') else [layers[layer]])),
                      'areaMm2':layers[layer].area,'meshAreaDifferenceMm2':difference}
    axial=[]
    for bore,points in barrels:
        for k,(ai,a) in enumerate(points):
            bi,b=points[(k+1)%len(points)];cc=describe([Polygon([a,b,bi]),Polygon([a,bi,ai])],[.035,1.565],'AXIAL');
            for c in cc:c['sectorId']=str(bore['id'])+':'+str(k)
            axial+=cc
    supports={'foil':foil,'axialSubprisms':axial};(OUT/(name+'-supports.json')).write_text(json.dumps(supports,allow_nan=False))
    rows.append({'name':name,'nativeContext':metadata,'layers':stats,'foil':len(foil),'axial':len(axial),'candidateCurrentCoefficients':2*len(foil)+len(axial)//2,'potentialNodes':sum(v['potentialNodes'] for v in stats.values()),'foilTrianglesPerMm2':len(foil)/sum(v['areaMm2'] for v in stats.values()),
                 'maxAspect':max(c['aspectProxy'] or float('inf') for c in foil),'minimumEdgeMm':min(math.dist(c['triangleMm'][k],c['triangleMm'][(k+1)%3]) for c in foil for k in range(3)),
                 'meaning':'Count-only experimental shared-grid GEOS arrangement/CDT; unchanged angular/plating hypothesis, no ROI physical operator or production mesher admission.'})
    if name=='STM32G0_CAN_RS485':
        original=read(OLD/'independent-current/STM32G0_CAN_RS485-supports.json')['foil']
        for i in [613,668,556]:
            centroid=np.mean(original[i]['triangleMm'],axis=0)
            from shapely.geometry import Point
            candidates=[(j,c) for j,c in enumerate(foil) if c['kind']=='TOP' and Polygon(c['triangleMm']).covers(Point(centroid))]
            covers=bool(candidates)
            if not candidates:
                candidates=[(j,c) for j,c in enumerate(foil) if c['kind']=='TOP']
                candidates=[min(candidates,key=lambda x:(Polygon(x[1]['triangleMm']).distance(Point(centroid)),math.dist(np.mean(x[1]['triangleMm'],axis=0),centroid),x[0]))]
            j,c=min(candidates,key=lambda x:x[0]);gap=Polygon(c['triangleMm']).distance(Point(centroid))
            assert gap<=native['report']['boolean_error_bound_mm']
            witnesses.append({'originalIndex':i,'postfixIndex':j,'oldCentroidInsideNewSupport':covers,'nearestDistanceMm':gap,'cell':c})
(OUT/'mesh-counts.json').write_text(json.dumps(rows,indent=2));(OUT/'postfix-witnesses.json').write_text(json.dumps(witnesses,indent=2))
print(json.dumps(rows))
