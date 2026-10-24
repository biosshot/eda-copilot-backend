"""Audit saved magnetic results, independent full energy and cost; no solves."""
from pathlib import Path
import hashlib,json,numpy as np
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent;OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-stability'
build=json.loads((OUT/'current-build.json').read_text());runpath=OUT/('current-run-'+build['binarySha256'][:12]);native=json.loads((runpath/'stdout.json').read_text());reference=json.loads((FIX/'reference.json').read_text());slender=json.loads((FIX/'slender-self-reference.json').read_text());refs={v['label']:v for v in reference['stm']}
sym=lambda a:(np.array(a)+np.array(a).T)/2
def norm(a,b):
 l=np.linalg.inv(np.linalg.cholesky(sym(b)));return float(max(abs(np.linalg.eigvalsh(l@(sym(a)-sym(b))@l.T))))
pair_rows=[]
for p in native['pairs']:
 label=p['label'];row=dict(p)
 if label in refs:
  r=refs[label];mean=r['referenceM5'];uncertainty=r['empiricalAbsoluteUncertaintyM5']
  if label=='stm668-self':mean=slender['referenceM5'];uncertainty=slender['absoluteTotalUncertaintyM5']
  row.update({'independentReferenceM5':mean,'absoluteReferenceUncertaintyM5':uncertainty,'referenceRelativeUncertainty':uncertainty/(mean-uncertainty),'signedRelativeErrors':{},'referenceMarginScreens':{}})
  for direction in ['forwardM5','reverseM5']:
   value=p[direction].get('Ok');assert value is not None,(label,direction,p[direction]);error=(value-mean)/mean;screen=(abs(value-mean)+uncertainty)/(mean-uncertainty);row['signedRelativeErrors'][direction]=error;row['referenceMarginScreens'][direction]=screen;assert screen<=.002,(label,p['xyOrder'],screen)
  row['directedReciprocity']=abs(p['forwardM5']['Ok']-p['reverseM5']['Ok'])/mean
 pair_rows.append(row)
base=next(p for p in native['pairs'] if p['label']=='stm668-556' and p['xyOrder']==12);transforms=[]
for p in native['pairs']:
 if p['label'] not in ['stm-scaled','stm-translated','stm-rotated']:continue
 scale=32 if p['label']=='stm-scaled' else 1
 errors={k:p[k]['Ok']/(base[k]['Ok']*scale)-1 for k in ['forwardM5','reverseM5']};assert max(abs(v) for v in errors.values())<1e-9;transforms.append({'label':p['label'],'relativeErrors':errors})
raw_ref=np.array(reference['operatorLevels'][-1]['rawLH']);previous=np.array(reference['operatorLevels'][-2]['rawLH']);b=np.vstack([np.eye(3),-np.ones(3)]);physical_ref=b.T@raw_ref@b;operator=[]
for level in native['matrices']:
 raw=np.array(level['rawLH']);assert not any(p['refusal'] for p in level['pairCosts']);physical=b.T@raw@b;delta=raw-raw_ref;entry_scale=np.sqrt(np.diag(raw_ref)[:,None]*np.diag(raw_ref)[None,:]);full=norm(raw,raw_ref);phys=norm(physical,physical_ref)
 reciprocity=float(np.max(abs(raw-raw.T)/entry_scale));assert full<=.05 and phys<=.05 and reciprocity<=1e-9
 assert min(np.linalg.eigvalsh(sym(raw)))>0 and min(np.linalg.eigvalsh(sym(physical)))>0
 row={'xyOrder':level['xyOrder'],'zOrder':level['zOrder'],'rawLH':raw.tolist(),'rawReferenceLH':raw_ref.tolist(),'signedRelativeEntryErrors':(delta/abs(raw_ref)).tolist(),'absoluteNearZeroNormalizedErrors':(abs(delta)/entry_scale).tolist(),'fullPositiveRangeError':full,'physicalPositiveRangeError':phys,'rawReciprocity':reciprocity,'fullEigenvaluesH':np.linalg.eigvalsh(sym(raw)).tolist(),'physicalEigenvaluesH':np.linalg.eigvalsh(sym(physical)).tolist(),'physicalReferenceEigenvaluesH':np.linalg.eigvalsh(physical_ref).tolist(),'physicalRawLH':physical.tolist(),'pairCosts':level['pairCosts'],'all64OrientedPrismPairSeconds':sum(p['seconds'] for p in level['pairCosts']),'meanOrientedPrismPairSeconds':sum(p['seconds'] for p in level['pairCosts'])/64}
 row['fullErrorSpectrum']=np.linalg.eigvalsh(np.linalg.inv(np.linalg.cholesky(raw_ref))@(sym(raw)-raw_ref)@np.linalg.inv(np.linalg.cholesky(raw_ref)).T).tolist();operator.append(row)
