"""Refresh hash-bound direct DSL inputs; does not run or accept a solver."""
import json, hashlib, pathlib
root = pathlib.Path(__file__).resolve().parents[1]
out = root / 'tests/fixtures/analysis-p7-closure'
def sha(p): return hashlib.sha256(p.read_bytes()).hexdigest()
cases=[]
supplemental=json.loads((out/'manifest.json').read_text())['supplementalControls']
# Supplemental DSL/reference owned separately; generator must preserve their exact bytes.
for control in supplemental: control['dslSha256']=sha(root/control['dsl'])
for name, layers, ends in [
 ('STM32G0_CAN_RS485',2,[['U5','7','U5','5'],['J5','2','J5','3']]),
 ('NanoScope',4,[['U3','52','U3','57'],['U4','6','U4','4']]),
 ('ThunderF722',6,[['U2','34','U2','31'],['U4','6','U4','4']]),
 ('TPS63060DSCR',2,[['J1','1','J1','2'],['U1','3','U1','4']]),
 ('BandAmpV5',2,[['RF1','2','RF1','1'],['C1','1','U1','2']])]:
 p=root/f'tests/pcb-analyze/{name}/{name}.json'; b=json.loads(p.read_text())
 pads=[]
 for e in ends:
  pair=[]
  for c,n in [e[:2],e[2:]]:
   q=next(p for p in b['pads'] if p.get('component')==c and p['number']==n)
   pair.append({k:q.get(k) for k in ['id','component','number','net','at','layers']})
  pads.append(pair)
 stack=[]
 for i in range(layers):
  stack.append(dict(kind='copper',name='TOP' if i==0 else 'BOTTOM' if i==layers-1 else f'INNER_{i}',thicknessMm=.035,material='cu'))
  if i<layers-1: stack.append(dict(kind='dielectric',name=f'assumed-gap-{i}',thicknessMm=(1.6-layers*.035)/(layers-1),material='d'))
 code="// Benchmark hypotheses: not measured stack, materials or termination. Complete selected copper channel.\nconst a=analysis({board});\n"
 for label,e in zip(['input','output'],ends): code+=f"const {label}=a.port({{positive:pad('{e[0]}','{e[1]}'),negative:pad('{e[2]}','{e[3]}')}});\n"
 code+=f"const layers=stackup({json.dumps(dict(boardThicknessMm=1.6,viaPlatingThicknessUm=25,platingMaterial='cu',diameterConvention='finished_hole',layers=stack))});\n"
 code+="const z=await a.impedance().target(a.channel({from:input,to:output})).method('layered').stackup(layers).materials({cu:{conductivitySM:58e6,state:'Benchmark copper 25 C'},d:{relativePermittivity:4.2,state:'Benchmark lossless dielectric; actual data missing'}}).frequency({startHz:1000,stopHz:10000,points:3,scale:'log'}).budget({maxSeconds:300,maxMemoryMiB:"+str(2048 if layers==2 else 4096)+"}).run();\na.plot({title:'Selected full copper channel',kind:'table',data:z.solution});\n"
 d=out/(name+'.js'); d.write_text(code,encoding='utf8')
 nets={q['net'] for pair in pads for q in pair}
 tracks=[dict(id=t.get('id'),net=t.get('net'),layer=t.get('layer')) for t in b['copper']['fixed']['tracks'] if t.get('net') in nets]
 cases.append(dict(id=name,category='real-copper-channel',board=str(p.relative_to(root)).replace('\\','/'),boardSha256=sha(p),dsl=str(d.relative_to(root)).replace('\\','/'),dslSha256=sha(d),layers=layers,contacts=pads,selectedNetTracks=tracks,branches='All physical branches on selected nets retained; no source clipping',status='blocked',diagnostic='Full-channel EM/reference admission unestablished; geometry planning audit required before heavy run',reference=None,missingFields=['actual dielectric stack','loss tangent','roughness','measured channel response'],loads='50 Ohm power-wave comparison convention; no inferred component loads'))
refs=root/'tests/fixtures/analysis-cross-section/references.json'
reused=[]
for variant,packet in [('straight','1793393382999381200'),('bend','1793388988025203100')]:
 p=root/f'debugging/p7-channel-plan/automatic-supervised-{packet}/whole-reference-comparison.json'
 if p.exists():
  target=out/(variant+'-reused-comparison.json');target.write_bytes(p.read_bytes())
  reused.append(dict(variant=variant,category='reused numerical whole-volume cross-reference; not independent physical truth',path=str(target.relative_to(root)).replace('\\','/'),sha256=sha(target),origin=str(p.relative_to(root)).replace('\\','/'),units='complex dimensionless50 Ohm S; delay seconds; copper loss W under stored drive',ports='Exact stored physical port convention retained inside packet',newRun=False))
(out/'reused-references.json').write_text(json.dumps(reused,indent=2),encoding='utf8')
r=json.loads(refs.read_text())['pair'][0]
line=dict(kind='coupled-microstrip',widthMm=1,copperThicknessMm=.035,dielectricHeightMm=1,gapMm=1,lengthMm=20,material=dict(relativePermittivity=4,lossTangent=0,state='Explicit lossless finite-thickness engineering benchmark against thin reference'))
code="const a=analysis();const z=await a.impedance().target(a.line("+json.dumps(line)+")).method('cross-section').frequency({startHz:1e6,stopHz:1e8,points:3,scale:'log'}).mesh({maxNodes:80000,maxTriangles:160000,domainScale:4,maxEdgeMm:.3,maxRefinements:5}).budget({maxSeconds:120,maxMemoryMiB:1024}).run();a.plot({title:'Coupled FEM RLGC',data:z.rlgc});a.plot({title:'Coupled modes',data:z.characteristicImpedance});"
d=out/'coupled.js';d.write_text(code,encoding='utf8')
cases.insert(0,dict(id='coupled',category='independent-engineering-equations',dsl=str(d.relative_to(root)).replace('\\','/'),dslSha256=sha(d),line=line,reference=dict(path=str(refs.relative_to(root)).replace('\\','/'),sha256=sha(refs),source='https://qucs.github.io/tech/node77.html',data=r,uncertainty='Engineering approximation; no rigorous measured uncertainty; thin-conductor reference versus explicit t/h=.035 candidate; finite-thickness systematic uncertainty unbounded'),ports=['positiveInput','negativeInput','positiveOutput','negativeOutput'],status='runnable'))
previous=json.loads((out/'manifest.json').read_text())
for case in cases:
 old=next((c for c in previous['cases'] if c['id']==case['id']),{})
 for key in ['physicalConnectivity','physicalValidation','comparisonPassed']:
  if key in old: case[key]=old[key]
(out/'manifest.json').write_text(json.dumps(dict(schema=1,geometryAudit=previous.get('geometryAudit'),supplementalControls=supplemental,coupons=json.loads((out/'manifest.json').read_text()).get('coupons',[]),units=dict(capacitance='F/m',inductance='H/m',delay='s/m',impedance='Ohm',response='dimensionless complex S'),frequenciesHz=[1e6,1e7,1e8],metrics=dict(relativeTolerance=.08,capacitanceFloor=1e-15,inductanceFloor=1e-12,complexResponseFloor=1e-4,delayFloor=1e-13,impedanceFloor=.01,floorRationale='Far below expected modal coefficients/delay; S floor -80dB avoids division by symmetry zeros; fixed before execution'),cases=cases),indent=2),encoding='utf8')




