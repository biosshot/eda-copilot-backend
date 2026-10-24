"""Freeze finite vectors and input-only error scales; no FFT or convolution."""
import hashlib,json,math,pathlib
p=pathlib.Path(__file__).resolve().parent
fixtures=[]
def add(name,values,real=True,**fields):
    pairs=[[float(v),0.] for v in values] if real else [[float(a),float(b)] for a,b in values]
    fixtures.append({'name':name,'real':real,'samples':pairs,'dtSeconds':1/1024,
        'inputL1':sum(math.hypot(*v) for v in pairs),'inputMaximum':max(math.hypot(*v) for v in pairs),
        'inputEnergy':sum(a*a+b*b for a,b in pairs),**fields})
add('impulse-5',[1,0,0,0,0]);add('constant-8',[2]*8)
add('tone-128',[.25+1.7*math.cos(2*math.pi*17*n/128+.31) for n in range(128)],tone={'bin':17,'amplitude':1.7,'phaseRad':.31,'dc':.25})
add('tone-odd-127',[.8*math.cos(2*math.pi*13*n/127-.43) for n in range(127)],tone={'bin':13,'amplitude':.8,'phaseRad':-.43,'dc':0})
add('odd-last-63',[1.2*math.cos(2*math.pi*31*n/63+.23) for n in range(63)],tone={'bin':31,'amplitude':1.2,'phaseRad':.23,'dc':0},rectangularOnly=True)
add('nyquist-64',[.8*(-1)**n for n in range(64)],nyquistAmplitude=.8)
add('offbin-256',[.9*math.cos(2*math.pi*37.35*n/256-.22) for n in range(256)],offBin=37.35)
add('asymmetric-prime-17',[(.2*n-1,(-1)**n*(.1+n/17)) for n in range(17)],False)
state=0x163b92a7
for length in [1,2,3,7,8,11,16,31,64]:
    values=[]
    for n in range(length):
        state=(1664525*state+1013904223)&0xffffffff;a=state/2**31-1
        state=(1664525*state+1013904223)&0xffffffff;b=state/2**31-1
        values.append((a,b))
    add('deterministic-'+str(length),values,False)
payload={'fixtures':fixtures,'convolution':{'left':[[1,0],[2,0],[3,0],[4,0]],'right':[[1,0],[-.5,0],[1,0]],'dtSeconds':.125}}
(p/'arrays.json').write_text(json.dumps(payload,separators=(',',':'))+'\n')
names=['definition.json','arrays.json'];manifest={name:hashlib.sha256((p/name).read_bytes()).hexdigest() for name in names}
(p/'frozen-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Frozen',len(fixtures),'vectors; no numerical reference transforms executed')
