"""Bounded verification run; explicit limits never change public defaults."""
from pathlib import Path
import argparse, subprocess, time, psutil, json, os, sys, math

MIB = 1024**2


def windows_commit_headroom():
 """System available commit, not pagefile free space or available physical RAM."""
 if sys.platform != 'win32':
  raise RuntimeError('OS paging mode requires Windows GetPerformanceInfo commit metrics')
 import ctypes
 from ctypes import wintypes
 class PerformanceInformation(ctypes.Structure):
  _fields_ = [('cb', wintypes.DWORD)] + [(name, ctypes.c_size_t) for name in (
   'CommitTotal', 'CommitLimit', 'CommitPeak', 'PhysicalTotal', 'PhysicalAvailable',
   'SystemCache', 'KernelTotal', 'KernelPaged', 'KernelNonpaged', 'PageSize')] + [
   ('HandleCount', wintypes.DWORD), ('ProcessCount', wintypes.DWORD), ('ThreadCount', wintypes.DWORD)]
 info = PerformanceInformation()
 info.cb = ctypes.sizeof(info)
 api = ctypes.WinDLL('psapi', use_last_error=True).GetPerformanceInfo
 api.argtypes = [ctypes.POINTER(PerformanceInformation), wintypes.DWORD]
 api.restype = wintypes.BOOL
 if not api(ctypes.byref(info), info.cb):
  raise ctypes.WinError(ctypes.get_last_error())
 if not info.PageSize or info.CommitLimit < info.CommitTotal:
  raise RuntimeError('Invalid Windows system commit counters')
 return (info.CommitLimit - info.CommitTotal) * info.PageSize


def memory_refusal(paging, available, headroom, used, cap, reserve, preflight=False):
 if paging:
  if headroom is None:
   return 'Windows available commit metric unavailable'
  if headroom < reserve + (cap if preflight else 0):
   return 'Insufficient Windows commit headroom for tree cap plus reserve' if preflight else 'Windows commit headroom fell below reserve'
 else:
  if available < reserve + (cap if preflight else 0):
   return 'Insufficient available RAM for tree cap plus host reserve' if preflight else 'host available RAM fell below reserve'
 if not preflight and used > cap:
  return 'process-tree private commit cap exceeded' if paging else 'process-tree memory cap exceeded'
 return None


def tree_memory(process, children, paging):
 infos = []
 child_peak = 0
 for index, item in enumerate([process] + children):
  try:
   info = item.memory_info()
   infos.append(info)
   if index:
    child_peak = max(child_peak, info.rss)
  except psutil.NoSuchProcess:
   pass
 rss = sum(info.rss for info in infos)
 private = None
 if paging:
  if any(not hasattr(info, 'private') for info in infos):
   raise RuntimeError('Windows process private committed bytes unavailable; RSS is not commit')
  private = sum(info.private for info in infos)
 return rss, child_peak, private


