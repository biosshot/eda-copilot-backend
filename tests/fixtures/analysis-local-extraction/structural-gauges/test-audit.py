from pathlib import Path
import json,copy,subprocess,sys
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p6-p7-structural-gauges';base=json.loads((OUT/'coupon.json').read_text());results=[]
for kind in ['missing-point','changed-physical-C','missing-certificate','false-admission','corrupt-current','corrupt-recovery']:
 v=copy.deepcopy(base);r=v['runs'][0]
 if kind=='missing-point':r['sweep'].pop()
 elif kind=='changed-physical-C':r['model']['capacitanceF'][0][2]*=2
 elif kind=='missing-certificate':r['model']['structuralGaugeModes']=[]
 elif kind=='false-admission':r['sweep'][0]['nativePointAdmitted']=False
 elif kind=='corrupt-current':r['sweep'][0]['currentsRealA'][0]*=2
 elif kind=='corrupt-recovery':r['sweep'][0]['reducedPotentialsRealV'][0]*=2
 path=OUT/(kind+'.json');path.write_text(json.dumps(v))
 p=subprocess.run([sys.executable,str(Path(__file__).with_name('audit.py')),str(path),str(OUT/'negative-audit')],capture_output=True,text=True)
 assert p.returncode!=0,kind
 results.append({'case':kind,'exitCode':p.returncode,'diagnostic':p.stderr[-2000:]})
(OUT/'audit-negative-tests.json').write_text(json.dumps(results,indent=2));print('6 saved-value corruption cases refused')
