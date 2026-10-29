"""Source-bound recovered maps, with explicit candidate/application limits."""
from pathlib import Path
import json,math,numpy as np
import matplotlib;matplotlib.use('Agg')
import matplotlib.pyplot as plt
from matplotlib.collections import PolyCollection
from matplotlib.colors import LogNorm
root=Path('debugging/p7-channel-plan');data=json.loads((root/'stm32-candidate-map.json').read_text());r=data['report'];p=r['prepared'];sample=r['samples'][0];nodes=np.array(p['nodesM']);origin=np.array(p['originMm']);groups={}
for i,indices in enumerate(p['copperCells']):
 v=nodes[indices];z=v[:,2].mean()
 if not 0<=z<=35e-6:continue
 xy=tuple(sorted(set(tuple(x) for x in v[:,:2])))
 if len(xy)!=3:raise RuntimeError('Expected a conforming prism footprint')
 volume=abs(np.linalg.det((v[1:]-v[0]).T))/6
 current=np.array(sample['currentRealImaginaryA'][3*i:3*i+3])*p['cellDensityPerAmpM2'][i];j2=float((current**2).sum())
 a=groups.setdefault(xy,[0.,0.,0.]);a[0]+=volume;a[1]+=volume*j2;a[2]+=sample['cellLossW'][i]
polygons=[];values=[[],[]]
for xy,(volume,j2,power) in groups.items():
 points=np.array(xy)*1000+origin;centre=points.mean(axis=0);order=np.argsort(np.arctan2(points[:,1]-centre[1],points[:,0]-centre[0]));polygons.append(points[order]);values[0].append(math.sqrt(j2/volume));values[1].append(power/volume)
fig,axes=plt.subplots(1,2,figsize=(14,6.8));fig.subplots_adjust(top=.82,bottom=.2,left=.06,right=.94,wspace=.3)
for ax,field,title,unit in zip(axes,values,['Recovered thickness RMS current density','Recovered mean Joule density'],['A/m² per 1 A RMS drive','W/m³ per (1 A RMS drive)²']):
 field=np.array(field);upper=field.max();lower=upper*1e-6;collection=PolyCollection(polygons,array=np.ma.masked_less(field,lower),cmap='inferno',norm=LogNorm(lower,upper),edgecolors='none');ax.add_collection(collection);ax.autoscale_view();ax.set_aspect('equal');ax.set_xlabel('Captured board x [mm]');ax.set_ylabel('Captured board y [mm]');ax.set_title(title,fontsize=12);fig.colorbar(collection,ax=ax,shrink=.8,label=unit)
 ax.text(.01,.01,'TOP foil, volume-weighted through thickness',transform=ax.transAxes,fontsize=8,color='#333',bbox={'facecolor':'white','alpha':.85,'edgecolor':'none'})
fig.suptitle('STM32 complete-contact direct-FEM candidate at 10 kHz\nUnit port drive [1,0] A RMS; source copper, barrels and holes retained',fontsize=15)
fig.text(.06,.1,'Conditional fixed level1 discretization: the higher spatial level exceeded 2 GiB. No admitted full channel or 5% field-accuracy claim.',fontsize=10)
fig.text(.06,.062,f"TOP foil loss {sum(v[2] for v in groups.values()):.6g} W; full-domain loss {sample['totalLossW']:.6g} W. Gray/background includes zero or values below the displayed 10⁻⁶ peak scale.",fontsize=9)
fig.text(.06,.027,'Recovered through stored current congruence from the checksum-validated production cache; no new FEM assembly/training for this map.',fontsize=9)
fig.savefig(root/'stm32-candidate-current-loss.png',dpi=150);plt.close(fig)
print(root/'stm32-candidate-current-loss.png')
