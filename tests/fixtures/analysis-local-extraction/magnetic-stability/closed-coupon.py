"""Prepare/build/run one complete closed-current magnetic coupon, bounded."""
import os
os.environ['OPENBLAS_NUM_THREADS']='1';os.environ['OMP_NUM_THREADS']='1'
from pathlib import Path
import sys,json,hashlib,time,subprocess,psutil
import numpy as np
ROOT=Path(__file__).resolve().parents[4];FIX=Path(__file__).resolve().parent;OUT=ROOT/'debugging/p7-design-checkpoint/magnetic-stability/closed';OUT.mkdir(exist_ok=True);mode=sys.argv[1];sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
if mode=='prepare':
 points=np.array([[x,y] for y in [0.,.0005,.001] for x in [0.,.0005,.001,.0015]]);triangles=[]
 for row in range(2):
  for col in range(3):
   a=row*4+col;triangles.extend([[a,a+1,a+5],[a,a+5,a+4]])
 cells=[];edges={};h=35e-6
 for index,ids in enumerate(triangles):
  t=points[ids];a,b,c=t;twice=float(np.cross(b-a,c-a));grads=np.array([[b[1]-c[1],c[0]-b[0]],[c[1]-a[1],a[0]-c[0]],[a[1]-b[1],b[0]-a[0]]])/twice;densities=[]
  for interior in [5,6]:
   g=grads[ids.index(interior)] if interior in ids else np.zeros(2);densities.append([float(g[1]/h),float(-g[0]/h)])
  cells.append({'sourceId':'closed-triangle-'+str(index),'triangleM':t.tolist(),'zM':[0.,h],'densityPerAmpM2':densities,'nodeIds':ids})
  for k in range(3):
   ia,ib=ids[k],ids[(k+1)%3];edge=points[ib]-points[ia];outward=np.array([edge[1],-edge[0]]);flux=np.array(densities)@outward*h;edges.setdefault(tuple(sorted([ia,ib])),[]).append(flux)
 flux_residual=max(float(max(abs(sum(flux)))) for flux in edges.values());assert flux_residual<1e-12
 data={'cells':cells,'pointsM':points.tolist(),'interiorStreamfunctionNodes':[5,6],'normalFluxJumpOrExteriorFluxMaxA':flux_residual,'geometry':'1.5x1mm finite35um foil;12exact prisms, two continuousP1 streamfunction hats. J=(dpsi/dy,-dpsi/dx)/h; all exterior normal current zero, internal normal flux continuous.','gates':{'relativeFullEnergyError':.05,'referenceWorkingUncertainty':.01,'reciprocity':1e-9,'negativeEigenvalue':0.,'bytes':512*1024**2,'seconds':60},'preflight':{'prisms':12,'physicalDimension':2,'nativeOrderedPairsPerRule':144,'nativeOrders':[8,12],'referenceUniquePairs':78,'referenceSobolPowers':[12,14,16],'referenceScrambleSeeds':[20261024,20261025,20261026,20261027],'referenceTotal4DSamples':78*4*(2**12+2**14+2**16),'plannedLargestSampleArrayBytes':2**16*4*8,'plannedArrayEnvelopeBytes':64*1024**2,'pythonRuntimeAllowanceBytes':192*1024**2,'plannedSimultaneousBytes':256*1024**2,'nativeVolumeSamplesPerRefinedRule':12*12*12**3,'referenceTimeEstimateSeconds':26,'reason':'Earlier independently timed3case Sobol+boxes11.49s for16.5M+boxes;26.8M positive4D samples estimated26s; hard60s/512MiB retained. No ROI run.'}}
 (FIX/'closed-coupon.json').write_text(json.dumps(data,indent=2,sort_keys=True));print(json.dumps(data['preflight']));sys.exit(0)
input_path=FIX/'closed-coupon.json';data=json.loads(input_path.read_text())
if mode=='control':
 assert not (OUT/'control-run.json').exists(),'one explicit control only'
 if not (OUT/'control-preflight.json').exists():(OUT/'control-preflight.json').write_bytes((FIX/'closed-control-preflight.json').read_bytes())
 selection=json.loads((OUT/'control-preflight.json').read_text());assert selection['selectedOrder']==24
 source=(OUT/'probe.rs').read_text();assert 'for order in [8,12]' in source;source=source.replace('for order in [8,12]','for order in [24]');path=OUT/'probe-control.rs';path.write_text(source);binary=OUT/'probe-control.exe';build=json.loads((OUT/'build.json').read_text())
 for name,digest in build['sources'].items():assert sha(ROOT/name)==digest
 subprocess.run(['rustc',*build['flags'],str(path),'-o',str(binary)],check=True,timeout=60);start=time.perf_counter();peak=0;refusal=None
 with (OUT/'control-stdout.json').open('wb') as stdout,(OUT/'control-stderr.log').open('wb') as stderr:
  p=subprocess.Popen([str(binary),str(input_path)],stdout=stdout,stderr=stderr);monitor=psutil.Process(p.pid)
  while p.poll() is None:
   try:peak=max(peak,monitor.memory_info().peak_wset)
   except psutil.Error:pass
   if peak>512*1024**2 or time.perf_counter()-start>60:refusal='512MiB' if peak>512*1024**2 else '60s';p.kill();p.wait();break
   time.sleep(.01)
 row={'seconds':time.perf_counter()-start,'peakBytes':peak,'exitCode':p.returncode,'externalRefusal':refusal,'binarySha256':sha(binary),'sourceSha256':sha(path),'inputSha256':sha(input_path),'preflightSha256':sha(OUT/'control-preflight.json'),'caps':selection['caps']};(OUT/'control-run.json').write_text(json.dumps(row,indent=2));print(json.dumps(row));sys.exit(0 if p.returncode==0 else 2)
