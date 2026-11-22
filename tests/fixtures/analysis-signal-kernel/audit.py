"""Saved-array audit and numeric plots. No FFT package or native calculation."""
import hashlib, json, math, pathlib, sys
import matplotlib
matplotlib.use('Agg')
import matplotlib.pyplot as plt

root = pathlib.Path(__file__).resolve().parents[3]
out = pathlib.Path(sys.argv[1]).resolve()
fixture = pathlib.Path(__file__).parent
definition = json.loads((fixture/'definition.json').read_text())
frozen = json.loads((fixture/'frozen-manifest.json').read_text())
# The frozen manifest is an input commitment, not refreshed during this audit.
for name, sha in frozen.items():
    assert hashlib.sha256((fixture/name).read_bytes()).hexdigest() == sha
a = json.loads((out/'inputs.json').read_text())
b = json.loads((out/'arrays.json').read_text())
x = [complex(*z) for z in a['samples']]
h = [complex(*z) for z in a['kernel']]
y = [complex(*z) for z in b['convolution']]
s = b['spectrum']
n, dt = len(x), a['dtSeconds']
g = definition['gates']
ref = [sum(x[k]*h[i-k] for k in range(max(0,i-len(h)+1),min(i+1,n))) for i in range(512)]
conv_error = max(abs(y[i]-v) for i,v in enumerate(ref))
conv_scale = sum(map(abs,x))*sum(map(abs,h))
assert conv_error <= g['convolutionAbsolute']+g['convolutionRelativeToProductL1']*conv_scale
assert len(y) == n+len(h)-1
short = [sum(v*complex(math.cos(-math.tau*k*j/512),math.sin(-math.tau*k*j/512)) for j,v in enumerate(x[:512])) for k in range(512)]
dft_error = max(abs(complex(*z)-v) for z,v in zip(b['shortFft'],short))
assert dft_error <= g['dftAbsolute']+g['dftRelativeToInputL1']*sum(map(abs,x[:512]))
energy = weighted = 0.
for i,z in enumerate(x):
    w = .5-.5*math.cos(math.tau*i/n)
    energy += w*w
    weighted += abs(z*w)**2
expected = weighted/energy
integral = sum(row[4] for row in s)/(n*dt)
assert abs(integral-expected) <= g['energyAbsolute']+g['energyRelative']*expected
assert all(math.isfinite(v) for row in s for v in row)
assert len(s) == n//2+1 and s[0][0] == 0 and s[-1][0] == .5/dt
for k,row in enumerate(s):
    fold = 1 if k in (0,n//2) else 2
    coefficient = complex(row[1],row[2])
    amplitude = fold*abs(coefficient)/(n/2)
    psd = fold*abs(coefficient)**2/((1/dt)*energy)
    assert abs(row[3]-amplitude) <= 1e-12+5e-11*amplitude
    assert abs(row[4]-psd) <= 1e-24+5e-11*psd

fig,ax = plt.subplots(2,1,figsize=(11,7),layout='constrained')
count = 4096
ax[0].plot([i*dt for i in range(count)],[z.real for z in x[:count]],label='finite input [V]',linewidth=.8)
ax[0].plot([i*dt for i in range(count)],[z.real for z in y[:count]],label='discrete convolution [V]',linewidth=1)
ax[0].set(xlabel='Time [s]',ylabel='Voltage [V]',title='First 4 s of the finite 128 s record; zero exterior for linear convolution')
ax[0].legend(); ax[0].grid(alpha=.2)
ax[1].plot([i*dt for i in range(512)],[y[i].real for i in range(512)],label='FFT linear convolution',linewidth=1)
ax[1].plot([i*dt for i in range(0,512,8)],[ref[i].real for i in range(0,512,8)],'.',label='independent direct sum (every 8th shown)',markersize=3)
ax[1].set(xlabel='Time [s]',ylabel='Voltage [V]',title=f'Direct 512-sample prefix: maximum error {conv_error:.3g} V')
ax[1].legend(); ax[1].grid(alpha=.2)
fig.savefig(out/'time.png',dpi=150); plt.close(fig)
fig,ax = plt.subplots(2,1,figsize=(11,7),layout='constrained')
ax[0].plot([v[0] for v in s],[v[3] for v in s],linewidth=.8)
ax[0].set(xlim=(0,25),xlabel='Frequency [Hz]',ylabel='Peak amplitude [V]',title='Periodic Hann, N=P=131072, Fs=1024 Hz; coherent-gain normalization')
ax[0].annotate(f'DC: {s[0][3]:.3g} V',xy=(0,s[0][3]),xytext=(3,.65),arrowprops={'arrowstyle':'->'})
peak = max(s[1600:1850],key=lambda v:v[3])
ax[0].annotate('Off-bin tone: finite-bin amplitude',xy=(peak[0],peak[3]),xytext=(15,.5),arrowprops={'arrowstyle':'->'})
ax[0].grid(alpha=.2)
ax[1].semilogy([v[0] for v in s],[max(v[4],1e-32) for v in s],linewidth=.8)
ax[1].set(xlabel='Frequency [Hz]',ylabel='PSD [V²/Hz]',title=f'Energy normalization: PSD integral {integral:.8g} V², independent weighted mean {expected:.8g} V²')
ax[1].grid(alpha=.2)
fig.savefig(out/'spectrum.png',dpi=150); plt.close(fig)
report = dict(status='pass',frozenInputHashesVerified=True,convolutionPrefixMax=conv_error,convolutionScale=conv_scale,shortDftMax=dft_error,psdIntegral=integral,weightedMeanSquare=expected,psdIntegralError=abs(integral-expected),inputCount=n,outputCount=len(y),spectrumBins=len(s))
(out/'audit.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report))
