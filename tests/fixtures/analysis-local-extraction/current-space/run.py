"""Bounded actual native example, with saved commands, hashes, wall and Windows RSS.
Compilation is separate; never modifies captured boards or native/analyze/debugging.
"""
from pathlib import Path
import subprocess,time,json,hashlib,psutil,sys
ROOT=Path(__file__).resolve().parents[4]
OUT=ROOT/'debugging/p7-completion'
OUT.mkdir(parents=True,exist_ok=True)
result=OUT/'coupon.json';assert not result.exists(),'preserve existing evidence; choose a fresh packet'
exe=ROOT/'native/analyze/target/release/examples/current_space.exe'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
boards={str(p.relative_to(ROOT)):sha(p) for d in (ROOT/'tests/pcb-analyze').iterdir() if d.is_dir() for p in [d/(d.name+'.json')] if p.exists()}
assert len(boards)==8
sources={str(p.relative_to(ROOT)):sha(p) for d in ['native/analyze/src/extraction/local','native/analyze/src/electrical'] for p in (ROOT/d).rglob('*.rs')}
sources['native/analyze/examples/current_space.rs']=sha(ROOT/'native/analyze/examples/current_space.rs')
start=time.perf_counter();peak=0;refusal=None
with result.open('wb') as out,(OUT/'coupon.stderr.log').open('wb') as err:
 p=subprocess.Popen([str(exe)],cwd=ROOT,stdout=out,stderr=err,creationflags=subprocess.CREATE_NO_WINDOW)
 proc=psutil.Process(p.pid)
 while p.poll() is None:
  try:
   m=proc.memory_info();peak=max(peak,m.rss,getattr(m,'peak_wset',m.rss))
  except psutil.NoSuchProcess:pass
  if time.perf_counter()-start>120 or peak>512*1024*1024:
   refusal='wall/RSS cap exceeded';p.kill();break
  time.sleep(.01)
 code=p.wait()
wall=time.perf_counter()-start
assert boards=={path:sha(ROOT/path) for path in boards},'captured board mutation'
run={'command':[str(exe)],'binarySha256':sha(exe),'sources':sources,'boardsBeforeAfter':boards,'wallSeconds':wall,'observedPeakWorkingSetBytes':peak,'capSeconds':120,'capBytes':512*1024*1024,'exitCode':code,'refusal':refusal,'outputSha256':sha(result),'rustc':subprocess.check_output(['rustc','-Vv'],text=True,creationflags=subprocess.CREATE_NO_WINDOW)}
(OUT/'run.json').write_text(json.dumps(run,indent=2))
assert code==0 and refusal is None,run
print(json.dumps({k:run[k] for k in ['wallSeconds','observedPeakWorkingSetBytes','exitCode','outputSha256']}))
