"""Fresh compiled-worker direct DSL supervision; no free preparation stage."""
from pathlib import Path
import subprocess,time,json,psutil,sys,os,hashlib
root=Path(__file__).resolve().parents[4]
name,packet,mode=sys.argv[1:4]
code_file=sys.argv[4] if len(sys.argv)>4 else "local-extraction.js"
if code_file not in ["local-extraction.js","section-cache.js"]:raise ValueError("known scenario file")
if name not in ['STM32G0_CAN_RS485','TPS63060DSCR','BandAmpV5'] or mode not in ['cold','cold-retry','restart']:raise ValueError('known fixture/mode')
base=root/'debugging/p6-p7-integration'/packet
out=base/name/mode;out.mkdir(parents=True,exist_ok=False)
env=dict(os.environ,PCB_ANALYSIS_CACHE_DIR=str(base/'cache'))
cmd=['node','--import','tsx',str(Path(__file__).with_name('integration-worker-run.ts')),name,str(out),code_file]
paths=[root/'tests/pcb-analyze'/name/(name+'.json'),root/'tests/pcb-analyze'/name/code_file,root/'dist/run-analysis-dsl.worker.js',*list((root/'native/node-addon').glob('*.node'))]
hashes={str(path.relative_to(root)):hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}
retry=None
if mode=='cold-retry':
 retry=json.loads((base/name/'cold/run.json').read_text());assert retry['refusal']=='wall' and retry['hashes']==hashes
start=time.perf_counter();peak=0;refusal=None
with (out/'stdout.txt').open('wb') as so,(out/'stderr.txt').open('wb') as se:
 p=subprocess.Popen(cmd,cwd=root,env=env,stdout=so,stderr=se);proc=psutil.Process(p.pid)
 while p.poll() is None:
  try:peak=max(peak,sum(x.memory_info().rss for x in [proc,*proc.children(recursive=True)]))
  except psutil.NoSuchProcess:pass
  if peak>2*1024**3 or time.perf_counter()-start>300:
   refusal='memory' if peak>2*1024**3 else 'wall'
   try:family=proc.children(recursive=True)+[proc]
   except psutil.NoSuchProcess:family=[]
   for child in reversed(family):
    try:child.kill()
    except psutil.NoSuchProcess:pass
   break
  time.sleep(.02)
 code=p.wait()
record=dict(command=cmd,wallSeconds=time.perf_counter()-start,peakRssBytes=peak,exitCode=code,refusal=refusal,capSeconds=300,capBytes=2*1024**3,hashes=hashes,inputsChangedDuringRun=any(hashlib.sha256(path.read_bytes()).hexdigest()!=hashes[str(path.relative_to(root))] for path in paths))
if retry:record.update(priorFailedWallSeconds=retry['wallSeconds'],retryReason='One bounded reproducibility retest after observed concurrent workspace cargo ended; unchanged input/core/limits; original refusal retained')
(out/'run.json').write_text(json.dumps(record,indent=2));print(json.dumps(record));sys.exit(0 if code==0 and refusal is None and not record["inputsChangedDuringRun"] else 1)
