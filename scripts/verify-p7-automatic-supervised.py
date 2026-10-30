"""Bounded verification run; explicit limits never change public defaults."""
from pathlib import Path
import argparse, subprocess, time, psutil, json, os, sys
parser=argparse.ArgumentParser()
parser.add_argument('--native-memory-mib',type=int,default=2048)
parser.add_argument('--tree-memory-mib',type=int,default=2048)
parser.add_argument('--seconds',type=int,default=300)
parser.add_argument('--reserve-mib',type=int,default=512)
args=parser.parse_args()
if min(args.tree_memory_mib,args.seconds,args.reserve_mib)<=0 or not 256<=args.native_memory_mib<=8192 or args.seconds>3600 or args.native_memory_mib>args.tree_memory_mib:
 parser.error('native memory256..8192MiB, seconds1..3600, positive tree/reserve and native <= tree required')
root=Path.cwd();folder=root/'debugging/p7-channel-plan'/('automatic-supervised-'+str(time.time_ns()));folder.mkdir(parents=True)
available=psutil.virtual_memory().available;cap=args.tree_memory_mib*1024**2
manifest=dict(status='preflight',availableRamBytes=available,nativeMemoryMiB=args.native_memory_mib,memoryCapBytes=cap,reserveMiB=args.reserve_mib,scenarioDeadlineSeconds=args.seconds,trainingJobs=4,cacheMode='cold-empty-directory',directory=str(folder))
def save():
 (folder/'resume-manifest.json').write_text(json.dumps(manifest,indent=2))
save()
if cap+args.reserve_mib*1024**2>available:
 manifest.update(status='refused-preflight',reason='Insufficient available RAM for tree cap plus host reserve');save();print(json.dumps(manifest));sys.exit(2)
env=os.environ.copy();env.update(PCB_ANALYSIS_TRAINING_JOBS='4',PCB_ANALYSIS_CACHE_DIR=str(folder/'cache'),P7_VERIFY_SECONDS=str(args.seconds),P7_VERIFY_MEMORY_MIB=str(args.native_memory_mib))
command=['node','--import','tsx','scripts/verify-p7-automatic-channel.ts','--four-terminal'];started=time.perf_counter();peak=0;worker_peak=0;refusal=None
manifest.update(status='running',command=command);save()
with (folder/'stdout.txt').open('wb') as so,(folder/'stderr.txt').open('wb') as se:
 p=subprocess.Popen(command,stdout=so,stderr=se,env=env);process=psutil.Process(p.pid)
 while p.poll() is None:
  children=[]
  try:
   children=process.children(recursive=True);rss=[c.memory_info().rss for c in children];peak=max(peak,process.memory_info().rss+sum(rss));worker_peak=max(worker_peak,max(rss,default=0))
  except psutil.NoSuchProcess:pass
  if peak>cap:refusal='process-tree memory cap exceeded'
  if psutil.virtual_memory().available<args.reserve_mib*1024**2:refusal='host available RAM fell below reserve'
  if time.perf_counter()-started>args.seconds+10:refusal='outer lifecycle guard exceeded'
  if refusal:
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
manifest.update(status='finished',wallSeconds=time.perf_counter()-started,processTreePeakRssBytes=peak,maxSingleChildRssBytes=worker_peak,exitCode=code,refusal=refusal,scenario=scenario,scope='Four-terminal actual STM32 via/return candidate; no complete-channel or5percent certificate')
save();(folder/'evidence.json').write_text(json.dumps(manifest,indent=2));print(json.dumps(manifest),flush=True)
sys.exit(1 if code or refusal or not scenario or scenario.get('status')!='complete' else 0)
