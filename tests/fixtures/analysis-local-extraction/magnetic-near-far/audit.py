"""Saved-values-only full physical energy, field/action, cost and spectral audit."""
import os
os.environ['OPENBLAS_NUM_THREADS']='1'
from pathlib import Path
import json,hashlib
import numpy as np
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent;OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-near-far'
read=lambda p:json.loads(p.read_text())
write=lambda p,v:p.write_text(json.dumps(v,indent=2,default=lambda x:x.item()))
sym=lambda a:(a+a.T)/2
def whitening(a):
 w,v=np.linalg.eigh(sym(a));assert w.min()>0;return (v/np.sqrt(w))@v.T
def norm(a):return float(max(abs(np.linalg.eigvalsh(sym(a)))))
native=read(OUT/'coupon-run/stdout.json');reference=read(FIX/'reference.json');fixture=read(FIX/'coupon.json')
ref=np.array(reference['levels'][-1]['matrixH']);prev=np.array(reference['levels'][-2]['matrixH']);W=whitening(ref)
old=read(FIX.parent/'magnetic-stability/closed-verification.json')
eta=old['referenceWorkingUncertainty']
base=np.array(read(FIX.parent/'magnetic-stability/closed-reference.json')['levels'][-1]['meanLH'])
eb=np.zeros((4,4));eb[:2,:2]=eb[2:,2:]=eta*base
inherited=norm(W@eb@W);crosschange=norm(W@(ref-prev)@W);uncertainty=inherited+2*crosschange+1e-12
dense=np.array(native['denseMatrixH']);kernel=np.array(native['kernelM5']);densities=np.array([c['densities'] for c in fixture['cells']])
refkernel=np.array(reference['levels'][-1]['kernelM5'])
policies=[];prior=None
for p in native['policies']:
 a=np.array(p['matrixH']);b=np.array(p['boundMatrixH']);d=sym(a-dense)
 relative=norm(W@(a-ref)@W);beta=norm(W@b@W)
 action=[]
 for x in p['actions']:
  energy=x['energyJ'];exact=x['dense']['energyJ'];bound=x['absoluteTruncationEnergyBoundJ'];q=np.array(x['coefficients'])
  fielddiff=abs(np.array(x['integratedPotential'])-np.array(x['dense']['integratedPotential']));fieldbound=np.array(x['absoluteFarPotentialBounds'])
  # Empirical dense quadrature and roundoff are reported separately, never included in analytic far bound.
  guard=512*np.finfo(float).eps*max(np.max(abs(np.array(x['dense']['integratedPotential']))),1e-300)
  action.append({'q':q.tolist(),'energyJ':energy,'denseEnergyJ':exact,'observedAbsoluteChangeJ':abs(energy-exact),'analyticTruncationBoundJ':bound,'absoluteBoundScreen':abs(energy-exact)<=bound+512*np.finfo(float).eps*abs(exact),'fieldBoundScreen':bool(np.all(fielddiff<=fieldbound+guard)),'fieldExcessMax':float(np.max(fielddiff-fieldbound)),'matrixActionResidual':float(np.max(abs(np.array(x['response'])-a@q))),'nearZero':abs(exact)<=128*np.finfo(float).eps*np.max(np.diag(dense))*np.dot(q,q)})
 reciprocity=float(np.max(abs(a-a.T))/np.max(abs(a)))
 truncnorm=norm(W@d@W)
 Wa=whitening(a);conditional_beta=norm(Wa@b@Wa)
 conditional_relative=conditional_beta/(1-conditional_beta) if conditional_beta<1 else None
 row=np.array(p['rowsM5']);monotone=True if prior is None else bool(np.all(row<=prior*(1+1e-12)+1e-300))
 prior=row
 policies.append({'theta':p['theta'],'plan':p['plan'],'matrixH':a.tolist(),'analyticBoundMatrixH':b.tolist(),'signedEnergyErrorEigenvalues':np.linalg.eigvalsh(W@sym(a-ref)@W).tolist(),'physicalEigenvaluesH':np.linalg.eigvalsh(sym(a)).tolist(),'fullError':relative,'referenceWorkingUncertainty':uncertainty,'fullScreen':relative+uncertainty,'fullGatePass':relative+uncertainty<=.05,'aggregateFarBudget':beta,'aggregateFarBudgetPass':beta<=.01,'farVsDenseError':truncnorm,'conditionalFarRelativeCertificate':conditional_relative,'conditionalCertificateMeaning':'Analytic truncation only, conditional exact near integrals/moments and F64 arithmetic; near/reference uncertainties remain separate.','reciprocity':reciprocity,'reciprocityPass':reciprocity<=1e-9,'psdPass':bool(np.linalg.eigvalsh(sym(a)).min()>0),'boundMinusDeltaMinH':float(min(np.linalg.eigvalsh(sym(b-d)).min(),np.linalg.eigvalsh(sym(b+d)).min())),'rowsMonotone':monotone,'actions':action,'matrixDifference':(a-ref).tolist(),'farPreparationSeconds':p['farPreparationSeconds']})
