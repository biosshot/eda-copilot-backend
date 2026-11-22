"""Build actual-source isolated probes or one supervised run; no native API."""
from pathlib import Path
import sys,json,hashlib,subprocess,time,psutil
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-stability';FIX=Path(__file__).resolve().parent
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
mode=sys.argv[1]
if mode=='build':
 for name in ['prism','polyhedron']:
  legacy=OUT/f'legacy-{name}.rs'
  if not legacy.exists():
   with legacy.open('wb') as stream:subprocess.run(['git','show',f'afe8ba3:native/analyze/src/extraction/local/{name}.rs'],cwd=ROOT,stdout=stream,check=True)
 deps=ROOT/'native/analyze/target/debug/deps';libraries={}
 for name in ['serde_json']:
  files=list(deps.glob('lib'+name+'-*.rlib'));assert len(files)==1;libraries[name]=files[0]
 flags=['--edition=2021','-O','-C','codegen-units=1','-L','dependency='+str(deps)]
 for name,p in libraries.items():flags+=['--extern',name+'='+str(p)]
 for label in ['legacy','current']:
  source=(ROOT/'native/analyze/examples/magnetic_kernel_coupon.rs').read_text()
  sources={}
  for name in ['prism','polyhedron']:
   path=OUT/f'legacy-{name}.rs' if label=='legacy' else ROOT/f'native/analyze/src/extraction/local/{name}.rs'
   source=source.replace('../src/extraction/local/'+name+'.rs',str(path).replace('\\','/'));sources[str(path.relative_to(ROOT))]=sha(path)
  probe=OUT/(label+'-probe.rs');probe.write_text(source);binary=OUT/(label+'-probe.exe');start=time.perf_counter()
  previous=OUT/(label+'-build.json')
  if previous.exists() and binary.exists():
   old=json.loads(previous.read_text())
   if old['sources']==sources and old['probeSha256']==sha(probe) and old['binarySha256']==sha(binary):print(json.dumps({'reusedBuild':label}));continue
  subprocess.run(['rustc',*flags,str(probe),'-o',str(binary)],check=True,timeout=60)
  row={'sources':sources,'probeSha256':sha(probe),'binarySha256':sha(binary),'flags':flags,'compiler':subprocess.check_output(['rustc','--version']).decode().strip(),'compileSeconds':time.perf_counter()-start}
  (OUT/(label+'-build.json')).write_text(json.dumps(row,indent=2));print(json.dumps(row))
 sys.exit(0)
assert mode in ['legacy','current','reference'];binary=(FIX/'independent-reference.py') if mode=='reference' else OUT/(mode+'-probe.exe');destination=OUT/(mode+'-run-'+sha(binary)[:12]);destination.mkdir(exist_ok=True)
assert not (destination/'run.json').exists(),'no automatic repeat'
pair_only=mode=='legacy';cmd=([sys.executable,str(binary)] if mode=='reference' else [str(binary),str(FIX/'coupon.json')]+(['--pair-only'] if pair_only else []))
limit=(128 if pair_only else 512)*1024**2;start=time.perf_counter();peak=0;refusal=None
with (destination/'stdout.json').open('wb') as out,(destination/'stderr.log').open('wb') as err:
 p=subprocess.Popen(cmd,stdout=out,stderr=err);monitor=psutil.Process(p.pid)
 while p.poll() is None:
  try:peak=max(peak,monitor.memory_info().peak_wset)
  except psutil.Error:pass
  if peak>limit or time.perf_counter()-start>60:refusal='memory' if peak>limit else '60s';p.kill();p.wait();break
  time.sleep(.01)
row={'mode':mode,'command':cmd,'seconds':time.perf_counter()-start,'peakBytes':peak,'capBytes':limit,'capSeconds':60,'exitCode':p.returncode,'externalRefusal':refusal,'binarySha256':sha(binary),'inputSha256':sha(FIX/'coupon.json'),'rawSha256':sha(destination/'stdout.json')}
(destination/'run.json').write_text(json.dumps(row,indent=2));print(json.dumps(row));sys.exit(0 if p.returncode==0 else 2)
