"""Explicit schema1->lossless schema2 developer evidence migration, never production.
Requires a complete independently audited old packet and an exactly equal new
prepared model/geometry. Numerical source changes forbid reuse of solved points.
"""
from pathlib import Path
import subprocess,json,hashlib,struct,os,sys
root=Path(__file__).resolve().parents[4]
old,new=map(Path,sys.argv[1:3]);validate_existing='--validate-existing' in sys.argv[3:]
changed=subprocess.check_output(['git','diff','a9f23dc','--name-only','--','native/analyze/src','native/node-addon/src','native/analyze/Cargo.toml','native/node-addon/Cargo.toml','native/analyze/Cargo.lock','native/node-addon/Cargo.lock'],cwd=root,text=True).splitlines()
assert changed==['native/analyze/src/extraction/local/developer_checkpoint.rs'],'numerical source/dependencies changed; recomputation required'
load=lambda p:json.loads(p.read_text())
digest=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
old_run,new_run=load(old/'run.json'),load(new/'run.json')
old_result,new_result=load(old/'stdout.json'),load(new/'stdout.json')
audit=load(old/'integration-audit.json')
assert audit['status']=='complete' and audit['points']==51 and audit['completedPointsPass']
assert old_run['exitCode']==new_run['exitCode']==0 and not old_run['refusal'] and not new_run['refusal']
for path,h in old_run['hashes'].items():
 if not path.endswith('board_volume.exe'):assert new_run['hashes'][path]==h,'board/request changed'
keys=['request','nodesM','copperCells','cellResistanceOhm','cellDensityPerAmpM2','mapping','potentialCount','currentCount','components','sourceIds','electrodeNodes','fieldNodesM','fieldCopperCells','fieldCells','fieldEpsilonR','externalFaces','copperVolumeM3','fieldVolumeM3']
for key in keys:assert old_result[key]==new_result[key],key+' mismatch'
for key in ['model','currentBasis','voltageBasis']:assert old_result['candidate'][key]==new_result['candidate'][key],key+' mismatch'
def fnv(text):
 h=0xcbf29ce484222325
 for b in text.encode():h=((h^b)*0x100000001b3)&((1<<64)-1)
 return format(h,'016x')
def encode(value):
 if isinstance(value,float):return {'@p7-f64':struct.pack('>d',value).hex()}
 if isinstance(value,list):return [encode(v) for v in value]
 if isinstance(value,dict):return {k:encode(v) for k,v in value.items()}
 return value
def decode(value):
 if isinstance(value,dict):
  if set(value)=={'@p7-f64'}:return struct.unpack('>d',bytes.fromhex(value['@p7-f64']))[0]
  return {k:decode(v) for k,v in value.items()}
 if isinstance(value,list):return [decode(v) for v in value]
 return value
target=Path(new_run['developerCheckpoint']);prepared=load(target/'prepared.json')
assert prepared['schema']==2 and prepared['checksum']==fnv(prepared['payload'])
origin=Path(old_run['developerCheckpoint']);rows=old_result['candidate']['fullChecks'];assert len(rows)==51
state=decode(json.loads(prepared['payload']))
assert state['model']==new_result['candidate']['model'] and state['basis']['current']==new_result['candidate']['currentBasis'] and state['basis']['voltage']==new_result['candidate']['voltageBasis']
for frequency,port,solution in state['training_solutions']:
 drive=[1.,0.] if port==0 else [0.,1.];row=next(r for r in rows if r['frequencyHz']==frequency and r['drive']==drive)
 assert solution['real_currents_a']==row['fullCurrentReal'] and solution['imaginary_currents_a']==row['fullCurrentImaginary'] and solution['audit']==row['audit'],'new full training solution differs from original core'
source_identity=None;files=[]
for index,row in enumerate(rows):
 name=f'point-{index:02}.json';source=origin/name;envelope=load(source)
 assert envelope['schema']==1 and envelope['checksum']==fnv(envelope['payload'])
 assert json.loads(envelope['payload'])==row,'original point differs from completed full packet'
 if source_identity is None:source_identity=envelope['identity']
 assert source_identity==envelope['identity']
 destination=target/name
 if validate_existing:
  existing=load(destination);assert existing['schema']==2 and existing['identity']==prepared['identity'] and existing['checksum']==fnv(existing['payload']) and decode(json.loads(existing['payload']))==row
  files.append(dict(name=name,sha256=digest(destination),originSha256=digest(source)));continue
 assert not destination.exists(),'refuse overwriting a new solved point'
 payload=json.dumps(encode(row),sort_keys=True,separators=(',',':'))
 provenance=dict(originIdentity=source_identity,originPointSha256=digest(source),originResultSha256=digest(old/'stdout.json'),exactPreparedModelAndGeometry=True)
 output=json.dumps(dict(schema=2,identity=prepared['identity'],checksum=fnv(payload),payload=payload,migration=provenance),sort_keys=True,separators=(',',':'))
 assert len(output.encode())<=64*1024**2
 temp=destination.with_suffix('.migration.tmp');temp.write_text(output);os.replace(temp,destination);files.append(dict(name=name,sha256=digest(destination),originSha256=digest(source)))
manifest=dict(status='validated-existing-migration' if validate_existing else 'migrated-without-full-resolve',points=len(files),originRun=old_run,newPreparationRun=new_run,sourceDiff=changed,exactGeometryKeys=keys,exactModelAndBothBases=True,exactFourNewTrainingCurrentsAndAudits=True,originIdentity=source_identity,newIdentity=prepared['identity'],preparedSha256=digest(target/'prepared.json'),files=files)
(target.parent/('migration-reaudit.json' if validate_existing else 'migration.json')).write_text(json.dumps(manifest,indent=2));print(json.dumps({k:v for k,v in manifest.items() if k not in ['originRun','newPreparationRun','files']}))
