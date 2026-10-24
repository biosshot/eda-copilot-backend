"""Saved native/Python coordinate transport audit, allowing vertex relabeling."""
import itertools,json
import numpy as np
from pathlib import Path
from trace import OLD,OUT,read
records=[]
for plan in [json.loads(l) for l in (OUT/'plans.jsonl').read_text().splitlines()]:
 name=Path(plan['source']).stem.removesuffix('-supports');rows=read(OLD/'independent-current'/f'{name}-supports.json')[plan['family']]
 expected=np.array([r['triangleMm'] for r in rows])*1e-3;actual=np.array([c['triangleM'] for c in plan['cells']]);perms=list(itertools.permutations(range(3)))
 costs=np.stack([np.max(abs(actual[:,p,:]-expected),axis=(1,2)) for p in perms],axis=1);choice=costs.argmin(axis=1)
 aligned=np.stack([actual[i,perms[int(j)],:] for i,j in enumerate(choice)])
 delta=abs(aligned-expected);changed=np.flatnonzero(choice!=0)
 def twice(t):return (t[:,1,0]-t[:,0,0])*(t[:,2,1]-t[:,0,1])-(t[:,1,1]-t[:,0,1])*(t[:,2,0]-t[:,0,0])
 cross1=twice(expected);cross2=twice(aligned)
 records.append({'fixture':name,'family':plan['family'],'changedCoordinateCount':int(np.count_nonzero(delta)),'totalCoordinates':int(delta.size),'maxAbsoluteSIReadDeltaM':float(delta.max()),'vertexRelabelingIndices':changed.tolist(),'orientationSignChangesInOriginalLabeling':np.flatnonzero(cross1*cross2<0).tolist(),'minimumAbsoluteAreaRatioNativeToPython':float(min(abs(cross2/cross1))),'maximumAbsoluteAreaRatioNativeToPython':float(max(abs(cross2/cross1))),'meaning':'Native Rust reader/scaling vs Python reader/scaling, vertex relabeling aligned. Census uses actual native membership; no parser change, tolerance relaxation or coordinate snap.'})
(OUT/'coordinate-transport.json').write_text(json.dumps(records,indent=2));print(json.dumps(records))
