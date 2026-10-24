from pathlib import Path
import json,hashlib,numpy as np
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint';FIX=ROOT/'tests/fixtures/analysis-local-extraction/layered-native'
sym=lambda a:(np.array(a)+np.array(a).T)/2
def norm(a,b):
 b=sym(b);l=np.linalg.cholesky(b);inv=np.linalg.inv(l)
 return float(np.max(np.abs(np.linalg.eigvalsh(inv@(sym(a)-b)@inv.T))))
expected=json.loads((FIX/'expected.json').read_text());build=json.loads((OUT/'layered-native-probe-build.json').read_text());rows={}
for p in (OUT/'layered-native-runs').glob('*/run.json'):
 r=json.loads(p.read_text())
 if r['binarySha256']!=build['binarySha256']:continue
 if r['inputSha256']!=hashlib.sha256((FIX/(r['case']+'.json')).read_bytes()).hexdigest():continue
 name=r['case']+('-cdt' if r['cdt'] else '');row={**r,'evidence':str(p.relative_to(ROOT))};rows[name]=row
 if r.get('result',{}).get('status')!='completed':continue
 c=r['result']['capacitanceF'];row['resourcePass']=r['seconds']<=60 and r['peakBytes']<=512*1024**2
 if name in expected['cases']:row['sameMeshFullParity']=norm(c,expected['cases'][name]['pythonCapacitanceF'])
 if name.startswith(('accepted','failed')):ref=expected['frozenVacuumRawCapacitanceF']
 else:
  tag='shifted' if name.startswith('shifted') else 'three';refp=FIX/'references'/f'{tag}-32/capacitance-raw-f.json'
  if not refp.exists():continue
  ref=json.loads(refp.read_text())
 row['independentFullError']=norm(c,ref);row['independentAccuracyPass']=row['independentFullError']<=.05
 assert row['resourcePass'],name
 assert all(r['result'][k]<=1e-9 for k in ['freeResidualMax','chargeBalanceMax','workBalanceMax','recoveryMax']),name
 if 'sameMeshFullParity' in row:assert row['sameMeshFullParity']<=1e-10,name
 expected_failure=name in ['failed-d32','shifted-d32-coarse','three-d32']
 assert row['independentAccuracyPass']!=expected_failure,name
for tag,triplet in {'frozen':['failed-d32','accepted-d64-domain','accepted-d64-mesh'],'shifted':['shifted-d32-coarse','shifted-d64-coarse','shifted-d64'],'three':['three-d32','three-d64','three-d64-mesh']}.items():
 if all(n in rows and 'capacitanceF' in rows[n].get('result',{}) for n in triplet):
  a,b,c=[rows[n]['result']['capacitanceF'] for n in triplet];rows[triplet[2]]['separateDomainChange']=norm(a,b);rows[triplet[2]]['separateMeshChange']=norm(b,c)
reference={}
for tag in ['shifted','three']:
 a=FIX/'references'/f'{tag}-16/capacitance-raw-f.json';b=FIX/'references'/f'{tag}-32/capacitance-raw-f.json'
 if a.exists() and b.exists():
  coarse=json.loads(a.read_text());fine=json.loads(b.read_text());delta=norm(coarse,fine)
  reference[tag]={'n16to32FullChange':delta,'rawReferenceReciprocity':float(np.max(np.abs(np.array(fine)-np.array(fine).T))/np.max(np.abs(fine))),'uncertaintyMeaning':'Observed refinement change, not certified continuum bound.'}
  final=rows.get('shifted-d64' if tag=='shifted' else 'three-d64-mesh',{})
  if 'independentFullError' in final:
   final['accuracyPlusObservedReferenceChange']=final['independentFullError']+delta
   final['referenceMarginScreenPass']=final['accuracyPlusObservedReferenceChange']<=.05
report={'rows':rows,'referenceUncertainty':reference,'sourceBuild':build,'gates':{'parity':1e-10,'accuracy':.05,'domainMesh':.02,'reference':.01,'balance':1e-9,'seconds':60,'bytes':512*1024**2},'limitations':['Three-conductor raw accuracy4.90099% is within5%, but observed reference change0.178% exceeds its remaining margin; no continuum5% claim.','Three-conductor mesh control adds only8 longest interior edges/plane in predeclared high-flux annulus; small change is not a global discretization certificate.','Exact source polygons/thickness/z and shifts retained; geometry and finite domain assumptions are explicit, no actual PCB/via claim.','Opaque faer operations are checked before/after one original deadline; external process supervisor supplies hard60s/512MiB acceptance cap.','Internal coupon kernel only; no publicDSL, realboard, via or L acceptance.']}
files=list(FIX.rglob('*'))+[ROOT/p for p in build['sourceSha256']]+[ROOT/'native/analyze/tests/layered_capacitance.rs',ROOT/'native/analyze/tests/local_extraction.rs']
for name,digest in build['sourceSha256'].items():assert hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==digest,name
report['sourceInputReferenceSha256']={str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files if p.is_file() and '__pycache__' not in p.parts}
(OUT/'layered-native-audit.json').write_text(json.dumps(report,indent=2));print(json.dumps({k:{q:v for q,v in r.items() if q in ['sameMeshFullParity','independentFullError','independentAccuracyPass','separateDomainChange','separateMeshChange','resourcePass','externalRefusal']} for k,r in rows.items()},indent=2));print(json.dumps(reference))
