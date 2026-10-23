"""Independent reference generation, Python stdlib only; never production code.

Uses complex ABCD hyperbolic telegrapher solution, unlike production's passive
interface wave formula. Very long-line oracle is its exact absorbing limit.
Re-run from repository root; fixed output remains versioned for Rust/Node tests.
"""
import cmath
import json
import math
from pathlib import Path

def c(z): return dict(real=z.real, imaginary=z.imag)

def case(name, r, l, g, capacitance, length, f=1e9):
    n=len(r)
    modes=[]
    wave=[]
    for k in range(n):
        sign=1 if k==0 else -1
        modal=lambda m: m[0][0] if n==1 else (m[0][0]+m[1][1]+sign*(m[0][1]+m[1][0]))/2
        R,L,G,C=map(modal,[r,l,g,capacitance])
        w=2*math.pi*f
        z,y=complex(R,w*L),complex(G,w*C)
        gamma=cmath.sqrt(z*y)
        zc=cmath.sqrt(z/y)
        if gamma.real*length>700:
            reflection=(zc-50)/(zc+50)
            transmission=0j
        else:
            A=cmath.cosh(gamma*length)
            B=zc*cmath.sinh(gamma*length)
            C=cmath.sinh(gamma*length)/zc
            denominator=2*A+B/50+C*50
            reflection=(B/50-C*50)/denominator
            transmission=2/denominator
        modes.append(dict(name='single' if n==1 else 'even' if k==0 else 'odd',resistanceOhmPerM=R,conductanceSPerM=G,alphaNpPerM=gamma.real,betaRadPerM=gamma.imag,characteristicImpedanceOhm=c(zc)))
        wave.append((reflection,transmission))
    s=[[0j]*(2*n) for _ in range(2*n)]
    # Explicit orthonormal basis vectors, rather than production index weights.
    U=[[1]] if n==1 else [[1/math.sqrt(2),1/math.sqrt(2)],[1/math.sqrt(2),-1/math.sqrt(2)]]
    for i in range(2*n):
        for j in range(2*n):
            s[i][j]=sum(U[i%n][k]*U[j%n][k]*wave[k][0 if i//n==j//n else 1] for k in range(n))
    result=dict(name=name,frequencyHz=f,lengthM=length,referenceOhm=50,R=r,L=l,G=g,C=capacitance,modes=modes,scattering=[[c(z) for z in row] for row in s])
    if n==2 and length<1:
        incident=[1+.3j,-.7+.2j,.2-.1j,.4+.5j]
        outgoing=[sum(s[i][j]*incident[j] for j in range(4)) for i in range(4)]
        amplitudes=[]
        for k in range(2):
            ai=sum(U[i][k]*incident[i] for i in range(2));bi=sum(U[i][k]*outgoing[i] for i in range(2))
            V=math.sqrt(50)*(ai+bi);I=(ai-bi)/math.sqrt(50)
            zc=complex(modes[k]['characteristicImpedanceOhm']['real'],modes[k]['characteristicImpedanceOhm']['imaginary'])
            gamma=complex(modes[k]['alphaNpPerM'],modes[k]['betaRadPerM'])
            amplitudes.append(((V+zc*I)/2,(V-zc*I)/2,zc,gamma))
        def local_power(x):
            V=[];I=[]
            for A,B,zc,gamma in amplitudes:
                forward=A*cmath.exp(-gamma*x);reverse=B*cmath.exp(gamma*x)
                V.append(forward+reverse);I.append((forward-reverse)/zc)
            physical_V=[sum(U[i][k]*V[k] for k in range(2)) for i in range(2)]
            physical_I=[sum(U[i][k]*I[k] for k in range(2)) for i in range(2)]
            return .5*sum((physical_I[i].conjugate()*r[i][j]*physical_I[j]+physical_V[i].conjugate()*g[i][j]*physical_V[j]).real for i in range(2) for j in range(2))
        count=2048;dx=length/count
        power=dx/3*(local_power(0)+local_power(length)+sum((4 if k%2 else 2)*local_power(k*dx) for k in range(1,count)))
        incoming=.5*sum(abs(a)**2 for a in incident);escaped=.5*sum(abs(b)**2 for b in outgoing)
        result['independentPower']=dict(incident=[c(a) for a in incident],incidentW=incoming,outgoingW=escaped,integratedDissipatedW=power,normalizedResidual=abs(incoming-escaped-power)/incoming,method='Python complex ABCD boundary solution; Simpson 2048 physical-space quadratic R/G integral, including mutual terms')
    return result

rows=[]
for length in [.02,.04,100000]:
    rows.append(case('single mismatch '+str(length),[[10]],[[4e-7]],[[.002]],[[1e-10]],length))
    rows.append(case('coupled full matrix '+str(length),[[10,2],[2,10]],[[4e-7,1e-7],[1e-7,4e-7]],[[.002,-.0005],[-.0005,.002]],[[1e-10,-2e-11],[-2e-11,1e-10]],length))
rows.append(case('matched lossy',[[10]],[[2.5e-7]],[[.004]],[[1e-10]],.2))
rows.append(case('nearly lossless',[[1e-10]],[[4e-7]],[[1e-15]],[[1e-10]],.1))
rows.append(case('lossless',[[0]],[[4e-7]],[[0]],[[1e-10]],.1))
output=dict(status='R1 and reviewer-authorized R2 equivalent physical references accepted; interactive HTML controls remain unverified',
    conventions='exp(+j omega t), currents into both ends, peak phasors; physical matrices per metre',
    sources=[dict(url='https://qucs.sourceforge.net/tech/node75.html',scope='single engineering attenuation, not a pair oracle'),dict(url='https://www.sonnetsoftware.com/support/downloads/techdocs/miccondloss_mar03.pdf',scope='surface impedance validity and side currents'),dict(url='https://doi.org/10.1109/75.222778',scope='candidate pair loss reference; numeric data pending; not yet a validation')],
    plate=dict(widthM=.002,heightM=.001,epsilonR=4,conductivitySM=5.8e7,frequencyHz=1e9,lossTangent=.015,gramEachPerM=500,RTotalOhmPerM=math.sqrt(math.pi*1e9*1.25663706212e-6/5.8e7)*1000,LinternalTotalHPerM=math.sqrt(math.pi*1e9*1.25663706212e-6/5.8e7)*1000/(2*math.pi*1e9),C_FPerM=8.8541878128e-12*4*2,G_SPerM=2*math.pi*1e9*8.8541878128e-12*4*2*.015),
    propagation=rows)
complex_capacity=8.8541878128e-12*.002/(.0005/complex(2,-2*.015)+.0005/complex(6,-6*.035))
output['layeredPlate']=dict(widthM=.002,bottomHeightM=.0005,topHeightM=.0005,bottomEpsilonR=2,topEpsilonR=6,bottomTangent=.015,topTangent=.035,frequencyHz=1e9,exactCapacitanceFPerM=complex_capacity.real,exactConductanceSPerM=-2*math.pi*1e9*complex_capacity.imag,discardedTerms='first-order perturbation discards O(tan delta squared) redistribution; compare exact series complex capacitance')
engineering=[]
for width,height,thickness,frequency in [(.3,.18,.005,20e9),(1.,.3,.01,5e9)]:
    u=width/height;tn=thickness/height
    delta_u=tn/math.pi*math.log(1+4*math.e/(tn/(math.tanh(math.sqrt(6.517*u))**2)))
    corrected=u+delta_u
    wave=math.sqrt(1.25663706212e-6/8.8541878128e-12)
    F=6+(2*math.pi-6)*math.exp(-(30.666/corrected)**.7528)
    impedance=wave/(2*math.pi)*math.log(F/corrected+math.sqrt(1+(2/corrected)**2))
    K=math.exp(-1.2*(impedance/wave)**.7)
    rs=math.sqrt(math.pi*frequency*1.25663706212e-6/5.8e7)
    engineering.append(dict(kind='microstrip',widthMm=width,dielectricHeightMm=height,copperThicknessMm=thickness,relativePermittivity=1,lossTangent=0,frequencyHz=frequency,conductivitySM=5.8e7,referenceThicknessMm=.035,impedanceOhm=impedance,conductorAlphaNpPerM=rs*K/(impedance*width*1e-3),source='Qucs node75 eqs 11.4,11.6,11.22,11.80-11.82; air substrate removes impedance notation ambiguity; smooth surfaces Kr=1; engineering approximation, no claimed published error bound',tolerance=.1))
output['singleEngineering']=engineering
output['reviewDecisions']=dict(r1=dict(date='2026-10-23',approved=True,physicalReference='boundary-panel-references.json',numericalTolerance=.05,engineeringQucs='Retained approximate comparison, original inputs and 10% goal; historical errors12.34959%/11.22408% unchanged'),r2Equivalent=dict(approved=True,date='2026-10-23',status='Reviewer closes equivalent criterion using full-R power, both modes, each surface at two gaps plus supplementary uncertain Johnson R; original literal published both-mode requirement replaced, not met literally'))
Path('tests/fixtures/analysis-line-losses/references.json').write_text(json.dumps(output,indent=2)+'\n')
