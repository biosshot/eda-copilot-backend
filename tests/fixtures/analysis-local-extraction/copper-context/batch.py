"""One bounded batch per G1/G2, no automatic repeats/reference matrix solve."""
from pathlib import Path
import json,subprocess,time,sys,os
import psutil
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent;OUT=ROOT/'debugging/p7-design-checkpoint/copper-context'
g=sys.argv[1];assert g in ['G1','G2'];assert not (OUT/(g+'-batch.json')).exists()
if sys.argv[2:] == ['child']:
    with (OUT/(g+'-coupon.json')).open('wb') as f:
        subprocess.run([str(OUT/'kernel-probe.exe'),'coupon',g,str(FIX.parent/'magnetic-near-far/coupon.json')],stdout=f,check=True)
    if g=='G1':
        with (OUT/'postfix-self.json').open('wb') as f:
            subprocess.run([str(OUT/'kernel-probe.exe'),'self',g,str(OUT/'postfix-witnesses.json')],stdout=f,check=True)
        subprocess.run([sys.executable,str(FIX/'reference-near.py')],check=True)
    sys.exit()
start=time.perf_counter();peak=0;refusal=None
with (OUT/(g+'-batch-stdout.log')).open('wb') as out,(OUT/(g+'-batch-stderr.log')).open('wb') as err:
    p=subprocess.Popen([sys.executable,str(Path(__file__).resolve()),g,'child'],stdout=out,stderr=err);m=psutil.Process(p.pid)
    while p.poll() is None:
        try:peak=max(peak,sum(q.memory_info().peak_wset for q in [m,*m.children(recursive=True)]))
        except psutil.Error:pass
        if time.perf_counter()-start>120 or peak>512*1024**2:
            refusal='120s' if time.perf_counter()-start>120 else '512MiB'
            for q in m.children(recursive=True):q.kill()
            p.kill();p.wait();break
        time.sleep(.01)
(OUT/(g+'-batch.json')).write_text(json.dumps({'configuration':g,'wallSeconds':time.perf_counter()-start,'peakSubtreeBytes':peak,'capBytes':512*1024**2,'capSeconds':120,'exitCode':p.returncode,'refusal':refusal},indent=2))
print((OUT/(g+'-batch.json')).read_text());sys.exit(p.returncode)
