"""Reuse existing supervisor memory policy for the small coupled AC runner."""
import importlib.util,pathlib,psutil,subprocess,time,os,json
root=pathlib.Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('p7_supervisor',root/'scripts/verify-p7-automatic-supervised.py')
s=importlib.util.module_from_spec(spec);spec.loader.exec_module(s)
out=pathlib.Path(os.environ.get('P7_SUPERVISOR_OUTPUT',str(root/'debugging/p7-closure'/str(time.time_ns()))));out.mkdir(parents=True,exist_ok=True)
cap=1536*1024**2;reserve=1024**3;head=s.windows_commit_headroom()
reason=s.memory_refusal(True,psutil.virtual_memory().available,head,0,cap,reserve,True)
if reason:
 (out/'supervision.json').write_text(json.dumps(dict(status='resource-failure',reason=reason)));raise SystemExit(1)
env={**os.environ,'P7_OUTPUT_DIR':str(out),'PCB_ANALYSIS_CACHE_DIR':str(out/'cache')}
start=time.monotonic();rss=private=0
with (out/'stdout.txt').open('w') as log:
 runner='scripts/verify-p7-closure-stripline.ts' if os.environ.get('P7_CLOSURE_CONTROL')=='stripline' else 'scripts/verify-p7-closure-baseline.ts'
 command=json.loads(os.environ['P7_SUPERVISOR_TEST_COMMAND']) if 'P7_SUPERVISOR_TEST_COMMAND' in os.environ else ['node','--import','tsx',runner]
 p=subprocess.Popen(command,cwd=root,env=env,stdout=log,stderr=subprocess.STDOUT,creationflags=getattr(subprocess,'CREATE_NO_WINDOW',0)); proc=psutil.Process(p.pid)
 while p.poll() is None:
  children=proc.children(recursive=True)
  r,_,v=s.tree_memory(proc,children,True);rss=max(rss,r);private=max(private,v)
  reason=s.memory_refusal(True,psutil.virtual_memory().available,s.windows_commit_headroom(),v,cap,reserve)
  if time.monotonic()-start>float(os.environ.get('P7_SUPERVISOR_TEST_TIMEOUT','145')):reason='145s supervisor deadline'
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
e=dict(exitCode=p.returncode,status=status,reason=reason,wallSeconds=time.monotonic()-start,peakTreeRssBytes=rss,peakTreePrivateBytes=private,peakConvention='20ms sampled lower bound',initialCommitHeadroomBytes=head,capBytes=cap,reserveBytes=reserve,threads=env.get('RAYON_NUM_THREADS','default'),cache='cold isolated directory')
(out/'supervision.json').write_text(json.dumps(e,indent=2));print(json.dumps(dict(folder=str(out),**e)))
raise SystemExit(0 if status=='pass' and p.returncode==0 else 1)