if mode=='build':
 source=(FIX/'closed-probe.rs').read_text();sources={}
 for name in ['prism','polyhedron']:
  p=ROOT/f'native/analyze/src/extraction/local/{name}.rs';sources[str(p.relative_to(ROOT))]=sha(p);source=source.replace('__'+name.upper()+'__',str(p).replace('\\','/'))
 path=OUT/'probe.rs';path.write_text(source);deps=ROOT/'native/analyze/target/debug/deps';lib=list(deps.glob('libserde_json-*.rlib'));assert len(lib)==1;binary=OUT/'probe.exe';start=time.perf_counter();flags=['--edition=2021','-O','-C','codegen-units=1','-L','dependency='+str(deps),'--extern','serde_json='+str(lib[0])]
 subprocess.run(['rustc',*flags,str(path),'-o',str(binary)],check=True,timeout=60);meta={'sources':sources,'binarySha256':sha(binary),'sourceSha256':sha(path),'serdeJsonRlibSha256':sha(lib[0]),'flags':flags,'compileSeconds':time.perf_counter()-start};(OUT/'build.json').write_text(json.dumps(meta,indent=2));print(json.dumps(meta));sys.exit(0)
if mode in ['native','reference']:
 destination=OUT/(mode+'-run');destination.mkdir(exist_ok=True);assert not (destination/'run.json').exists(),'no automatic repeat'
 cmd=[str(OUT/'probe.exe'),str(input_path)] if mode=='native' else [sys.executable,str(Path(__file__)),'compute-reference'];start=time.perf_counter();peak=0;refusal=None
 with (destination/'stdout.json').open('wb') as stdout,(destination/'stderr.log').open('wb') as stderr:
  p=subprocess.Popen(cmd,stdout=stdout,stderr=stderr);monitor=psutil.Process(p.pid)
  while p.poll() is None:
   try:peak=max(peak,monitor.memory_info().peak_wset)
   except psutil.Error:pass
   if peak>512*1024**2 or time.perf_counter()-start>60:refusal='512MiB' if peak>512*1024**2 else '60s';p.kill();p.wait();break
   time.sleep(.01)
 row={'command':cmd,'seconds':time.perf_counter()-start,'peakBytes':peak,'exitCode':p.returncode,'externalRefusal':refusal,'caps':data['gates'],'inputSha256':sha(input_path),'driverSha256':sha(Path(__file__))};(destination/'run.json').write_text(json.dumps(row,indent=2));print(json.dumps(row));sys.exit(0 if p.returncode==0 else 2)
assert mode=='compute-reference';assert not (OUT/'reference.json').exists();from scipy.stats import qmc
started=time.perf_counter();cells=data['cells'];origin=np.array(cells[0]['triangleM'][0]);triangles=[np.array(c['triangleM'])-origin for c in cells];areas=[abs(np.cross(t[1]-t[0],t[2]-t[0]))/2 for t in triangles];h=cells[0]['zM'][1]-cells[0]['zM'][0];density=np.array([c['densityPerAmpM2'] for c in cells]);levels=[]
for power in data['preflight']['referenceSobolPowers']:
 rows=[]
 for seed in data['preflight']['referenceScrambleSeeds']:
  u=qmc.Sobol(4,scramble=True,seed=seed).random_base2(power);s=np.sqrt(u[:,0,None]);t=np.sqrt(u[:,2,None]);pa=[p[0]+s*(p[1]-p[0])+s*u[:,1,None]*(p[2]-p[1]) for p in triangles];pb=[p[0]+t*(p[1]-p[0])+t*u[:,3,None]*(p[2]-p[1]) for p in triangles];kernel=np.zeros((12,12));l=np.zeros((2,2))
  for i in range(12):
   for j in range(i+1):
    assert time.perf_counter()-started<58,'reference internal58s cap';rho=np.linalg.norm(pa[i]-pb[j],axis=1);assert min(rho)>0;k=h/rho;z=2*h*(np.arcsinh(k)-k/(np.hypot(1,k)+1));value=float(areas[i]*areas[j]*np.mean(z));kernel[i,j]=kernel[j,i]=value
  for i in range(12):
   for j in range(12):l+=1e-7*kernel[i,j]*(density[i]@density[j].T)
  rows.append({'seed':seed,'rawKernelM5':kernel.tolist(),'rawLH':l.tolist()})
 levels.append({'sobolPower':power,'scrambles':rows,'meanLH':np.mean([r['rawLH'] for r in rows],axis=0).tolist(),'meanKernelM5':np.mean([r['rawKernelM5'] for r in rows],axis=0).tolist()})
result={'independentMethod':'Positive4D exactdouble-z integration, all78 scalar pairs; full finite solenoidal current bilinear form. No production imports.','levels':levels,'seconds':time.perf_counter()-started,'inputSha256':sha(input_path),'driverSha256':sha(Path(__file__))};(OUT/'reference.json').write_text(json.dumps(result,indent=2));(FIX/'closed-reference.json').write_text(json.dumps(result,indent=2));print(json.dumps({'seconds':result['seconds']}))
