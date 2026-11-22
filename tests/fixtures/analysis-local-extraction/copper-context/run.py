"""Bounded, non-overwriting frozen geometry and kernel measurement launcher."""
from pathlib import Path
import hashlib,json,subprocess,time,sys
import psutil
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent
OUT=ROOT/'debugging/p7-design-checkpoint/copper-context'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
read=lambda p:json.loads(p.read_text(encoding='utf-8'))
def bounded(cmd,name):
    assert not (OUT/(name+'-run.json')).exists(),'No repeated/overwritten measurement'
    start=time.perf_counter();peak=0;refusal=None
    with (OUT/(name+'.json')).open('wb') as out,(OUT/(name+'-stderr.log')).open('wb') as err:
        p=subprocess.Popen(list(map(str,cmd)),stdout=out,stderr=err);monitor=psutil.Process(p.pid)
        while p.poll() is None:
            try:peak=max(peak,sum(q.memory_info().peak_wset for q in [monitor,*monitor.children(recursive=True)]))
            except psutil.Error:pass
            if time.perf_counter()-start>120 or peak>512*1024**2:
                refusal='120s' if time.perf_counter()-start>120 else '512MiB'
                for q in monitor.children(recursive=True):q.kill()
                p.kill();p.wait();break
            time.sleep(.01)
    meta={'command':list(map(str,cmd)),'wallSeconds':time.perf_counter()-start,'peakBytes':peak,'refusal':refusal,'exitCode':p.returncode,'preflightSha256':sha(FIX/'preflight.json'),'stdoutSha256':sha(OUT/(name+'.json'))}
    meta['binarySha256']=sha(Path(cmd[0]))
    (OUT/(name+'-run.json')).write_text(json.dumps(meta,indent=2));print(json.dumps({'name':name,**meta}),flush=True)
    return p.returncode==0
if __name__=='__main__':
    if sys.argv[1] in ['geometry','geometry-optimized']:
        optimized=sys.argv[1]=='geometry-optimized'
        names=sys.argv[2:] or sorted(p.parent.name for p in (ROOT/'tests/pcb-analyze').glob('*/fixture.json'))
        for name in names:
            for g in ['G0','G1']:
                cmd=[OUT/'optimized-probe.exe' if optimized else ROOT/'native/analyze/target/debug/examples/copper_context.exe','geometry',g,ROOT/f'tests/pcb-analyze/{name}/{name}.json']
                pre=OUT.parent/f'{name}-whole-barrel-preflight.json'
                if g=='G1' and pre.exists():cmd += [json.dumps(read(pre)['scopeMm'])]
                bounded(cmd,name+'-'+g+('-opt' if optimized else ''))
