"""ONE supervised120s/512MiB batch; actual kernel unchanged, telemetry copy explicit."""
from pathlib import Path
import json,hashlib,sys,subprocess,time,psutil
from trace import ROOT,FIX,OUT,OLD,sha,read
mode=sys.argv[1]
if mode=='build':
 source=(FIX/'probe.rs').read_text();sources={}
 paths={'MODEL':ROOT/'native/analyze/src/model.rs','AREAS':ROOT/'native/analyze/src/geometry/areas.rs','PRIMITIVES':ROOT/'native/analyze/src/geometry/primitives.rs','PRISM':ROOT/'native/analyze/src/extraction/local/prism.rs','POLYHEDRON':ROOT/'native/analyze/src/extraction/local/polyhedron.rs','FAR':ROOT/'native/analyze/src/extraction/local/far.rs','NEAR_FAR':ROOT/'native/analyze/src/extraction/local/near_far.rs'}
 for label,p in paths.items():sources[str(p.relative_to(ROOT))]=sha(p);source=source.replace('__'+label+'__',p.as_posix())
 assert sha(paths['POLYHEDRON'])=='1f9498a626e53e91a2d6de2882b799dd30612109fb2ab8996befe3854df81e5e'
 assert sha(paths['PRISM'])=='22857c009475aa9b8884bab0cb8b5dede6d75c84a1a9112bd7ae284b20a8fddc'
 original=paths['POLYHEDRON'].read_text();needle='self.at_diagnostic(point).map(|v| v.value_m2)'
 replacement='match self.at_diagnostic(point) { Ok(v)=>{crate::record_evaluation(v.source_order);Ok(v.value_m2)}, Err(e)=>{crate::record_failure();Err(e)} }'
 assert original.count(needle)==1;instrumented=OUT/'poly-count.rs';instrumented.write_text(original.replace(needle,replacement));source=source.replace('__POLY_COUNT__',instrumented.as_posix())
 probe=OUT/'probe.rs';probe.write_text(source);binary=OUT/'probe.exe';deps=ROOT/'native/analyze/target/debug/deps';flags=['--edition=2021','-O','-C','codegen-units=1','-L','dependency='+str(deps)];libraries={}
 for lib in ['geo','serde_json']:
  paths=list(deps.glob('lib'+lib+'-*.rlib'));assert len(paths)==1;flags+=['--extern',lib+'='+str(paths[0])];libraries[lib]=sha(paths[0])
 start=time.perf_counter();subprocess.run(['rustc',*flags,str(probe),'-o',str(binary)],check=True,timeout=60)
 meta={'sources':sources,'probeSha256':sha(probe),'instrumentedSha256':sha(instrumented),'instrumentationExactReplacement':{'needle':needle,'replacement':replacement},'binarySha256':sha(binary),'libraries':libraries,'flags':flags,'compiler':subprocess.check_output(['rustc','--version']).decode().strip(),'seconds':time.perf_counter()-start,'preflightSha256':sha(FIX/'measurement-preflight.json') if (FIX/'measurement-preflight.json').exists() else None}
 (OUT/'build.json').write_text(json.dumps(meta,indent=2));print(json.dumps(meta));sys.exit()
if mode=='plans':
 assert not (OUT/'plans-run.json').exists(),'one additional native plan telemetry export only'
 inputs=[OLD/'independent-current'/f'{n}-supports.json' for n in ['BandAmpV5','STM32G0_CAN_RS485','TPS63060DSCR']]
 start=time.perf_counter();cmd=[str(OUT/'probe.exe'),'--plans',*map(str,inputs)]
 with (OUT/'plans.jsonl').open('wb') as out:
  subprocess.run(cmd,stdout=out,check=True,timeout=60)
 (OUT/'plans-run.json').write_text(json.dumps({'command':cmd,'seconds':time.perf_counter()-start,'kernelEvaluations':0,'reason':'Saved plans contained counts only; export actual native membership after Python mirror TPS mismatch40pairs, no numeric kernel measurement.','rawSha256':sha(OUT/'plans.jsonl'),'inputHashes':{str(p):sha(p) for p in inputs}},indent=2))
 print(json.dumps({'seconds':time.perf_counter()-start,'kernelEvaluations':0}));sys.exit()
assert mode=='run';assert not (OUT/'run.json').exists(),'ONE batch, no automatic repeats'
preflight=FIX/'measurement-preflight.json';assert read(OUT/'build.json')['preflightSha256'] in [None,sha(preflight)]
binary=OUT/'probe.exe';assert sha(binary)==read(OUT/'build.json')['binarySha256'];assert len(read(preflight)['samples'])<=48
start=time.perf_counter();peak=0;refusal=None;cmd=[str(binary),str(preflight),str(FIX/'stm-trace.json')]
with (OUT/'batch.jsonl').open('wb') as stdout,(OUT/'stderr.log').open('wb') as stderr:
 p=subprocess.Popen(cmd,stdout=stdout,stderr=stderr);monitor=psutil.Process(p.pid)
 while p.poll() is None:
  try:peak=max(peak,monitor.memory_info().peak_wset)
  except psutil.Error:pass
  if time.perf_counter()-start>120 or peak>512*1024**2:refusal='120s' if time.perf_counter()-start>120 else '512MiB';p.kill();p.wait();break
  time.sleep(.01)
row={'command':cmd,'seconds':time.perf_counter()-start,'peakBytes':peak,'capBytes':512*1024**2,'capSeconds':120,'exitCode':p.returncode,'externalRefusal':refusal,'inputSha256':sha(preflight),'traceSha256':sha(FIX/'stm-trace.json'),'binarySha256':sha(binary),'rawSha256':sha(OUT/'batch.jsonl')}
(OUT/'run.json').write_text(json.dumps(row,indent=2));print(json.dumps(row));sys.exit(0 if p.returncode==0 else 2)
