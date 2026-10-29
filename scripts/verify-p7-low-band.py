from pathlib import Path
import json,subprocess,time,hashlib,psutil,sys,math
root=Path.cwd();out=root/'debugging/p7-channel-plan/tps-low-band-requested-axis-v2';out.mkdir(parents=True,exist_ok=True)
request=out/'request.json'
if not request.exists():
 for path in (root/'debugging/p6-p7-integration/acceptance-v3/cache').glob('*.json'):
  envelope=json.loads(path.read_text());prepared=json.loads(envelope['payload'])
  r=prepared.get('request')
  if r and abs(r['roiMm'][0][0]+1.97)<1e-9:
   request.write_text(json.dumps(r,indent=2));break
 if not request.exists():raise RuntimeError('Original accepted TPS request was not found; no invented input')
exe=root/'native/analyze/target/release/examples/board_low_band.exe';board=root/'tests/pcb-analyze/TPS63060DSCR/TPS63060DSCR.json';journal=out/'journal'
attempt=out/('attempt-'+str(time.time_ns()));attempt.mkdir();before=len(list(journal.glob('point-*.json'))) if journal.exists() else 0
hashes={str(p.relative_to(root)):hashlib.sha256(p.read_bytes()).hexdigest() for p in [exe,board,request]}
command=[str(exe),str(board),str(request),str(journal)];started=time.perf_counter();peak=0;refusal=None
with (attempt/'stdout.json').open('wb') as so,(attempt/'stderr.txt').open('wb') as se:
 p=subprocess.Popen(command,stdout=so,stderr=se);process=psutil.Process(p.pid)
 while p.poll() is None:
  try:peak=max(peak,sum(c.memory_info().rss for c in [process,*process.children(recursive=True)]))
  except psutil.NoSuchProcess:pass
  if time.perf_counter()-started>=300 or peak>2*1024**3:
   refusal='wall' if time.perf_counter()-started>=300 else 'memory'
   try:children=process.children(recursive=True)
   except psutil.NoSuchProcess:children=[]
   for child in reversed(children):
    try:child.kill()
    except psutil.NoSuchProcess:pass
   try:p.kill()
   except ProcessLookupError:pass
   break
  time.sleep(.02)
 code=p.wait()
evidence=dict(command=command,wallSeconds=time.perf_counter()-started,peakRssBytes=peak,exitCode=code,refusal=refusal,capSeconds=300,capBytes=2*1024**3,hashes=hashes,pointsBefore=before,pointsAfter=len(list(journal.glob('point-*.json'))) if journal.exists() else 0,scope='New low-frequency developer holdouts only; no production band or transient admission')
(attempt/'run.json').write_text(json.dumps(evidence,indent=2));print(json.dumps(dict(directory=str(attempt),**evidence)),flush=True)
if code!=0 or refusal:sys.exit(1)
r=json.loads((attempt/'stdout.json').read_text());checks=r['candidate']['fullChecks'];rows=[]
for c in checks:
 full=c['fullPortVoltages'];real=c['reducedPortReal'];imag=c['reducedPortImaginary'];floor=max(1e-12,1e-3*max(math.hypot(*v) for v in full))
 port=max(math.hypot(v[0]-real[i],v[1]-imag[i])/max(math.hypot(*v),floor) for i,v in enumerate(full));loss=abs(c['fullLossW']-c['reducedLossW'])/max(c['fullLossW'],1e-18)
 rows.append(dict(frequencyHz=c['frequencyHz'],drive=c['drive'],complexPortRelativeError=port,totalLossRelativeError=loss,currentRelativeError=c['maxCurrentCoordinateError'],cellLossRelativeError=c['maxCellLossError']))
passed=len(rows)==15 and all(max(v for k,v in row.items() if k.endswith('Error'))<=.05 for row in rows)
audit=dict(status='pass' if passed else 'failed',rows=rows,physicalTolerance=.05,scope='Existing source FEM versus reduced candidate below1kHz; does not independently certify geometry, common-mode DC or physical transient bandwidth')
(attempt/'audit.json').write_text(json.dumps(audit,indent=2));print(json.dumps(audit));sys.exit(0 if passed else 1)
