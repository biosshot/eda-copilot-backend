"""Bounded verification run; explicit limits never change public defaults."""
from pathlib import Path
import argparse, subprocess, time, psutil, json, os, sys, math
parser=argparse.ArgumentParser()
parser.add_argument('--scenario',choices=['via','segmented-coupon','segmented-reference','current-reference','remote-channel'],default='via')
parser.add_argument('--electric-edge-mm',type=float,default=0)
parser.add_argument('--reference-area-mm2',type=float,default=.1)
parser.add_argument('--warm',action='store_true')
parser.add_argument('--cache-dir')
parser.add_argument('--native-memory-mib',type=int,default=2048)
parser.add_argument('--tree-memory-mib',type=int,default=2048)
parser.add_argument('--seconds',type=int,default=300)
parser.add_argument('--reserve-mib',type=int,default=512)
args=parser.parse_args()
if not math.isfinite(args.electric_edge_mm) or not 0<=args.electric_edge_mm<=100:parser.error('Electric edge must be finite in [0,100]mm')
if not math.isfinite(args.reference_area_mm2) or not 0<args.reference_area_mm2<=1:parser.error('Reference area must be finite in (0,1]mm2')
if args.warm and args.scenario!='segmented-coupon':parser.error('--warm is supported only by the segmented coupon scenario')
if min(args.tree_memory_mib,args.seconds,args.reserve_mib)<=0 or not 256<=args.native_memory_mib<=8192 or args.seconds>3600 or args.native_memory_mib>args.tree_memory_mib:
 parser.error('native memory256..8192MiB, seconds1..3600, positive tree/reserve and native <= tree required')
root=Path.cwd();folder=root/'debugging/p7-channel-plan'/('automatic-supervised-'+str(time.time_ns()));folder.mkdir(parents=True)
available=psutil.virtual_memory().available;cap=args.tree_memory_mib*1024**2
manifest=dict(status='preflight',availableRamBytes=available,nativeMemoryMiB=args.native_memory_mib,memoryCapBytes=cap,reserveMiB=args.reserve_mib,modelDeadlineSeconds=args.seconds,scenarioDeadlineSeconds=args.seconds*(2 if args.warm else 1),trainingJobs=4,cacheMode='cold-empty-directory',directory=str(folder))
def save():
 (folder/'resume-manifest.json').write_text(json.dumps(manifest,indent=2))
save()
if cap+args.reserve_mib*1024**2>available:
 manifest.update(status='refused-preflight',reason='Insufficient available RAM for tree cap plus host reserve');save();print(json.dumps(manifest));sys.exit(2)
cache=Path(args.cache_dir).resolve() if args.cache_dir else folder/'cache'
if args.cache_dir and (not cache.is_dir() or not any(cache.glob('*.json'))):
 manifest.update(status='refused-preflight',reason='Restart cache must already contain model JSON entries');save();print(json.dumps(manifest));sys.exit(2)
manifest.update(cacheMode='restart-existing-directory' if args.cache_dir else ('cold+same-worker-warm' if args.warm else 'cold-empty-directory'),cacheDirectory=str(cache),scenario=args.scenario)
env=os.environ.copy();env.update(PCB_ANALYSIS_TRAINING_JOBS='4',PCB_ANALYSIS_CACHE_DIR=str(cache),P7_OUTPUT_DIR=str(folder),P7_REFERENCE_AREA_MM2=str(args.reference_area_mm2),P7_REFERENCE_ELECTRIC_EDGE_MM=str(args.electric_edge_mm),P7_VERIFY_SECONDS=str(args.seconds),P7_VERIFY_MEMORY_MIB=str(args.native_memory_mib))
command=['node','--import','tsx',{'via':'scripts/verify-p7-automatic-channel.ts','segmented-coupon':'scripts/verify-p7-segmented-coupon.ts','remote-channel':'scripts/verify-p7-full-channel.ts','segmented-reference':'scripts/verify-p7-segmented-coupon.ts','current-reference':'scripts/verify-p7-full-coupon-reference.ts'}[args.scenario]]
if args.scenario=='via':command.append('--four-terminal')
if args.scenario=='segmented-reference':command.append('--reference')
if args.warm:command.append('--warm')
started=time.perf_counter();peak=0;worker_peak=0;refusal=None
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
  if time.perf_counter()-started>args.seconds*(2 if args.warm else 1)+10:refusal='outer lifecycle guard exceeded'
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
manifest.update(status='finished',wallSeconds=time.perf_counter()-started,processTreePeakRssBytes=peak,maxSingleChildRssBytes=worker_peak,exitCode=code,refusal=refusal,scenario=scenario,scope=args.scenario+'; conditional physical result, no complete5percent certificate')
save();(folder/'evidence.json').write_text(json.dumps(manifest,indent=2));print(json.dumps(manifest),flush=True)
sys.exit(1 if code or refusal or not scenario or scenario.get('status')!='complete' else 0)
