"""One predeclared offline reference refinement; existing native matrices reused."""
from pathlib import Path
import importlib.util,json,hashlib,subprocess,time,sys
import numpy as np,psutil
FIX=Path(__file__).resolve().parent;ROOT=FIX.parents[3];OUT=ROOT/'debugging/p7-design-checkpoint/three-reference-margin';OUT.mkdir(parents=True,exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
spec=importlib.util.spec_from_file_location('original_reference',FIX/'generate-reference.py');generator=importlib.util.module_from_spec(spec);spec.loader.exec_module(generator)
old=json.loads((FIX/'references/manifest.json').read_text());levels={r['n']:r for r in old['rows'] if r['case']=='three'}
n=48;panels=18*n*n;ratio=(n/32)**2
predicted_bytes=levels[32]['peakBytes']+(levels[32]['peakBytes']-levels[16]['peakBytes'])*(ratio-1)/(1-1/4)
predicted_seconds=levels[32]['seconds']*ratio**2
binary=ROOT/'debugging/p7-design-checkpoint/fastcap-cli/fastcap-reference.exe';assert sha(binary)==levels[32]['binarySha256']
inp=OUT/'input.qui';text=generator.panels(n,'three');inp.write_text(text,encoding='ascii')
preflight={'refinement':n,'panels':panels,'sourceGeometry':'unchanged three1mm-square35um foils at z0/1/2mm, vacuum; same original panels() generator','inputSha256':sha(inp),'binarySha256':sha(binary),'generatorSha256':sha(FIX/'generate-reference.py'),'priorLevels':{str(k):{'peakBytes':v['peakBytes'],'seconds':v['seconds'],'panels':18*k*k} for k,v in levels.items()},'predictedBytesAffinePanels':predicted_bytes,'memorySafetyFactor':1.25,'admittedPredictedBytes':predicted_bytes*1.25,'predictedSecondsQuadraticPanels':predicted_seconds,'timeSafetyFactor':1.5,'admittedPredictedSeconds':predicted_seconds*1.5,'caps':{'bytes':1024*1024**2,'seconds':60},'reason':'n48 stays inside measured-panel memory/time envelopes; n64 affine memory estimate alone exceeds1GiB. One higher reference only; no native rerun.','screen':'native full quadratic error+2*last full reference change<=5%, strictly decreasing observed changes; not a certified continuum bound.'}
assert preflight['admittedPredictedBytes']<1024*1024**2 and preflight['admittedPredictedSeconds']<60
selection=FIX/'references/three-48-preflight.json'
if selection.exists():assert json.loads(selection.read_text())==preflight
else:selection.write_text(json.dumps(preflight,indent=2))
if '--run' not in sys.argv:print(json.dumps(preflight));sys.exit(0)
assert not (OUT/'run.json').exists(),'one reference attempt already recorded; inspect evidence, do not rerun'
start=time.perf_counter();peak=0;refusal=None
cmd=[str(binary),'-t1e-8','-o6','input.qui']
with (OUT/'stdout.log').open('wb') as stdout,(OUT/'stderr.log').open('wb') as stderr:
 p=subprocess.Popen(cmd,cwd=OUT,stdout=stdout,stderr=stderr);monitor=psutil.Process(p.pid)
 while p.poll() is None:
  try:peak=max(peak,sum(q.memory_info().peak_wset for q in [monitor]+monitor.children(recursive=True)))
  except psutil.Error:pass
  if peak>1024*1024**2 or time.perf_counter()-start>60:
   refusal='1024MiB' if peak>1024*1024**2 else '60s'
   for q in monitor.children(recursive=True):q.kill()
   p.kill();p.wait();break
  time.sleep(.01)
row={'refinement':n,'panels':panels,'seconds':time.perf_counter()-start,'peakBytes':peak,'exitCode':p.returncode,'externalRefusal':refusal,'inputSha256':sha(inp),'binarySha256':sha(binary),'preflightSha256':sha(selection),'command':cmd,'primarySolverSource':old['primarySolverSource'],'driverSha256':sha(Path(__file__))}
for name in ['stdout.log','stderr.log']:row[name+'Sha256']=sha(OUT/name)
if p.returncode==0 and refusal is None:
 row['rawCapacitanceF']=json.loads((OUT/'capacitance-raw-f.json').read_text());destination=FIX/'references/three-48';destination.mkdir(exist_ok=True)
 for name in ['stdout.log','stderr.log','capacitance-raw-f.json']:(destination/name).write_bytes((OUT/name).read_bytes())
 row['capacitance-raw-f.jsonSha256']=sha(OUT/'capacitance-raw-f.json')
(OUT/'run.json').write_text(json.dumps(row,indent=2));(FIX/'references/three-48-run.json').write_text(json.dumps(row,indent=2));print(json.dumps(row))
sys.exit(0 if p.returncode==0 and refusal is None else 2)
