from pathlib import Path
import subprocess,time,json,psutil,hashlib,sys,re,os
r=Path(__file__).resolve().parents[4]
packet=sys.argv[1] if len(sys.argv)>1 else 'single-method-300s-reproduction'
mode=sys.argv[2] if len(sys.argv)>2 else 'prepare'
if mode not in ['prepare','verify']:raise ValueError('prepare or developer verify mode required')
cap_seconds=2100 if mode=='verify' else 300
if not re.fullmatch(r'[A-Za-z0-9-]+',packet):raise ValueError('plain packet name required')
out=r/'debugging/p7-board-integration'/packet;out.mkdir(parents=True,exist_ok=False)
exe=r/'native/analyze/target/release/examples/board_volume.exe'; board=r/'tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json'; req=r/'tests/fixtures/analysis-local-extraction/board-integration/single-method-via-request.json'
for label,config in [('direct-fem','entity:6:8:16:3')]:
 d=out/label;d.mkdir(); start=time.perf_counter();peak=0;reason=None
 with (d/'stdout.json').open('wb') as so,(d/'stderr.txt').open('wb') as se:
  cmd=[str(exe),str(board),str(req),config,mode]; p=subprocess.Popen(cmd,stdout=so,stderr=se);proc=psutil.Process(p.pid)
  while p.poll() is None:
   try: peak=max(peak,sum(x.memory_info().rss for x in [proc,*proc.children(recursive=True)]))
   except psutil.NoSuchProcess:pass
   if time.perf_counter()-start>=cap_seconds or peak>2*1024**3:
    reason='wall' if time.perf_counter()-start>=cap_seconds else 'memory'
    try: children=proc.children(recursive=True)
    except psutil.NoSuchProcess: children=[]
    for child in reversed(children):
     try:child.kill()
     except psutil.NoSuchProcess:pass
    try:p.kill()
    except ProcessLookupError:pass
    break
   time.sleep(.02)
  code=p.wait()
 result=dict(methodThreads=int(os.environ.get("P7_FEM_THREADS","1")),command=cmd,wallSeconds=time.perf_counter()-start,peakRssBytes=peak,exitCode=code,refusal=reason,capSeconds=cap_seconds,capBytes=2*1024**3,hashes={str(x.relative_to(r)):hashlib.sha256(x.read_bytes()).hexdigest() for x in [exe,board,req]})
 (d/'run.json').write_text(json.dumps(result,indent=2));print(json.dumps(result),flush=True)
