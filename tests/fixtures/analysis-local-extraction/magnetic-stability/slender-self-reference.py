"""Finite-width enclosure for the unresolved extreme STM self reference.
Uses exact double-z kernel and triangle marginal density, not native potentials.
"""
from pathlib import Path
import json,time,hashlib
import numpy as np,mpmath as mp
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent;OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-stability';start=time.perf_counter()
assert not (OUT/'slender-self-reference.json').exists(),'reuse saved independent enclosure'
p=json.loads((FIX/'coupon.json').read_text())['stmPair'][0];tri=np.array(p['triangleM']);edge=min(range(3),key=lambda i:np.linalg.norm(tri[(i+1)%3]-tri[i]));tri=np.roll(tri,-edge,axis=0)
a,b,c=tri;base=b-a;w=float(np.linalg.norm(base));length=float(np.linalg.norm(a-c+base/2));area=float(abs(np.cross(base,c-a))/2);h=p['zM'][1]-p['zM'][0]
levels=[]
for digits in [50,80]:
 mp.mp.dps=digits;A,H,L,W=map(mp.mpf,[area,h,length,w])
 def integrand(t):
  if t==0:return mp.mpf('0')
  v=t*t;rho=L*v;k=H/rho;kernel=2*H*(mp.asinh(k)-k/(mp.sqrt(1+k*k)+1));density=mp.mpf(8)/3-4*v+mp.mpf(4)/3*v**3
  return 2*t*density*kernel
 value=A*A*mp.quad(integrand,[0,mp.mpf('.01'),mp.mpf('.1'),1]);levels.append({'digits':digits,'collapsedReferenceM5':str(value)})
eps=2*W/L;density_max=mp.mpf(8)/3
near_collapsed=A*A*density_max*2*H*(eps*mp.asinh(H/(L*eps))+(H/L)*mp.asinh(L*eps/H))
source_area_bound=8*A*W/L;radius=mp.sqrt(source_area_bound/mp.pi)
disk_point_bound=2*mp.pi*radius**2*(mp.asinh(H/radius)+H/(mp.sqrt(H*H+radius*radius)+H))
near_finite=A*H*disk_point_bound
far_difference=A*A*density_max*4*H*W/L*mp.log(1/eps)
geometry_bound=near_collapsed+near_finite+far_difference
numeric_change=abs(mp.mpf(levels[-1]['collapsedReferenceM5'])-mp.mpf(levels[-2]['collapsedReferenceM5']))
total_bound=geometry_bound+2*numeric_change
result={'method':'Independent exact-double-z1D triangle marginal convolution with finite-width transport enclosure','geometry':{'areaM2':area,'baseWidthM':w,'medianLengthM':length,'thicknessM':h},'derivation':['Uniform triangle marginal2r gives pair distance density g(v)=8/3-4v+4v^3/3.','Each transverse displacement <=w/2; pair radial perturbation<=w. Split v at epsilon=2w/L.','Far: |Hprime(rho)|<=2h/rho and rho>=Lv/2 give difference<=4hw/(Lv); g<=8/3.','Near finite source strip has XYarea<=8Aw/L. Symmetric decreasing rearrangement bounds its point potential by equal-area disk with source-z upper kernel2asinh(h/rho). Multiply by target volumeAh.','Near collapsed integral uses g<=8/3 and H(rho)<=2h*asinh(h/rho).','Finite-width geometry is not discarded: its entire change is included in the enclosure. MP50/80 numeric change recorded separately; this is not a native geometry replacement.'],'levels':levels,'referenceM5':float(value),'absoluteGeometryEnclosureM5':float(geometry_bound),'absoluteNumericalChangeM5':float(numeric_change),'absoluteTotalUncertaintyM5':float(total_bound),'relativeUncertaintyToLowerEndpoint':float(total_bound/(value-total_bound)),'lowerM5':float(value-total_bound),'upperM5':float(value+total_bound),'boundComponentsM5':{'nearCollapsed':float(near_collapsed),'nearFinite':float(near_finite),'farDifference':float(far_difference)},'seconds':time.perf_counter()-start,'inputSha256':hashlib.sha256((FIX/'coupon.json').read_bytes()).hexdigest(),'scriptSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
(OUT/'slender-self-reference.json').write_text(json.dumps(result,indent=2));(FIX/'slender-self-reference.json').write_text(json.dumps(result,indent=2));print(json.dumps({k:result[k] for k in ['referenceM5','absoluteTotalUncertaintyM5','relativeUncertaintyToLowerEndpoint','seconds']}))
