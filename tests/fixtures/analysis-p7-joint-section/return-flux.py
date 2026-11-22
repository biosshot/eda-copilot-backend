"""Integrate native recovered tetrahedral J across an interior plane; no solve."""
import json,pathlib,sys,numpy as np
root=pathlib.Path(sys.argv[1] if len(sys.argv)>1 else 'debugging/p7-closure/1793437157149686200');p=json.loads((root/'joint-fine.json').read_text())['report']['prepared'];m=json.loads((root/'joint-fine-magnetic.json').read_text());layers={r[0]:r[3]for r in p['mapping']};cells=[i for i,ids in enumerate(p['sourceIds'])if any(layers.get(v)=='BOTTOM'for v in ids)];cut=(1.000123-p['originMm'][0])*1e-3;flux={"negativeYBypassA":0.,"positiveYBypassA":0.};total_area=0.
for cell,data in zip(cells,m['returnCurrentDensity']):
 v=np.array([p['nodesM'][i]for i in p['copperCells'][cell]]);points=[]
 for a in range(4):
  for b in range(a):
   x,y=v[a,0]-cut,v[b,0]-cut
   if x*y<0:points.append((v[a]+(v[b]-v[a])*x/(x-y))[1:])
 if len(points)<3:continue
 points=np.array(points);center=points.mean(axis=0);points=points[np.argsort(np.arctan2(points[:,1]-center[1],points[:,0]-center[0]))];area=abs(sum(np.cross(points[i],points[(i+1)%len(points)])for i in range(len(points))))/2;y=center[0]*1000+p['originMm'][1];key='negativeYBypassA'if y<-.08 else 'positiveYBypassA';flux[key]+=area*data['realAPerM2'][0];total_area+=area
flux['totalReturnA']=sum(flux.values());flux['areaM2']=total_area;v=m['states'][0]['samples'][1]['portRealV'];flux['electrodeBalanceExpectedA']=-1+(v[0]+v[1])/50;flux['cutXMm']=1.000123;flux['convention']='Recovered volume-candidate real RMS current flux at1kHz, +x normal; negative sign is the physical return direction. Not a mesh certificate.';(pathlib.Path('debugging/p7-d1-replay-final')/('return-flux-'+root.name+'.json')).write_text(json.dumps(flux,indent=2));print(flux)