roi=[]
for p in sorted((OUT.parent/'independent-current').glob('*-supports.json')):
 source=json.loads(p.read_text());foil=len(source['foil']);axial=len(source['axialSubprisms']);pairs=foil*(foil+1)//2+axial*(axial+1)//2
 roi.append({'name':p.stem,'foilSupports':foil,'axialSupports':axial,'packedScalarPairs':pairs,'packedScalarBytes':pairs*8,'oneDirectionCoarseSecondsEstimate':pairs*operator[0]['meanOrientedPrismPairSeconds'],'oneDirectionRefinedSecondsEstimate':pairs*operator[1]['meanOrientedPrismPairSeconds'],'twoDirectionTwoRuleSecondsEstimate':pairs*2*sum(v['meanOrientedPrismPairSeconds'] for v in operator),'sourceSha256':hashlib.sha256(p.read_bytes()).hexdigest()})
resources=[]
for folder in [runpath,*OUT.glob('reference-run-*'),OUT/'legacy-run']:
 if not (folder/'run.json').exists():continue
 r=json.loads((folder/'run.json').read_text());assert r['exitCode']==0 and not r['externalRefusal'] and r['seconds']<=60 and r['peakBytes']<=r['capBytes'];resources.append(r)
reference_change=norm(previous,raw_ref);reference_self_changes=[abs(s[-1]['LH']/s[-2]['LH']-1) for s in reference['selfConvolutionLevels']]
working_reference_uncertainty=2*(reference_change+max(reference_self_changes))+128*np.finfo(float).eps*np.linalg.cond(raw_ref)
report={'gates':{'relativeIntegral':.002,'fullPhysicalEnergy':.05,'reciprocity':1e-9,'absoluteNearZeroEntry':'0.002sqrt(LiiLjj)','newCouponBytes':512*1024**2,'newCouponSeconds':60},'pairs':pair_rows,'transformations':transforms,'operator':operator,'physicalBasisB':b.tolist(),'physicalConservationResidual':float(np.max(abs(np.ones(4)@b))),'independentOperatorReferenceLastChange':reference_change,'independentOperatorReferenceWorkingUncertainty':working_reference_uncertainty,'referenceUncertaintyMeaning':'2*(last mutual operator change+max last self change)+128eps*condition; observed numerical screen, not certified arbitrary geometry bound.','independentSelfConvolutionLastChanges':reference_self_changes,'resources':resources,'roiCostPreflight':roi,'slenderSelfEnclosure':slender,'retainedInsufficientSlenderSelfQMCUncertainty':refs['stm668-self']['empiricalRelativeUncertainty'],'limitations':['Full4x4 partial-volume magnetic kernel and full3D sum(I)=0 current space; no inferred exterior return/joint fields. Not a complete closed-PCB B-field/recovery certificate.','Point-source1e-5 order-change acceptance and conditioning indicator are controls, not universal certified error bounds. Independent accuracy applies to frozen coupons only.','Sobol6SE+2last-change uncertainty is empirical/statistical, not a deterministic bound. Slender self uses a separate finite-width geometry enclosure.','ROI timing is an extrapolation from saved small pair costs, not measured full-ROI performance; no large ROI was run. Dense cost remains a production blocker.','All old production geometry/resource refusals and failed first candidate/reference evidence remain; P7/C/P6/Thermal/DSL/vias not broadened.']}
files=[FIX/'coupon.json',FIX/'stm-668-556.json',FIX/'reference.json',FIX/'slender-self-reference.json',Path(__file__),ROOT/'native/analyze/examples/magnetic_kernel_coupon.rs',ROOT/'native/analyze/tests/magnetic_kernel.rs']
for name,digest in build['sources'].items():p=ROOT/name;assert hashlib.sha256(p.read_bytes()).hexdigest()==digest;files.append(p)
report['sha256']={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files}
(OUT/'audit.json').write_text(json.dumps(report,indent=2));(FIX/'verification.json').write_text(json.dumps(report,indent=2))
print(json.dumps({'fullErrors':[r['fullPositiveRangeError'] for r in operator],'physicalErrors':[r['physicalPositiveRangeError'] for r in operator],'pairMaxScreen':max(max(r['referenceMarginScreens'].values()) for r in pair_rows if 'referenceMarginScreens' in r),'referenceLastChange':report['independentOperatorReferenceLastChange'],'roiCoarseSeconds':{r['name']:r['oneDirectionCoarseSecondsEstimate'] for r in roi}},indent=2))
