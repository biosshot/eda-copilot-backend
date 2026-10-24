"""Full closed-current magnetic energy audit from saved native/reference data."""
from pathlib import Path
import hashlib,json,numpy as np
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent;OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-stability/closed';case=json.loads((FIX/'closed-coupon.json').read_text());native=json.loads((OUT/'native-run/stdout.json').read_text());ref=json.loads((FIX/'closed-reference.json').read_text());b=np.array(ref['levels'][-1]['meanLH']);sym=lambda x:(np.array(x)+np.array(x).T)/2;inv=np.linalg.inv(np.linalg.cholesky(sym(b)))
def norm(x):return float(max(abs(np.linalg.eigvalsh(inv@(sym(x)-sym(b))@inv.T))))
last=norm(ref['levels'][-2]['meanLH']);scatter=max(norm(r['rawLH']) for r in ref['levels'][-1]['scrambles']);uncertainty=2*last+6*scatter/np.sqrt(4*3);assert uncertainty<=case['gates']['referenceWorkingUncertainty']
rows=[];levels=native['levels'].copy()
if (OUT/'control-stdout.json').exists():levels+=json.loads((OUT/'control-stdout.json').read_text())['levels']
for level in levels:
 raw=np.array(level['rawLH']);error=norm(raw);eigen=np.linalg.eigvalsh(sym(raw));reciprocity=float(max(abs(raw-raw.T).flat)/max(abs(raw).flat));assert error+uncertainty<=.05 and min(eigen)>0
 assert all('Ok' in p['result'] for p in level['allOrderedPairCosts']);assert len(level['allOrderedPairCosts'])==144
 row={**level,'fullPositiveRangeEnergyError':error,'referenceMarginScreen':error+uncertainty,'referenceWorkingUncertainty':uncertainty,'signedEntryRelativeErrors':((raw-b)/abs(b)).tolist(),'eigenvaluesH':eigen.tolist(),'referenceEigenvaluesH':np.linalg.eigvalsh(sym(b)).tolist(),'energyErrorSpectrum':np.linalg.eigvalsh(inv@(sym(raw)-sym(b))@inv.T).tolist(),'reciprocity':reciprocity,'reciprocityPass':reciprocity<=1e-9,'allOrderedPairSeconds':sum(p['seconds'] for p in level['allOrderedPairCosts']),'physicalDimension':2};rows.append(row)
resources=[]
for kind in ['native','reference']:
 r=json.loads((OUT/(kind+'-run')/'run.json').read_text());assert r['exitCode']==0 and not r['externalRefusal'] and r['peakBytes']<=512*1024**2 and r['seconds']<=60;resources.append(r)
if (OUT/'control-run.json').exists():
 r=json.loads((OUT/'control-run.json').read_text());assert r['exitCode']==0 and not r['externalRefusal'] and r['peakBytes']<=512*1024**2 and r['seconds']<=60;resources.append(r)
assert case['normalFluxJumpOrExteriorFluxMaxA']<1e-12
assert rows[-1]['reciprocityPass']
report={'physicalCurrentSpace':'All linear combinations of two continuousP1 streamfunction hats. Interior/exterior normal flux check and Jz=0 imply compactly supported distributionally divergence-free3D currents; no ideal external closure or hidden ground.','basisDimension':2,'sourcePrisms':12,'normalFluxJumpOrExteriorFluxMaxA':case['normalFluxJumpOrExteriorFluxMaxA'],'referenceMethod':ref['independentMethod'],'referenceLastFullChange':last,'referenceMaxFullScrambleDeviation':scatter,'referenceWorkingUncertainty':uncertainty,'referenceUncertaintyMeaning':'2*last full-matrix change+6*max full-matrix scramble deviation/sqrt(4*3), empirical/statistical screen, not deterministic continuum bound.','rawReferenceLH':b.tolist(),'levels':rows,'resources':resources,'preflight':case['preflight'],'scope':'Full physical magnetic operator of the frozen finite closed-current basis, all self/mutual terms and all current directions. Not arbitrary PCB/ROI/mesh-convergence admission.'}
paths=[FIX/'closed-coupon.json',FIX/'closed-reference.json',FIX/'closed-probe.rs',FIX/'closed-coupon.py',Path(__file__),OUT/'build.json']
for name in ['control-preflight.json','control-run.json','probe-control.rs','driver-before-control.py']:
 if (OUT/name).exists():paths.append(OUT/name)
driver=OUT/'driver-before-control.py' if (OUT/'driver-before-control.py').exists() else FIX/'closed-coupon.py'
assert hashlib.sha256(driver.read_bytes()).hexdigest()==ref['driverSha256']
build=json.loads((OUT/'build.json').read_text())
for name,digest in build['sources'].items():p=ROOT/name;assert hashlib.sha256(p.read_bytes()).hexdigest()==digest;paths.append(p)
report['sha256']={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
(OUT/'audit.json').write_text(json.dumps(report,indent=2));(FIX/'closed-verification.json').write_text(json.dumps(report,indent=2));print(json.dumps({'errors':[v['fullPositiveRangeEnergyError'] for v in rows],'workingReferenceUncertainty':uncertainty,'marginScreens':[v['referenceMarginScreen'] for v in rows],'fluxJump':case['normalFluxJumpOrExteriorFluxMaxA']}))
