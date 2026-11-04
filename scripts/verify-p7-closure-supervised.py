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
args=parser.parse_args()
if not 256<=args.memory_mib<=8192:parser.error('--memory-mib must be in 256..8192')
timeout=float(os.environ.get('P7_SUPERVISOR_TEST_TIMEOUT','145'))
if not math.isfinite(timeout) or timeout<=0:parser.error('supervisor timeout must be finite and positive')
cap=args.memory_mib*1024**2;reserve=1024**3;head=s.windows_commit_headroom()
reason=s.memory_refusal(True,psutil.virtual_memory().available,head,0,cap,reserve,True)
if reason:
 (out/'supervision.json').write_text(json.dumps(dict(status='resource-failure',reason=reason,cache=cache_mode,cachePath=str(cache_dir))));raise SystemExit(1)
env={**os.environ,'P7_OUTPUT_DIR':str(out),'PCB_ANALYSIS_CACHE_DIR':str(cache_dir)}
start=time.monotonic();rss=private=0
with (out/'stdout.txt').open('w') as log:
 runner='scripts/verify-p7-closure-stripline.ts' if os.environ.get('P7_CLOSURE_CONTROL')=='stripline' else 'scripts/verify-p7-closure-baseline.ts'
 command=json.loads(os.environ['P7_SUPERVISOR_TEST_COMMAND']) if 'P7_SUPERVISOR_TEST_COMMAND' in os.environ else ['node','--import','tsx',runner]
 p=subprocess.Popen(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0)); proc=psutil.Process(p.pid)
 while p.poll() is None:
  children=proc.children(recursive=True)
  r,_,v=s.tree_memory(proc,children,True);rss=max(rss,r);private=max(private,v)
  reason=s.memory_refusal(True,psutil.virtual_memory().available,s.windows_commit_headroom(),v,cap,reserve)
  if time.monotonic()-start>timeout:reason=f'{timeout:g}s supervisor deadline'
  if reason:
   for child in reversed(children):
    try:child.kill()
    except psutil.NoSuchProcess:pass
   p.kill();break
  time.sleep(.02)
 p.wait()
resultpath=out/('stripline-comparison.json' if os.environ.get('P7_CLOSURE_CONTROL')=='stripline' else 'comparison.json')
status='missing-evidence'; validation=None
if resultpath.exists():
 try:validation=json.loads(resultpath.read_text());status=validation.get('status') if validation.get('status') in ['pass','unsupported','timeout','resource-failure','missing-evidence'] else 'validation-failure'
 except (ValueError,OSError):status='missing-evidence'
if reason:status='timeout' if 'deadline' in reason else 'resource-failure'
elif p.returncode!=0 and status=='pass':status='child-failure'
elif p.returncode!=0 and status=='missing-evidence':status='child-failure'
e=dict(exitCode=p.returncode,status=status,reason=reason,wallSeconds=time.monotonic()-start,peakTreeRssBytes=rss,peakTreePrivateBytes=private,peakConvention='20ms sampled lower bound',initialCommitHeadroomBytes=head,capBytes=cap,reserveBytes=reserve,threads=env.get('RAYON_NUM_THREADS','default'),cache=cache_mode,cachePath=str(cache_dir))
(out/'supervision.json').write_text(json.dumps(e,indent=2));print(json.dumps(dict(folder=str(out),**e)))
raise SystemExit(0 if status=='pass' and p.returncode==0 else 1)
