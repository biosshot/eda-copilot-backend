"""Explicit single geometry-only replay; never overwrite saved run evidence.

The original run used subprocess timeouts and did not measure peak memory.
This reproducible launcher additionally monitors a512MiB/60s outer bound.
It must not be used to repeat the already saved green replay without reason.
"""
from pathlib import Path
import hashlib,json,subprocess,time
import psutil
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent
OUT=ROOT/'debugging/p7-design-checkpoint/copper-geometry'
assert not (OUT/'stages.jsonl').exists(),'Saved replay exists; do not overwrite or repeat it.'
OUT.mkdir(parents=True,exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
source=(FIX/'probe.rs').read_text();sources={}
for tag,path in {'MODEL':'native/analyze/src/model.rs','AREAS':'native/analyze/src/geometry/areas.rs','PRIMITIVES':'native/analyze/src/geometry/primitives.rs'}.items():
    p=ROOT/path;source=source.replace('__'+tag+'__',p.as_posix());sources[path]=sha(p)
(OUT/'probe.rs').write_text(source)
deps=ROOT/'native/analyze/target/debug/deps'
cmd=['rustc','--edition=2021','-O','-C','codegen-units=1','-L','dependency='+str(deps)]
for lib in ['geo','serde_json']:
    paths=list(deps.glob('lib'+lib+'-*.rlib'));assert len(paths)==1
    cmd+=['--extern',lib+'='+str(paths[0])]
cmd += [str(OUT/'probe.rs'),'-o',str(OUT/'probe.exe')]
def bounded(command,stdout,stderr):
    start=time.perf_counter();peak=0
    with stdout.open('wb') as out,stderr.open('wb') as err:
        p=subprocess.Popen(command,stdout=out,stderr=err);m=psutil.Process(p.pid)
        while p.poll() is None:
            try:
                peak=max(peak,sum(q.memory_info().peak_wset for q in [m,*m.children(recursive=True)]))
            except psutil.Error:pass
            if peak>512*1024**2 or time.perf_counter()-start>60:
                for q in m.children(recursive=True):q.kill()
                p.kill();p.wait();raise RuntimeError('Explicit512MiB/60s refusal; no retry')
            time.sleep(.01)
    assert p.returncode==0,stderr.read_text()
    return {'command':command,'seconds':time.perf_counter()-start,'peakBytes':peak,'exitCode':p.returncode}
build=bounded(cmd,OUT/'build-stdout.log',OUT/'build-stderr.log')
run=bounded([str(OUT/'probe.exe'),str(FIX/'regressions.json')],OUT/'stages.jsonl',OUT/'run-stderr.log')
(OUT/'replay-launcher.json').write_text(json.dumps({'build':build,'run':run,'sources':sources,'kernelEvaluations':0},indent=2))
