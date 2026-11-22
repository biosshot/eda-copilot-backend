"""Optimized real-source coupon probe; cached deps only, no profile change."""
from pathlib import Path
import hashlib,json,subprocess,time,re
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint';deps=ROOT/'native/analyze/target/debug/deps';flags=['--edition=2021','-O','-C','codegen-units=1','-L','dependency='+str(deps)];libraries={}
for name in ['faer','geo','spade','rstar','serde','serde_json']:
 matches=list(deps.glob('lib'+name+'-*.rlib'));assert len(matches)==1,(name,matches);libraries[name]=matches[0];flags+=['--extern',name+'='+str(matches[0])]
source=ROOT/'native/analyze/examples/layered_capacitance_coupon.rs';binary=OUT/'layered-capacitance-coupon.exe';OUT.mkdir(parents=True,exist_ok=True)
actual=re.search(r'pub const EPS0: f64 = ([^;]+)',(ROOT/'native/analyze/src/extraction/mod.rs').read_text()).group(1)
assert float(actual)==8.8541878128e-12
start=time.perf_counter();subprocess.run(['rustc',*flags,str(source),'-o',str(binary)],check=True,timeout=60)
files=list((ROOT/'native/analyze/src/extraction/local').glob('*.rs'))+[source,ROOT/'native/analyze/src/extraction/mod.rs']
manifest={'flags':flags,'rustc':subprocess.check_output(['rustc','--version']).decode().strip(),'compileSeconds':time.perf_counter()-start,'librarySha256':{name:hashlib.sha256(p.read_bytes()).hexdigest() for name,p in libraries.items()},'sourceSha256':{str(p.relative_to(ROOT)):hashlib.sha256(p.read_bytes()).hexdigest() for p in files},'binarySha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'scope':'Actual production source inclusion, cached pinned deps; no replacement field solver or crate/profile/default GPU changes.'}
(OUT/'layered-native-probe-build.json').write_text(json.dumps(manifest,indent=2));print(json.dumps(manifest))
