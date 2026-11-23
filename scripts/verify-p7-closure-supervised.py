"""Reuse existing supervisor memory policy for the small coupled AC runner."""
import argparse,importlib.util,pathlib,psutil,subprocess,time,os,json,math
root=pathlib.Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('p7_supervisor',root/'scripts/verify-p7-automatic-supervised.py')
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
out=pathlib.Path(os.environ.get('P7_SUPERVISOR_OUTPUT',str(root/'debugging/p7-closure'/str(time.time_ns()))));out.mkdir(parents=True,exist_ok=True)
shared_cache=os.environ.get('P7_SUPERVISOR_CACHE_DIR')
cache_dir=(root/pathlib.Path(shared_cache)).resolve() if shared_cache else (out/'cache').resolve()
cache_mode='explicit shared directory' if shared_cache else 'cold isolated directory'
parser=argparse.ArgumentParser()
parser.add_argument('--memory-mib',type=int,default=1536)
parser.add_argument('--board-id', help='Run one final queue board as a diagnostic; physical accuracy remains unverified')
args=parser.parse_args()
if args.board_id and ('P7_SUPERVISOR_TEST_COMMAND' in os.environ or 'P7_CLOSURE_CONTROL' in os.environ):parser.error('--board-id cannot be combined with another runner selection')
if not 256<=args.memory_mib<=65536:parser.error('--memory-mib must be in 256..65536')
timeout=float(os.environ.get('P7_SUPERVISOR_TEST_TIMEOUT','310' if args.board_id else '145'))
if not math.isfinite(timeout) or timeout<=0:parser.error('supervisor timeout must be finite and positive')
cap=args.memory_mib*1024**2;reserve=1024**3
paging=os.name=='nt'
def commit_headroom():
 return s.windows_commit_headroom() if paging else None
head=commit_headroom()
reason=s.memory_refusal(paging,psutil.virtual_memory().available,head,0,cap,reserve,True)
if reason:
 (out/'supervision.json').write_text(json.dumps(dict(status='resource-failure',reason=reason,cache=cache_mode,cachePath=str(cache_dir))));raise SystemExit(1)
env={**os.environ,'P7_OUTPUT_DIR':str(out),'PCB_ANALYSIS_CACHE_DIR':str(cache_dir),'P7_TREE_CAP_MIB':str(args.memory_mib)}
start=time.monotonic();rss=private=0
with (out/'stdout.txt').open('w') as log:
 runner='scripts/verify-p7-closure-stripline.ts' if os.environ.get('P7_CLOSURE_CONTROL')=='stripline' else 'scripts/verify-p7-closure-baseline.ts'
 command=json.loads(os.environ['P7_SUPERVISOR_TEST_COMMAND']) if 'P7_SUPERVISOR_TEST_COMMAND' in os.environ else (['node','--import','tsx','scripts/verify-p7-queue-board.ts',args.board_id] if args.board_id else ['node','--import','tsx',runner])
 if args.board_id: env['P7_SUPERVISED_BOARD']='1'
 p=subprocess.Popen(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0)); proc=psutil.Process(p.pid)
 while p.poll() is None:
  children=proc.children(recursive=True)
  r,_,v=s.tree_memory(proc,children,paging)
  rss=max(rss,r)
  if paging: private=max(private,v)
  reason=s.memory_refusal(paging,psutil.virtual_memory().available,commit_headroom(),v if paging else r,cap,reserve)
  if time.monotonic()-start>timeout:reason=f'{timeout:g}s supervisor deadline'
  if reason:
   for child in reversed(children):
    try:child.kill()
    except psutil.NoSuchProcess:pass
   p.kill();break
  time.sleep(.02)
 p.wait()
resultpath=out/('board-result.json' if args.board_id else 'stripline-comparison.json' if os.environ.get('P7_CLOSURE_CONTROL')=='stripline' else 'comparison.json')
status='missing-evidence'; validation=None
if resultpath.exists():
 try:validation=json.loads(resultpath.read_text());status=('diagnostic-complete' if args.board_id and validation.get('status')=='complete' else validation.get('status') if validation.get('status') in ['pass','unsupported','timeout','resource-failure','missing-evidence'] else 'validation-failure')
 except (ValueError,OSError):status='missing-evidence'
if reason:status='timeout' if 'deadline' in reason else 'resource-failure'
elif p.returncode!=0 and status in ('pass','diagnostic-complete'):status='child-failure'
elif p.returncode!=0 and status=='missing-evidence':status='child-failure'
e=dict(exitCode=p.returncode,status=status,reason=reason,wallSeconds=time.monotonic()-start,peakTreeRssBytes=rss,peakTreePrivateBytes=private if paging else None,peakConvention='20ms sampled lower bound',initialCommitHeadroomBytes=head,capBytes=cap,reserveBytes=reserve,threads=env.get('RAYON_NUM_THREADS','default'),cache=cache_mode,cachePath=str(cache_dir))
(out/'supervision.json').write_text(json.dumps(e,indent=2));print(json.dumps(dict(folder=str(out),**e)))
raise SystemExit(0 if status in ('pass','diagnostic-complete') and p.returncode==0 else 1)