def main():
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
 parser.add_argument('--allow-os-paging',action='store_true',help='Developer Windows mode: cap tree private committed bytes; guard system commit headroom instead of physical RAM')
 parser.add_argument('--preflight-only',action='store_true',help='Record admission metrics without starting a scenario')
 args=parser.parse_args()
 if not math.isfinite(args.electric_edge_mm) or not 0<=args.electric_edge_mm<=100:parser.error('Electric edge must be finite in [0,100]mm')
 if not math.isfinite(args.reference_area_mm2) or not 0<args.reference_area_mm2<=1:parser.error('Reference area must be finite in (0,1]mm2')
 if args.warm and args.scenario!='segmented-coupon':parser.error('--warm is supported only by the segmented coupon scenario')
 if min(args.tree_memory_mib,args.seconds,args.reserve_mib)<=0 or not 256<=args.native_memory_mib<=8192 or args.seconds>3600 or args.native_memory_mib>args.tree_memory_mib:
  parser.error('native memory256..8192MiB, seconds1..3600, positive tree/reserve and native <= tree required')
 root=Path.cwd();folder=root/'debugging/p7-channel-plan'/('automatic-supervised-'+str(time.time_ns()));folder.mkdir(parents=True)
 available=psutil.virtual_memory().available;cap=args.tree_memory_mib*MIB
 headroom=None;metric_error=None
 if args.allow_os_paging:
  try:
   headroom=windows_commit_headroom()
   tree_memory(psutil.Process(),[],True)
  except (OSError,RuntimeError,psutil.Error) as error:metric_error=str(error);headroom=None
 manifest=dict(status='preflight',availableRamBytes=available,nativeMemoryMiB=args.native_memory_mib,memoryCapBytes=cap,reserveMiB=args.reserve_mib,modelDeadlineSeconds=args.seconds,scenarioDeadlineSeconds=args.seconds*(2 if args.warm else 1),trainingJobs=4,cacheMode='cold-empty-directory',directory=str(folder))
 manifest.update(memoryMode='developer-os-paging' if args.allow_os_paging else 'physical-ram',memoryCapMetric='sum-tree-private-committed-bytes' if args.allow_os_paging else 'sum-tree-rss-bytes',availableCommitBytes=headroom,commitMetricSource='Windows GetPerformanceInfo (CommitLimit-CommitTotal)*PageSize' if args.allow_os_paging else None,commitMetricError=metric_error)
 def save():
  (folder/'resume-manifest.json').write_text(json.dumps(manifest,indent=2))
 save()
 reason=memory_refusal(args.allow_os_paging,available,headroom,0,cap,args.reserve_mib*MIB,preflight=True)
 if reason:
  manifest.update(status='refused-preflight',reason=reason);save();print(json.dumps(manifest));return 2
 if args.preflight_only:
  manifest.update(status='preflight-passed');save();print(json.dumps(manifest));return 0
 cache=Path(args.cache_dir).resolve() if args.cache_dir else folder/'cache'
 if args.cache_dir and (not cache.is_dir() or not any(cache.glob('*.json'))):
  manifest.update(status='refused-preflight',reason='Restart cache must already contain model JSON entries');save();print(json.dumps(manifest));sys.exit(2)
 manifest.update(cacheMode='restart-existing-directory' if args.cache_dir else ('cold+same-worker-warm' if args.warm else 'cold-empty-directory'),cacheDirectory=str(cache),scenario=args.scenario)
 env=os.environ.copy();env.update(PCB_ANALYSIS_TRAINING_JOBS='4',PCB_ANALYSIS_CACHE_DIR=str(cache),P7_OUTPUT_DIR=str(folder),P7_REFERENCE_AREA_MM2=str(args.reference_area_mm2),P7_REFERENCE_ELECTRIC_EDGE_MM=str(args.electric_edge_mm),P7_VERIFY_SECONDS=str(args.seconds),P7_VERIFY_MEMORY_MIB=str(args.native_memory_mib))
 command=['node','--import','tsx',{'via':'scripts/verify-p7-automatic-channel.ts','segmented-coupon':'scripts/verify-p7-segmented-coupon.ts','remote-channel':'scripts/verify-p7-full-channel.ts','segmented-reference':'scripts/verify-p7-segmented-coupon.ts','current-reference':'scripts/verify-p7-full-coupon-reference.ts'}[args.scenario]]
 if args.scenario=='via':command.append('--four-terminal')
 if args.scenario=='segmented-reference':command.append('--reference')
 if args.warm:command.append('--warm')
 started=time.perf_counter();peak=0;worker_peak=0;private_peak=0;refusal=None
 min_available=available;min_headroom=headroom
 manifest.update(status='running',command=command);save()
 with (folder/'stdout.txt').open('wb') as so,(folder/'stderr.txt').open('wb') as se:
  p=subprocess.Popen(command,stdout=so,stderr=se,env=env);process=psutil.Process(p.pid)
  children=[]
  try:
   while p.poll() is None:
    children=[]
    try:
     children=process.children(recursive=True)
     rss,child_rss,private=tree_memory(process,children,args.allow_os_paging)
     peak=max(peak,rss);worker_peak=max(worker_peak,child_rss)
     if private is not None:private_peak=max(private_peak,private)
     available=psutil.virtual_memory().available;min_available=min(min_available,available)
     if args.allow_os_paging:
      headroom=windows_commit_headroom();min_headroom=min(min_headroom,headroom)
     refusal=memory_refusal(args.allow_os_paging,available,headroom,private if args.allow_os_paging else rss,cap,args.reserve_mib*MIB)
    except psutil.NoSuchProcess:pass
    except (OSError,RuntimeError,psutil.Error) as error:
     refusal='memory supervision metric unavailable: '+str(error)
    if time.perf_counter()-started>args.seconds*(2 if args.warm else 1)+10:refusal='outer lifecycle guard exceeded'
    if refusal:break
    time.sleep(.05)
  except KeyboardInterrupt:
   refusal='cancelled by user'
  finally:
   if refusal:
    try:children=process.children(recursive=True)
    except psutil.NoSuchProcess:pass
    for c in reversed(children):
     try:c.kill()
     except psutil.NoSuchProcess:pass
    if p.poll() is None:p.kill()
  code=p.wait()
 scenario=None
 for line in reversed((folder/'stdout.txt').read_text(encoding='utf-8',errors='replace').splitlines()):
  try:
   item=json.loads(line)
   if isinstance(item,dict) and 'status'in item:scenario=item;break
  except json.JSONDecodeError:pass
 manifest.update(status='finished',wallSeconds=time.perf_counter()-started,processTreePeakRssBytes=peak,processTreePeakPrivateCommitBytes=private_peak if args.allow_os_paging else None,minAvailableRamBytes=min_available,minAvailableCommitBytes=min_headroom,maxSingleChildRssBytes=worker_peak,exitCode=code,refusal=refusal,scenario=scenario,scope=args.scenario+'; conditional physical result, no complete5percent certificate')
 save();(folder/'evidence.json').write_text(json.dumps(manifest,indent=2));print(json.dumps(manifest),flush=True)
 return 1 if code or refusal or not scenario or scenario.get('status')!='complete' else 0

if __name__ == "__main__":
 sys.exit(main())
