"""One cold direct-DSL run, preserving logs and OS process-tree RSS on refusal."""
from pathlib import Path
import subprocess,time,psutil,json,os,sys,hashlib
root=Path.cwd();folder=root/'debugging/p7-channel-plan'/('automatic-supervised-'+str(time.time_ns()));folder.mkdir(parents=True)
env=os.environ.copy();env['PCB_ANALYSIS_TRAINING_JOBS']='4';env['PCB_ANALYSIS_CACHE_DIR']=str(folder/'cache')
command=['node','--import','tsx','scripts/verify-p7-automatic-channel.ts','--four-terminal'];started=time.perf_counter();peak=0;worker_peak=0;refusal=None
with (folder/'stdout.txt').open('wb') as so,(folder/'stderr.txt').open('wb') as se:
 p=subprocess.Popen(command,stdout=so,stderr=se,env=env);process=psutil.Process(p.pid)
 while p.poll() is None:
  try:
   children=process.children(recursive=True);rss=[c.memory_info().rss for c in children];peak=max(peak,process.memory_info().rss+sum(rss));worker_peak=max(worker_peak,max(rss,default=0))
  except psutil.NoSuchProcess:pass
  if peak>2*1024**3:
   refusal='process-tree memory exceeded2GiB'
   for c in reversed(children):
    try:c.kill()
    except psutil.NoSuchProcess:pass
   p.kill();break
  # Public scenario owns the shared300s deadline; this outer cap only covers termination.
  if time.perf_counter()-started>310:
   refusal='outer lifecycle guard310s after public300s deadline'
   for c in reversed(children):
    try:c.kill()
    except psutil.NoSuchProcess:pass
   p.kill();break
  time.sleep(.05)
 code=p.wait()
scenario=None
for line in reversed((folder/'stdout.txt').read_text(encoding='utf-8',errors='replace').splitlines()):
 try:
  item=json.loads(line)
  if isinstance(item,dict) and 'status'in item:scenario=item;break
 except json.JSONDecodeError:pass
evidence=dict(command=command,wallSeconds=time.perf_counter()-started,processTreePeakRssBytes=peak,maxSingleChildRssBytes=worker_peak,exitCode=code,refusal=refusal,trainingJobs=4,cacheDirectory=str(folder/'cache'),memoryCapBytes=2*1024**3,scenarioDeadlineSeconds=300,scenarioStatus=scenario.get('status') if scenario else 'unavailable',scenarioErrorCode=(scenario.get('error') or {}).get('code') if scenario else None,scope='Full cold no-scope four-terminal actual STM32 channel (selected scenario recorded in command); conditional canonicalS50/Cu-loss validation, no global5percent claim')
(folder/'evidence.json').write_text(json.dumps(evidence,indent=2));print(json.dumps(dict(directory=str(folder),**evidence)),flush=True);sys.exit(1 if code or refusal or not scenario or scenario.get('status')!='complete' else 0)
