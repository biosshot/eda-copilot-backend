"""Isolated actual-source builds and single supervised coupon/plan/reference runs."""
import os
os.environ['OPENBLAS_NUM_THREADS']='1';os.environ['OMP_NUM_THREADS']='1'
from pathlib import Path
import sys,json,hashlib,subprocess,time,psutil
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent
OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-near-far';OUT.mkdir(exist_ok=True)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
mode=sys.argv[1]
if mode=='build':
 source=(FIX/'probe.rs').read_text();sources={}
 for name in ['prism','polyhedron','far','near_far']:
  p=ROOT/f'native/analyze/src/extraction/local/{name}.rs';sources[str(p.relative_to(ROOT))]=sha(p);source=source.replace('__'+name.upper()+'__',p.as_posix())
 path=OUT/'probe.rs';path.write_text(source);deps=ROOT/'native/analyze/target/debug/deps';lib=list(deps.glob('libserde_json-*.rlib'));assert len(lib)==1
 binary=OUT/'probe.exe';flags=['--edition=2021','-O','-C','codegen-units=1','-L','dependency='+str(deps),'--extern','serde_json='+str(lib[0])]
 start=time.perf_counter();subprocess.run(['rustc',*flags,str(path),'-o',str(binary)],check=True,timeout=60)
 row={'sources':sources,'sourceSha256':sha(path),'binarySha256':sha(binary),'rlibSha256':sha(lib[0]),'compiler':subprocess.check_output(['rustc','--version']).decode().strip(),'flags':flags,'seconds':time.perf_counter()-start}
 (OUT/'build.json').write_text(json.dumps(row,indent=2));print(json.dumps(row));sys.exit()
if mode in ['coupon','plan','reference']:
 destination=OUT/(mode+'-run');destination.mkdir(exist_ok=True);assert not (destination/'run.json').exists(),'refuse unchanged completed repeat'
 inputs=[FIX/'coupon.json'] if mode!='plan' else [ROOT/'debugging/p7-design-checkpoint/independent-current'/f'{n}-supports.json' for n in ['BandAmpV5','STM32G0_CAN_RS485','TPS63060DSCR']]
 cmd=[sys.executable,str(Path(__file__)),'compute-reference'] if mode=='reference' else [str(OUT/'probe.exe'),mode,*map(str,inputs)]
 start=time.perf_counter();peak=0;refusal=None
 with (destination/'stdout.json').open('wb') as stdout,(destination/'stderr.log').open('wb') as stderr:
  p=subprocess.Popen(cmd,stdout=stdout,stderr=stderr);monitor=psutil.Process(p.pid)
  while p.poll() is None:
   try:peak=max(peak,monitor.memory_info().peak_wset)
   except psutil.Error:pass
   if peak>512*1024**2 or time.perf_counter()-start>60:refusal='512MiB' if peak>512*1024**2 else '60s';p.kill();p.wait();break
   time.sleep(.01)
 row={'command':cmd,'seconds':time.perf_counter()-start,'peakBytes':peak,'capBytes':512*1024**2,'capSeconds':60,'exitCode':p.returncode,'externalRefusal':refusal,'inputs':{str(i.relative_to(ROOT)):sha(i) for i in inputs},'driverSha256':sha(Path(__file__)),'rawSha256':sha(destination/'stdout.json')}
 (destination/'run.json').write_text(json.dumps(row,indent=2));print(json.dumps(row));sys.exit(0 if p.returncode==0 else 2)
assert mode=='compute-reference'
import numpy as np
from numpy.polynomial.legendre import leggauss
started=time.perf_counter();data=json.loads((FIX/'coupon.json').read_text());cells=data['cells'];density=np.array([c['densities'] for c in cells]);old=json.loads((FIX.parent/'magnetic-stability/closed-reference.json').read_text());base=np.array(old['levels'][-1]['meanKernelM5']);levels=[]
assert not (FIX/'reference.json').exists(),'frozen reference cannot be overwritten'
for order in [4,6,8]:
 x,w=leggauss(order);x=(x+1)/2;w=w/2;u,v=np.meshgrid(x,x,indexing='ij');wu,wv=np.meshgrid(w,w,indexing='ij');u=u.ravel();v=v.ravel();wg=(wu*wv).ravel()
 points=[];weights=[]
 for c in cells:
  t=np.array(c['triangleM']);a,b,d=t;twice=abs(np.linalg.det(np.stack([b-a,d-a])))
  points.append(a+u[:,None]*(b-a)+(1-u[:,None])*v[:,None]*(d-a));weights.append(wg*(1-u)*twice)
 kernel=np.zeros((24,24));kernel[:12,:12]=base;kernel[12:,12:]=base;h=cells[0]['zM'][1]-cells[0]['zM'][0]
 for i in range(12):
  for j in range(12,24):
   assert time.perf_counter()-started<58
   rho=np.linalg.norm(points[i][:,None,:]-points[j][None,:,:],axis=2);assert rho.min()>.001
   k=h/rho;z=2*h*(np.arcsinh(k)-k/(np.hypot(1,k)+1))
   kernel[i,j]=kernel[j,i]=weights[i]@z@weights[j]
 l=np.einsum('ij,iad,jbd->ab',1e-7*kernel,density,density)
 levels.append({'order':order,'kernelM5':kernel.tolist(),'matrixH':l.tolist()})
result={'method':'Separated cross-island positive triangle Gauss4/6/8 with exact double-z; translated self blocks reuse frozen independent4D Sobol reference, never native imports.','levels':levels,'seconds':time.perf_counter()-started,'sourceReferenceSha256':sha(FIX.parent/'magnetic-stability/closed-reference.json'),'inputSha256':sha(FIX/'coupon.json'),'referenceUncertainty':'Empirical inherited self uncertainty plus twice cross last-change; not certified continuum enclosure.'}
(FIX/'reference.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
