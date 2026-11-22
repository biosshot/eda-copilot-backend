"""Offline primary FastCap reference for finite shifted/three foils, no kernel."""
from pathlib import Path
import argparse,hashlib,json,subprocess,time
import numpy as np
import psutil
ROOT=Path(__file__).resolve().parent

def panels(n,case):
 lines=['0 Finite35um foils, vacuum; primary FastCap offline reference SI metres']
 positions=[(0.,0.),(.001,.00025)] if case=='shifted' else [(0.,0.),(.001,0.),(.002,0.)]
 for index,(z,shift) in enumerate(positions):
  vertices=np.array([[x+shift,y,h] for h in [z,z+.000035] for x,y in [(-.0005,-.0005),(.0005,-.0005),(.0005,.0005),(-.0005,.0005)]])
  for face in [[0,3,2,1],[4,5,6,7],[0,1,5,4],[1,2,6,5],[2,3,7,6],[3,0,4,7]]:
   a,b,c,d=vertices[face]
   for i in range(n):
    for j in range(n):
     corners=[a+(b-a)*u+(d-a)*v for u,v in [(i/n,j/n),((i+1)/n,j/n),((i+1)/n,(j+1)/n),(i/n,(j+1)/n)]]
     lines.append('Q foil'+str(index)+' '+' '.join(format(t,'.17g') for p in corners for t in p))
 return '\n'.join(lines)+'\n'

if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('binary',type=Path);parser.add_argument('output',type=Path);args=parser.parse_args();build=json.loads((ROOT.parent/'fastcap/manifest.json').read_text())['build'];assert hashlib.sha256(args.binary.read_bytes()).hexdigest()==build['binarySha256'];args.output.mkdir(parents=True,exist_ok=True);rows=[]
 for case in ['shifted','three']:
  for n in [16,32]:
   folder=args.output/f'{case}-{n}';folder.mkdir(exist_ok=True);inp=folder/'input.qui';inp.write_text(panels(n,case),encoding='ascii')
   start=time.perf_counter();peak=0
   with (folder/'stdout.log').open('wb') as out,(folder/'stderr.log').open('wb') as err:
    p=subprocess.Popen([str(args.binary.resolve()),'-t1e-8','-o6','input.qui'],cwd=folder,stdout=out,stderr=err);monitor=psutil.Process(p.pid)
    while p.poll() is None:
     try:peak=max(peak,monitor.memory_info().peak_wset)
     except psutil.Error:pass
     if peak>512*1024**2 or time.perf_counter()-start>30:p.kill();p.wait();raise RuntimeError('offline reference512MiB/30s cap')
     time.sleep(.01)
   assert p.returncode==0
   row={'case':case,'n':n,'rawCapacitanceF':json.loads((folder/'capacitance-raw-f.json').read_text()),'inputSha256':hashlib.sha256(inp.read_bytes()).hexdigest(),'binarySha256':hashlib.sha256(args.binary.read_bytes()).hexdigest(),'seconds':time.perf_counter()-start,'peakBytes':peak,'command':['fastcap-reference.exe','-t1e-8','-o6','input.qui']}
   for name in ['capacitance-raw-f.json','stdout.log','stderr.log']:
    row[name+'Sha256']=hashlib.sha256((folder/name).read_bytes()).hexdigest()
   destination=ROOT/'references'/f'{case}-{n}';destination.mkdir(parents=True,exist_ok=True)
   for name in ['capacitance-raw-f.json','stdout.log','stderr.log']:(destination/name).write_bytes((folder/name).read_bytes())
   rows.append(row);print(json.dumps(row),flush=True)
 (ROOT/'references/manifest.json').write_text(json.dumps({'primarySolverSource':{k:build[k] for k in ['source','sourceCommit','binarySha256']},'rows':rows,'geometryGeneratorSha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'referenceUncertaintyGate':'n16->32 full quadratic change<=1%; raw asymmetry retained; primary vacuum solver only, not used for dielectric.'},indent=2))
