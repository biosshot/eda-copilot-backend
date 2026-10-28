"""Small independent frozen elementary-function controls, generated once."""
import json,struct,mpmath as mp
mp.mp.dps=90
def bits(x):return struct.unpack('<Q',struct.pack('<d',x))[0]
def parts(x):
 hi=float(x);lo=float(x-mp.mpf(hi));return[bits(hi),bits(lo)]
def value(p):return mp.mpf(p[0])+mp.mpf(p[1])
inputs=[(1.,2.**-70),(.5,2.**-90),(1e-100,0.),(1e100,0.),(2.,0.),(.99999999999999,0.),(1.01,0.),(.125,0.),(3.141592653589793,0.)]
pairs=[((y,0.),(x,0.))for y,x in [(1,1),(1,-1),(-1,-1),(-1,1),(1e-100,1),(1,1e-100),(1e100,-1e100),(.4142135623730951,1),(2.414213562373095,1)]]+[((1.,2.**-70),(.5,-2.**-90))]
print(json.dumps(dict(method='independent mpmath elementary functions on exact binary64 sums',decimalPrecision=90,ln=[dict(inputBits=[bits(a)for a in p],expectedBits=parts(mp.log(value(p))))for p in inputs],atan=[dict(yBits=[bits(a)for a in y],xBits=[bits(a)for a in x],expectedBits=parts(mp.atan2(value(y),value(x))))for y,x in pairs]),indent=2))
