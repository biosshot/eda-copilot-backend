"""Supervise one fresh bounded automatic-current packet; preserve all history."""
from pathlib import Path
import hashlib,json,psutil,subprocess,time,sys
ROOT=Path(__file__).resolve().parents[4]
OUT=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else ROOT/'debugging/p7-final-integration';OUT.mkdir(parents=True,exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
exe=ROOT/'native/analyze/target/release/examples/automatic_reduction.exe'
result=OUT/'automatic.json';assert not result.exists(),'never overwrite a numerical packet'
boards={str(p.relative_to(ROOT)):sha(p) for d in (ROOT/'tests/pcb-analyze').iterdir() if d.is_dir() for p in [d/(d.name+'.json')] if p.exists()}
assert len(boards)==8
history={str(p.relative_to(ROOT)):sha(p) for name in ['p7-completion','p6-p7-structural-gauges'] for p in (ROOT/'debugging'/name).rglob('*') if p.is_file()}
sources={str(p.relative_to(ROOT)):sha(p) for name in ['native/analyze/src/extraction/local','native/analyze/src/electrical','tests/fixtures/analysis-local-extraction/final-integration'] for p in (ROOT/name).rglob('*') if p.is_file() and '__pycache__' not in str(p)}
for name in ['native/analyze/examples/automatic_reduction.rs','native/analyze/tests/automatic_reduction.rs','native/analyze/tests/finite_contacts.rs','tests/fixtures/analysis-local-extraction/current-space/fixture.rs']:
 sources[name]=sha(ROOT/name)
start=time.perf_counter();peak=0;refusal=None
with result.open('wb') as out,(OUT/'automatic.stderr.log').open('wb') as err:
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
run={'command':[str(exe)],'binarySha256':sha(exe),'sources':sources,'boardsBeforeAfter':boards,'historyBeforeAfter':history,
 'wallSeconds':time.perf_counter()-start,'observedPeakWorkingSetBytes':peak,'capSeconds':120,'capBytes':512*1024*1024,
 'exitCode':code,'refusal':refusal,'outputSha256':sha(result)}
assert boards=={path:sha(ROOT/path) for path in boards}
assert history=={path:sha(ROOT/path) for path in history}
(OUT/'run.json').write_text(json.dumps(run,indent=2))
assert code==0 and refusal is None,run
print(json.dumps({k:run[k] for k in ['wallSeconds','observedPeakWorkingSetBytes','exitCode']}))
