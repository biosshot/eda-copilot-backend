"""One externally bounded native run per frozen input/binary, preserve evidence."""
from pathlib import Path
import sys,subprocess,time,json,hashlib,psutil
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint'
binary=OUT/'layered-capacitance-coupon.exe'
case=sys.argv[1];cdt='--cdt' in sys.argv
inp=ROOT/'tests/fixtures/analysis-local-extraction/layered-native'/(case+'.json')
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
identity={'inputSha256':sha(inp),'binarySha256':sha(binary),'cdt':cdt}
folder=OUT/'layered-native-runs'/(case+('-cdt' if cdt else '')+'-'+identity['binarySha256'][:12]+'-'+identity['inputSha256'][:8]);folder.mkdir(parents=True,exist_ok=True)
meta=folder/'run.json'
if meta.exists():
 old=json.loads(meta.read_text());assert all(old[k]==v for k,v in identity.items());print(json.dumps({'cachedEvidence':str(meta),'exitCode':old['exitCode']}));sys.exit(0)
start=time.perf_counter();peak=0;reason=None
cmd=[str(binary),str(inp)]+(['--cdt'] if cdt else [])
with (folder/'stdout.log').open('wb') as out,(folder/'stderr.log').open('wb') as err:
 p=subprocess.Popen(cmd,cwd=ROOT,stdout=out,stderr=err);monitor=psutil.Process(p.pid)
 while p.poll() is None:
  try:
   processes=[monitor]+monitor.children(recursive=True)
   peak=max(peak,sum(x.memory_info().peak_wset for x in processes))
  except psutil.Error:pass
  elapsed=time.perf_counter()-start
  if peak>512*1024**2 or elapsed>60:
   reason='memory512MiB' if peak>512*1024**2 else 'wall60s'
   for child in monitor.children(recursive=True):child.kill()
   p.kill();p.wait();break
  time.sleep(.01)
row={**identity,'case':case,'command':cmd,'seconds':time.perf_counter()-start,'peakBytes':peak,'exitCode':p.returncode,'externalRefusal':reason}
try:row['result']=json.loads((folder/'stdout.log').read_text())
except (json.JSONDecodeError,UnicodeDecodeError):pass
for name in ['stdout.log','stderr.log']:row[name+'Sha256']=sha(folder/name)
meta.write_text(json.dumps(row,indent=2));summary={k:v for k,v in row.items() if k not in ['result','command']};summary['result']={k:v for k,v in row.get('result',{}).items() if k!='fieldSlice'};print(json.dumps(summary))
sys.exit(0 if p.returncode==0 and reason is None else 2)
