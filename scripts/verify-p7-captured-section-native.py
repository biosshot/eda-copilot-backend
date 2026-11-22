"""Bounded native continuation of the saved actual captured section request."""
from pathlib import Path
import json,subprocess,time,os,psutil,hashlib,sys
root=Path.cwd();source=root/'debugging/p7-channel-plan/captured-section-field-reuse-1793342259915/input.json';data=json.loads(source.read_text());request=dict(line=dict(kind='polygon-section',geometry=data['geometry'],lengthMm=1),frequencies=[1e9],budget=dict(maxSeconds=300,maxMemoryMiB=1024));folder=root/'debugging/p7-channel-plan'/('captured-section-graded-'+str(time.time_ns()));folder.mkdir();packet=json.dumps(request).encode();(folder/'input.json').write_bytes(packet);exe=root/'native/analyze/target/release/examples/cross-section.exe';env=os.environ.copy();env['P7_SECTION_FULL_REPORT']='1';start=time.perf_counter();peak=0;refusal=None
with (folder/'report.json').open('wb') as out,(folder/'stderr.txt').open('wb') as err:
 p=subprocess.Popen([str(exe)],stdin=subprocess.PIPE,stdout=out,stderr=err,env=env);p.stdin.write(packet);p.stdin.close();process=psutil.Process(p.pid)
 while p.poll() is None:
  try:peak=max(peak,process.memory_info().rss)
  except psutil.NoSuchProcess:pass
  if time.perf_counter()-start>300 or peak>2*1024**3:refusal='wall' if time.perf_counter()-start>300 else 'memory';p.kill();break
  time.sleep(.02)
 code=p.wait()
r=json.loads((folder/'report.json').read_text()) if code==0 else {};evidence=dict(wallSeconds=time.perf_counter()-start,peakRssBytes=peak,exitCode=code,refusal=refusal,status=r.get('status'),diagnostics=r.get('diagnostics'),convergence=r.get('convergence'),binarySha256=hashlib.sha256(exe.read_bytes()).hexdigest(),inputSha256=hashlib.sha256(packet).hexdigest(),scope='Actual captured local section only; no full realRFchannel or5percent certificate');(folder/'evidence.json').write_text(json.dumps(evidence,indent=2));print(json.dumps(dict(directory=str(folder),**evidence)));sys.exit(0 if code==0 and r.get('status')=='complete' and not refusal else 1)
