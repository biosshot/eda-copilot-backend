"""Run already compiled optimized native tests sequentially under hard caps.
Build separately with CARGO_BUILD_JOBS=1. No debug corpus/reference replay.
"""
from pathlib import Path
import subprocess,time,json,re,hashlib,psutil
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-completion'
log=(OUT/'native-final-regression-build.log').read_text(encoding='utf-8')
assert 'Finished' in log
bins=re.findall(r'Executable tests[\\/]([^ .]+)\.rs \(([^)]+\.exe)\)',log)
assert len(bins)==13,len(bins)
results=[]
for name,binary in bins:
 exe=ROOT/binary;assert exe.exists(),exe
 cmd=[str(exe),'--test-threads=1','--skip','saved_boards_conduction_corpus','--skip','separated_object_growth_candidates_and_corpus']
 start=time.perf_counter();peak=0;refusal=None
 with (OUT/(name+'.test.log')).open('wb') as stream:
  p=subprocess.Popen(cmd,cwd=ROOT,stdout=stream,stderr=subprocess.STDOUT,creationflags=subprocess.CREATE_NO_WINDOW);proc=psutil.Process(p.pid)
  while p.poll() is None:
   try:
    m=proc.memory_info();peak=max(peak,m.rss,getattr(m,'peak_wset',m.rss))
   except psutil.NoSuchProcess:pass
   if time.perf_counter()-start>120 or peak>512*1024*1024:
    refusal='120s/512MiB batch cap';p.kill();break
   time.sleep(.01)
  code=p.wait()
 text=(OUT/(name+'.test.log')).read_text(encoding='utf-8');counts=re.findall(r'(\d+) passed; (\d+) failed; (\d+) ignored;.*?(\d+) filtered out',text)
 result={'name':name,'command':cmd,'binarySha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'wallSeconds':time.perf_counter()-start,'observedPeakWorkingSetBytes':peak,'exitCode':code,'refusal':refusal,'counts':list(map(int,counts[-1])) if counts else None}
 results.append(result);(OUT/'native-regressions.json').write_text(json.dumps(results,indent=2));print(json.dumps(result),flush=True)
assert all(r['exitCode']==0 and r['refusal'] is None for r in results)
