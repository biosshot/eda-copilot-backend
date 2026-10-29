"""Small developer DC limit check; existing full conductive factor, no AC training."""
from pathlib import Path
import hashlib,json,subprocess,time,sys,psutil
root=Path(__file__).resolve().parents[4]
base=root/'debugging/p6-p7-integration'/sys.argv[1]
exe=root/'native/analyze/target/release/examples/board_dc_limit.exe'
out=base/'model-reuse';out.mkdir(exist_ok=True)
for name in ['STM32G0_CAN_RS485','TPS63060DSCR']:
 result=json.loads((base/name/'cold/result.json').read_text())
 cold=next(p['data'] for r in result['reports'] for p in r['plots'] if p['title'].startswith('Cold stage'))
 cache=base/'cache'/(cold['cache']['key']+'.json')
 board=root/'tests/pcb-analyze'/name/(name+'.json')
 paths=[exe,cache,board];hashes={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in paths}
 started=time.perf_counter();peak=0;refusal=None
 with (out/(name+'-dc.json')).open('wb') as stdout,(out/(name+'-dc.stderr.txt')).open('wb') as stderr:
  p=subprocess.Popen([str(exe),str(board),str(cache)],stdout=stdout,stderr=stderr);process=psutil.Process(p.pid)
  while p.poll() is None:
   try:peak=max(peak,sum(c.memory_info().rss for c in [process,*process.children(recursive=True)]))
   except psutil.NoSuchProcess:pass
   if time.perf_counter()-started>30 or peak>512*1024**2:
    refusal='wall' if time.perf_counter()-started>30 else 'memory';p.kill();break
   time.sleep(.02)
  code=p.wait()
 record=dict(exitCode=code,refusal=refusal,wallSeconds=time.perf_counter()-started,peakRssBytes=peak,hashes=hashes,inputsChanged=any(hashlib.sha256(path.read_bytes()).hexdigest()!=hashes[str(path.relative_to(root))] for path in paths))
 (out/(name+'-dc-run.json')).write_text(json.dumps(record,indent=2));print(name,json.dumps(record))