assert native['kernelEvaluations']==24**2
for w in native['weakModes']:
 x=w['action'];assert not w['relativeCertificateAvailable']
 if w['epsilon']==0:assert x['energyJ']==0 and x['absoluteTruncationEnergyBoundJ']==0
assert all(p['rowsMonotone'] for p in policies)
assert all(p['boundMinusDeltaMinH']>=-512*np.finfo(float).eps*np.max(np.diag(dense)) for p in policies)
assert all(policies[i+1]['farVsDenseError']<=policies[i]['farVsDenseError'] for i in range(len(policies)-1))
assert not policies[0]['aggregateFarBudgetPass'] and all(p['aggregateFarBudgetPass'] for p in policies[1:])
assert all(p['fullGatePass'] and p['reciprocityPass'] and p['psdPass'] for p in policies)
assert all(all(x['absoluteBoundScreen'] and x['fieldBoundScreen'] for x in p['actions']) for p in policies)
farunit=next(p['farPreparationSeconds']/p['plan']['farPairs'] for p in policies if p['theta']==.4 and p['plan']['farPairs'])
nearunit=native['kernelSeconds']/native['kernelEvaluations']
rois=[]
for r in read(OUT/'plan-run/stdout.json')['rois']:
 for theta in [.7,.4,.2]:
  groups=[g for g in r['groups'] if g['theta']==theta]
  if any('refusal' in g for g in groups):rois.append({'roi':Path(r['source']).stem,'theta':theta,'refusals':groups});continue
  ps=[g['plan'] for g in groups];n=sum(p['supports'] for p in ps);near=sum(p['nearPairs'] for p in ps);far=sum(p['farPairs'] for p in ps)
  rois.append({'roi':Path(r['source']).stem,'theta':theta,'supports':n,'nearPairs':near,'farPairs':far,'blocks':sum(p['blocks'] for p in ps),'nodes':sum(p['nodes'] for p in ps),'constructionSeconds':sum(p['constructionSeconds'] for p in ps),'simultaneousPlansUpperBytes':sum(p['plannedBytes'] for p in ps),'nearCacheUpperBytes':sum(p['nearCacheUpperBytes'] for p in ps),'farPlanStorageBytes':sum(p['blocks']*40 for p in ps),'oneDirectionNearOptimisticSeconds':near*nearunit,'bothDirectionsNearPlusFarSeconds':(2*near-n)*nearunit+far*farunit,'densePackedSameMeasuredSeconds':(near+far)*nearunit,'historicalRefinedPackedSeconds':(near+far)*.0039237515625,'measuredMomentPairSeconds':farunit,'physicalRelativeBound':'Unavailable: no ROI physical current-space operator built or accepted.'})
result={'reference':{'matrixH':ref.tolist(),'eigenvaluesH':np.linalg.eigvalsh(sym(ref)).tolist(),'inheritedSelfUncertainty':inherited,'twiceCrossLastChange':2*crosschange,'workingUncertainty':uncertainty,'nature':'Empirical/statistical, not certified continuum enclosure.'},'dense':{'matrixH':dense.tolist(),'fullError':norm(W@(dense-ref)@W),'rawReciprocity':float(np.max(abs(dense-dense.T))/np.max(abs(dense))),'kernelEvaluations':native['kernelEvaluations'],'kernelSeconds':native['kernelSeconds'],'meanOrderedPairSeconds':nearunit},'policies':policies,'weakModes':native['weakModes'],'rois':rois,'limits':{'maxBytes':512*1024**2,'maxSeconds':60,'exactCacheMaxSupports':64,'fullEnergyGate':.05,'analyticFarBudget':.01,'productionROIAccepted':False,'remainingCost':'Far plan is hierarchical; action still pair-streamed O(N²). No full ROI energy bound or solve admitted.'}}
write(OUT/'audit.json',result);write(FIX/'verification.json',result)
print(json.dumps({'referenceUncertainty':uncertainty,'policies':[{k:p[k] for k in ['theta','fullScreen','aggregateFarBudget','aggregateFarBudgetPass','farVsDenseError']} for p in policies],'roiRows':len(rois)}))
