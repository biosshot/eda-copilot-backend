from pathlib import Path
import copy,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-final-integration'
if len(sys.argv)>1:OUT=Path(sys.argv[1]).resolve()
source=json.loads((OUT/'automatic.json').read_text());audit=Path(__file__).with_name('audit.py')
results=[]
for case in ['missing-point','false-gauge','changed-source','damaged-current','damaged-basis','false-via-admission']:
 data=copy.deepcopy(source);run=data['runs'][0]
 if case=='missing-point':run['rows'].pop()
 if case=='false-gauge':run['reducedModel']['structuralGaugeModes']=[]
 if case=='changed-source':run['foils'][0]['source_id']='absent-board-source'
 if case=='damaged-current':run['rows'][0]['recoveredState'][4]+=0.1
 if case=='damaged-basis':run['basis'][0][0]+=0.1
 if case=='false-via-admission':data['viaAdmitted']=True
 path=OUT/(case+'.json');path.write_text(json.dumps(data))
 result=subprocess.run([sys.executable,str(audit),str(path),str(OUT)],cwd=ROOT,capture_output=True,text=True)
 assert result.returncode!=0,case
 results.append({'case':case,'exitCode':result.returncode,'diagnostic':result.stderr[-1200:]})
(OUT/'negative-audits.json').write_text(json.dumps(results,indent=2));print(f'{len(results)} corruptions refused')
