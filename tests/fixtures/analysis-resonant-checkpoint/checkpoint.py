"""Isolated F64 experiment. No imports from, or changes to, production.
prepare freezes samples before run. run enforces a Windows job memory/CPU limit
and a wall deadline BEFORE importing numpy. plot only reads saved outputs.
"""
import os, sys, json, math, time, hashlib, pathlib, ctypes, threading
ROOT = pathlib.Path(__file__).resolve().parent
OUT = ROOT.parents[2] / 'debugging' / 'p6-resonant-checkpoint'
DEF = json.loads((ROOT / 'definition.json').read_text())
W = 2 * math.pi * 1e6

def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, separators=(',', ':'), allow_nan=False) + '\n')

def budget():
    if os.name != 'nt':
        raise RuntimeError('This supervisor is Windows-only; no unbounded fallback')
    from ctypes import wintypes as wt
    class Basic(ctypes.Structure):
        _fields_ = [('processTime',ctypes.c_longlong),('jobTime',ctypes.c_longlong),
            ('flags',wt.DWORD),('minWorkingSet',ctypes.c_size_t),('maxWorkingSet',ctypes.c_size_t),
            ('active',wt.DWORD),('affinity',ctypes.c_size_t),('priority',wt.DWORD),('scheduling',wt.DWORD)]
    class Extended(ctypes.Structure):
        _fields_ = [('basic',Basic),('io',ctypes.c_ulonglong*6),('processMemory',ctypes.c_size_t),
            ('jobMemory',ctypes.c_size_t),('peakProcess',ctypes.c_size_t),('peakJob',ctypes.c_size_t)]
    api = ctypes.WinDLL('kernel32', use_last_error=True)
    api.CreateJobObjectW.restype = wt.HANDLE
    api.CreateJobObjectW.argtypes = [ctypes.c_void_p, wt.LPCWSTR]
    api.SetInformationJobObject.argtypes = [wt.HANDLE,ctypes.c_int,ctypes.c_void_p,wt.DWORD]
    api.AssignProcessToJobObject.argtypes = [wt.HANDLE,wt.HANDLE]
    api.GetCurrentProcess.restype = wt.HANDLE
    job = api.CreateJobObjectW(None, None)
    info = Extended(); info.basic.flags = 0x2 | 0x100
    info.basic.processTime = 60 * 10000000
    info.processMemory = 512 * 1024 * 1024
    if not job or not api.SetInformationJobObject(job,9,ctypes.byref(info),ctypes.sizeof(info)) or not api.AssignProcessToJobObject(job,api.GetCurrentProcess()):
        raise ctypes.WinError(ctypes.get_last_error())
    timer = threading.Timer(60, lambda: os._exit(124)); timer.daemon=True; timer.start()
    return job,timer

if len(sys.argv) > 1 and sys.argv[1] == 'run':
    JOB,TIMER = budget()
for var in ['OPENBLAS_NUM_THREADS','OMP_NUM_THREADS','MKL_NUM_THREADS']:
    os.environ[var]='1'
import numpy as np

def stats():
    class Counters(ctypes.Structure):
        _fields_=[('cb',ctypes.c_ulong),('faults',ctypes.c_ulong)]+[(n,ctypes.c_size_t) for n in
            ['peakWorkingSet','workingSet','peakPaged','paged','peakNonpaged','nonpaged','pagefile','peakPagefile']]
    c=Counters(); c.cb=ctypes.sizeof(c)
    kernel=ctypes.WinDLL('kernel32'); kernel.GetCurrentProcess.restype=ctypes.c_void_p
    psapi=ctypes.WinDLL('psapi'); psapi.GetProcessMemoryInfo.argtypes=[ctypes.c_void_p,ctypes.c_void_p,ctypes.c_ulong]
    if not psapi.GetProcessMemoryInfo(kernel.GetCurrentProcess(),ctypes.byref(c),c.cb): raise RuntimeError('memory measurement failed')
    return {'peakWorkingSetBytes':c.peakWorkingSet,'peakCommitBytes':c.peakPagefile}

def basis(case):
    if len(case['modes'])==1:return np.eye(1)
    a=case['angle']; return np.array([[math.cos(a),-math.sin(a)],[math.sin(a),math.cos(a)]])

def scale(case):return 1/case['normalizingR'] if case['parameter']=='Y' else case['normalizingR']

def coeff(case):
    rows=[]
    for m in case['modes']:
        R,L,C=m['R'],m['L'],m['C']
        k,a,b=(1/L,R/L,1/(L*C)) if case['parameter']=='Y' else (1/C,1/(R*C),1/(L*C))
        rows.append([k/(scale(case)*W),a/W,b/W**2])
    return np.array(rows)

def modal(z, coef):
    return np.stack([K*z/(z*z+A*z+B) for K,A,B in coef],axis=-1)

def matrix(z, coef, U):return np.einsum('ik,fk,jk->fij',U,modal(z,coef),U)

def independent_ac(case, f):
    # Physical impedance/admittance inversion, not modal transfer evaluation.
    U=basis(case); n=len(U); answer=[]
    R=U@np.diag([m['R'] for m in case['modes']])@U.T
    L=U@np.diag([m['L'] for m in case['modes']])@U.T
    C=U@np.diag([m['C'] for m in case['modes']])@U.T
    def inverse(M):
        if n==1:return 1/M
        return np.array([[M[1,1],-M[0,1]],[-M[1,0],M[0,0]]])/(M[0,0]*M[1,1]-M[0,1]*M[1,0])
    for hz in f:
        if hz==0:answer.append(np.zeros((n,n),complex));continue
        s=2j*np.pi*hz
        Z=R+s*L+inverse(C)/s if case['parameter']=='Y' else None
        H=inverse(Z) if Z is not None else inverse(inverse(R)+s*C+inverse(L)/s)
        answer.append(H/scale(case))
    return np.array(answer)

def encode(x):return np.stack([x.real,x.imag],axis=-1).tolist()
def decode(x):
    def pair(v):return [v['real'],v['imaginary']] if isinstance(v,dict) else [pair(a) for a in v]
    a=np.array(pair(x));return a[...,0]+1j*a[...,1]

def prepare():
    for case in DEF['cases']:
        c=coeff(case); U=basis(case)
        for count in DEF['trainingCounts']:
            train=np.r_[0,np.geomspace(*DEF['frequencyHz'],count)]
            hold=np.sqrt(train[1:-1]*train[2:])
            def resource(f):
                values=matrix(2j*np.pi*f/W,c,U)*scale(case)
                return {'id':case['name']+'-synthetic','kind':'network','ports':['p'+str(i+1) for i in range(len(U))],
                    'source':'Authored synthetic RLC; not extracted or measured',
                    'data':{'format':'network-parameters',
                    'representation':case['parameter'],'referenceTopology':'common','nature':'passive',
                    'frequenciesHz':f.tolist(),'parameters':[[[{'real':float(v.real),'imaginary':float(v.imag)} for v in row] for row in mat] for mat in values]}}
            contract={'orthonormalModes':U.tolist(),'family':'one-resonant-section-per-mode',
                'closure':{'dc':np.zeros(len(U)).tolist(),'normalizedDcSlope':(c[:,0]/c[:,2]).tolist(),
                    'normalizedHighFrequencyK':c[:,0].tolist()},'normalization':{'omega':W,'immittance':scale(case)}}
            save(ROOT/f'{case["name"]}-{count}.json',{'resource':resource(train),
                'independentHoldout':resource(hold),'checkpointContract':contract})
    files=[ROOT/'definition.json',*[ROOT/f'{c["name"]}-{n}.json' for c in DEF['cases'] for n in DEF['trainingCounts']]]
    save(ROOT/'frozen-manifest.json',{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in files})
    print('Frozen fixtures:',len(files))

def verify_frozen():
    for name,digest in json.loads((ROOT/'frozen-manifest.json').read_text()).items():
        if hashlib.sha256((ROOT/name).read_bytes()).hexdigest()!=digest:raise ValueError('fixture hash mismatch '+name)

def preflight(data,rise):
    contract=data['checkpointContract']
    if 'closure' not in contract:raise ValueError('authored DC slope/HF closure required')
    U=np.array(contract['orthonormalModes']); n=len(U)
    if U.shape!=(n,n) or not np.all(np.isfinite(U)) or np.max(abs(U.T@U-np.eye(n)))>1e-12:raise ValueError('orthonormal modal transform required')
    r=data['resource']['data'];f=np.array(r['frequenciesHz']);H=decode(r['parameters'])/contract['normalization']['immittance']
    if f[0]!=0 or np.any(np.diff(f)<=0) or not np.all(np.isfinite(H)):raise ValueError('finite ordered samples including DC required')
    if rise<10/f[-1]:raise ValueError('source band incompatible')
    if np.max(abs(H-H.transpose(0,2,1)))>1e-10:raise ValueError('reciprocity required')
    if np.min(np.linalg.eigvalsh((H+H.conj().transpose(0,2,1))/2)) < -1e-10:raise ValueError('active sampled immittance')
    M=np.einsum('ik,fij,jl->fkl',U,H,U)
    off=M.copy()
    for i in range(n):off[:,i,i]=0
    if np.max(abs(off))>1e-9*np.max(abs(M))+1e-10:raise ValueError('constant authored modes incompatible')
    if np.max(abs(H[0]))>1e-10:raise ValueError('DC closure requires zero')

def ls(A,b):
    A=np.vstack([A.real,A.imag]); b=np.r_[b.real,b.imag]
    norms=np.linalg.norm(A,axis=0)
    if np.any(norms==0):raise ValueError('conditioning zero column')
    An=A/norms; sv=np.linalg.svd(An,compute_uv=False); condition=sv[0]/sv[-1]
    if condition>DEF['gates']['conditionMaximum']:raise ValueError('conditioning '+str(condition))
    x=np.linalg.lstsq(An,b,rcond=None)[0]/norms
    return x,float(condition)

def structural(z,h):
    x,condition=ls(np.stack([np.ones(len(z)),z,1/z],axis=1),1/h)
    if np.any(x<=0):raise ValueError('nonpositive RLC coefficient')
    r,l,c=x;return np.array([1/l,r/l,c/l]),condition

def atom_order(z,h,seed,order):
    # Positive parallel sections, fixed total K. Exhaustive simplex faces <=4.
    count=order//2; factors=[1,.6,1.5,2.4][:count]
    atoms=np.array([[seed[0],seed[1]*f,seed[2]*f*f] for f in factors])
    F=modal(z,atoms); best=None
    for mask in range(1,1<<count):
        ids=[j for j in range(count) if mask>>j&1]
        if len(ids)==1:w=np.ones(1)
        else:
            # Eliminate equality sum(w)=1 before QR.
            try:
                x,_=ls(F[:,ids[:-1]]-F[:,ids[-1],None],h-F[:,ids[-1]])
            except ValueError:continue
            w=np.r_[x,1-x.sum()]
        if np.any(w<0):continue
        err=float(np.linalg.norm(F[:,ids]@w-h)**2)
        if best is None or err<best[0]-1e-24 or (abs(err-best[0])<=1e-24 and len(ids)<len(best[1])):best=(err,ids,w)
    _,ids,w=best
    return atoms[ids]*np.array([w,np.ones(len(w)),np.ones(len(w))]).T,{'activeSections':len(ids),'weights':w.tolist()}

def vector_fit(z,h,order,K,dcSlope):
    # Original VF sigma pole relocation in a real conjugate-pair basis.
    peak=abs(z[np.argmax(abs(h))]); centers=peak*np.geomspace(.6,1.5,order//2) if order>2 else [peak]
    poles=np.array([p for f in centers for p in [-.05*f+1j*f,-.05*f-1j*f]])
    history=[]
    for _ in range(8):
        pairs=[]
        for p in poles[::2]:
            pairs.extend([1/(z-p)+1/(z-p.conjugate()),1j/(z-p)-1j/(z-p.conjugate())])
        phi=np.stack(pairs,axis=1)
        x,condition=ls(np.c_[phi,-h[:,None]*phi],h)
        c=np.array([q for a,b in x[order:].reshape(-1,2) for q in [a+1j*b,a-1j*b]])
        new=np.linalg.eigvals(np.diag(poles)-np.ones((order,1))*c[None,:])
        if np.any(new.real>=0):raise ValueError('unstable relocated pole; no reflection')
        pos=sorted([p for p in new if p.imag>1e-10],key=lambda p:p.imag)
        if len(pos)!=order//2:raise ValueError('outside conjugate resonant family')
        new=np.array([p for a in pos for p in [a,a.conjugate()]])
        delta=float(np.max(abs(new-poles))/(1+np.max(abs(poles))));poles=new
        history.append({'condition':condition,'poleChange':delta})
        if delta<1e-11:break
    if order!=2:raise ValueError('higher-order VF lacks structural passivity certificate')
    # Constrained final residue solve: authored DC=0 and HF coefficient K.
    p=poles[0]; result=np.array([K,-2*p.real,abs(p)**2])
    if abs(result[0]/result[2]-dcSlope)>DEF['gates']['closureRelative']*abs(dcSlope)+DEF['gates']['closureAbsoluteNormalized']:
        raise ValueError('DC slope closure')
    return result,{'iterations':history,'polesNormalized':encode(poles),'certificate':'positive one-section RLC'}

def error(got,ref,relative):
    gate=DEF['gates']; diff=abs(got-ref)
    return {'normalizedMax':float(np.max(diff/(abs(ref)+gate['matrixAbsoluteNormalized']/relative))),
        'absoluteNormalizedMax':float(np.max(diff)),
        'pass':bool(np.all(diff<=relative*abs(ref)+gate['matrixAbsoluteNormalized']))}

def resolution(z,coefs):
    f=abs(z); result=[]
    for K,A,B in coefs:
        f0=math.sqrt(B); width=A
        j=np.searchsorted(f,f0); gap=f[min(j,len(f)-1)]-f[max(j-1,0)]
        result.append(width/gap)
    return min(result)

def analytic(case,c,t):
    U=basis(case); k=c[:,0]*scale(case)*W; a=c[:,1]*W; b=c[:,2]*W*W
    gamma=a+case['load']*k; alpha=gamma/2; beta=np.sqrt(b-alpha*alpha)
    x=np.zeros((len(t),len(c),2));rise=DEF['time']['riseSeconds']
    for physical,(amplitude,delay) in enumerate(zip(case['sourceSI'],case['delaySeconds'])):
        def integrated_step(v):
            v=np.maximum(v,0)[:,None]; p=-alpha+1j*beta; e=np.expm1(v*p)/p
            return np.stack([(v-np.real((1-1j*alpha/beta)*e))/b,np.imag(e)/beta],axis=-1)
        x+=(integrated_step(t-delay)-integrated_step(t-delay-rise))/rise*(amplitude*U[physical])[None,:,None]
    y=(x[:,:,1]*k)@U.T
    energy=.5*np.sum(k*(b*x[:,:,0]**2+x[:,:,1]**2),axis=1)
    return y,energy

def transient(case,c,h):
    U=basis(case); k=c[:,0]*scale(case)*W; a=c[:,1]*W; b=c[:,2]*W*W
    config=DEF['time']; steps=round(config['endSeconds']/h);stride=round(config['sampleSeconds']/h)
    x=np.zeros(2*len(c)+2);samples=[];times=[]
    def rhs(t,x):
        v=x[:2*len(c)].reshape(-1,2); y=k*v[:,1]
        physical=np.array(case['sourceSI'])*np.clip((t-np.array(case['delaySeconds']))/config['riseSeconds'],0,1)
        u=physical@U
        dx=np.stack([v[:,1],u-(a+case['load']*k)*v[:,1]-b*v[:,0]],axis=1)
        return np.r_[dx.ravel(),np.dot(u,y),np.sum((a/k+case['load'])*y*y)]
    for i in range(steps+1):
        t=i*h
        if i%stride==0:samples.append(x.copy());times.append(t)
        if i==steps:break
        q1=rhs(t,x);q2=rhs(t+h/2,x+h*q1/2);q3=rhs(t+h/2,x+h*q2/2);q4=rhs(t+h,x+h*q3)
        x+=h/6*(q1+2*q2+2*q3+q4)
    samples=np.array(samples);t=np.array(times);v=samples[:,:2*len(c)].reshape(-1,len(c),2)
    y=(v[:,:,1]*k)@U.T;E=.5*np.sum(k*(b*v[:,:,0]**2+v[:,:,1]**2),axis=1)
    balance=samples[:,-2]-samples[:,-1]-E
    norm=np.maximum(abs(samples[:,-2])+samples[:,-1]+E,1e-30)
    return t,y,E,samples[:,-2],samples[:,-1],balance,float(np.max(abs(balance)/norm))

def run_case(name):
    case=next(c for c in DEF['cases'] if c['name']==name);U=basis(case);report={'case':name,'grids':[]};fine=None
    for count in DEF['trainingCounts']:
        data=json.loads((ROOT/f'{name}-{count}.json').read_text());contract=data['checkpointContract'];preflight(data,DEF['time']['riseSeconds'])
        train=data['resource']['data'];hold=data['independentHoldout']['data']
        f=np.array(train['frequenciesHz']);fh=np.array(hold['frequenciesHz']);z=2j*np.pi*f[1:]/W;zh=2j*np.pi*fh/W
        H=decode(train['parameters'])/scale(case); reference=independent_ac(case,fh)
        h=np.einsum('ik,fij,jk->fk',U,H[1:],U);seed=[];conditions=[]
        for mode in range(len(U)):
            c,condition=structural(z,h[:,mode]);seed.append(c);conditions.append(condition)
        seed=np.array(seed)
        for i,c in enumerate(seed):
            closure=contract['closure'];tol=DEF['gates']['closureRelative'];floor=DEF['gates']['closureAbsoluteNormalized']
            for actual,expected in [(c[0],closure['normalizedHighFrequencyK'][i]),(c[0]/c[2],closure['normalizedDcSlope'][i])]:
                if abs(actual-expected)>tol*abs(expected)+floor:raise ValueError('closure mismatch')
        widths=resolution(z,seed);grid={'count':count,'samplesPerLinewidth':widths,'resolutionPass':bool(widths>=8),'conditions':conditions,'orders':[]}
        previous=None
        for order in DEF['orders']:
            atoms=[];active=[]
            for m,c in enumerate(seed):
                atom,info=atom_order(z,h[:,m],c,order);atoms.append(atom);active.append(info)
            response=np.stack([modal(zh,atom).sum(axis=1) for atom in atoms],axis=-1)
            full=np.einsum('ik,fk,jk->fij',U,response,U)
            entry={'nominalOrderPerMode':order,'sections':active,'holdout':error(full,reference,.005),
                'training':error(matrix(z,seed,U),H[1:],.005)}
            if previous is not None:entry['refinement']=error(full,previous,.001)
            previous=full
            try:
                fitted=[];vfinfo=[]
                for m in range(len(U)):
                    c,info=vector_fit(z,h[:,m],order,contract['closure']['normalizedHighFrequencyK'][m],contract['closure']['normalizedDcSlope'][m]);fitted.append(c);vfinfo.append(info)
                vf=np.array(fitted);entry['vectorFitting']={'accepted':True,'details':vfinfo,'coefficients':vf.tolist(),'holdout':error(matrix(zh,vf,U),reference,.005)}
            except ValueError as exc:entry['vectorFitting']={'accepted':False,'reason':str(exc)}
            grid['orders'].append(entry)
        grid['coefficients']=seed.tolist();report['grids'].append(grid)
        if count==DEF['trainingCounts'][-1]:fine=seed
    times=[];traces=[]
    for step in DEF['time']['stepsSeconds']:
        t,y,E,work,loss,balance,relative=transient(case,fine,step);ref,eref=analytic(case,coeff(case),t)
        maxerr=float(np.max(abs(y-ref)));eerr=float(np.max(abs(E-eref)))
        gate=DEF['gates'];passed=maxerr<=.01*case['outputScaleSI']+gate['transientAbsoluteSI'] and eerr<=gate['energyReferenceFractionOfScale']*case['energyScaleJ']+gate['energyAbsoluteJ'] and bool(np.all(abs(balance)<=gate['energyBalanceRelative']*(abs(work)+loss+E)+gate['energyAbsoluteJ']))
        traces.append(y);times.append({'stepSeconds':step,'outputErrorSI':maxerr,'fractionOfScale':maxerr/case['outputScaleSI'],'energyReferenceErrorJ':eerr,'balanceMaxJ':float(np.max(abs(balance))),'balanceRelativeMax':relative,'pass':passed})
    report['time']=times;report['timeRefinementFraction']=float(np.max(abs(traces[0]-traces[1]))/case['outputScaleSI'])
    vfcoefs=report['grids'][-1]['orders'][0]['vectorFitting']
    if vfcoefs['accepted']:
        _,vy,_,_,_,_,_=transient(case,np.array(vfcoefs['coefficients']),DEF['time']['stepsSeconds'][-1])
        report['vectorTimeFraction']=float(np.max(abs(vy-ref))/case['outputScaleSI'])
    np.savez(OUT/f'{name}-traces.npz',time=t,output=y,reference=ref,energy=E,energyReference=eref,work=work,loss=loss,balance=balance)
    fplot=np.geomspace(*DEF['frequencyHz'],4097);np.savez(OUT/f'{name}-ac.npz',frequency=fplot,fit=matrix(2j*np.pi*fplot/W,fine,U),reference=independent_ac(case,fplot))
    report['pass']=all(g['orders'][0]['holdout']['pass'] and all(o['holdout']['pass'] and o.get('refinement',{'pass':True})['pass'] for o in g['orders']) for g in report['grids'] if g['resolutionPass']) and all(t['pass'] for t in times) and vfcoefs['accepted'] and vfcoefs['holdout']['pass'] and report.get('vectorTimeFraction',1)<=.01
    return report

def negatives():
    case=DEF['cases'][0];d=json.loads((ROOT/'series-1025.json').read_text());t=d['resource']['data'];f=np.array(t['frequenciesHz'])[1:];z=2j*np.pi*f/W;c=coeff(case);base=modal(z,c)[:,0]
    import copy
    def refusal(data,rise=DEF['time']['riseSeconds']):
        try:preflight(data,rise);return {'rejected':False}
        except ValueError as exc:return {'rejected':True,'reason':str(exc)}
    active=float(np.min((-base).real));active_data=copy.deepcopy(d)
    for mat in active_data['resource']['data']['parameters']:
        for row in mat:
            for v in row:v['real']*=-1;v['imaginary']*=-1
    n={'active':{**refusal(active_data),'minimumHermitianEigenvalue':active,'stablePoles':True}}
    try:structural(1j*(1+np.linspace(-1e-12,1e-12,32)),np.ones(32,dtype=complex));bad=False;reason=''
    except ValueError as exc:bad=True;reason=str(exc)
    n['conditioning']={'rejected':bad,'reason':reason}
    missing_data=copy.deepcopy(d);del missing_data['checkpointContract']['closure']
    n['missingClosure']=refusal(missing_data);missing=n['missingClosure']['rejected']
    n['sourceBand']={**refusal(d,1e-9),'riseSeconds':1e-9,'minimumSeconds':10/DEF['frequencyHz'][1]}
    co=coeff(DEF['cases'][1]);coarse=np.geomspace(*DEF['frequencyHz'],257)/1e6
    width=resolution(1j*coarse,co);n['insufficientGrid']={'samplesPerLinewidth':float(width),'rejected':bool(width<8)}
    # Invisible between sampled points: finite-grid success is NOT a certificate.
    j=np.searchsorted(f,1.5e6);fh=f[j-1]*(f[j]/f[j-1])**DEF['negativeFixtures']['hidden']['logCellFraction'];b=(fh/1e6)**2
    hidden=np.array([[c[0,0]*1e-7,math.sqrt(b)/1e7,b]])
    hold=np.array(d['independentHoldout']['data']['frequenciesHz']);zh=1j*hold/1e6
    combined=base+modal(z,hidden)[:,0];fit,condition=structural(z,combined)
    finite=error(modal(zh,fit[None,:])[:,0],modal(zh,c)[:,0]+modal(zh,hidden)[:,0],.005)
    oracle=1j*np.array([fh/1e6]);miss=error(modal(oracle,fit[None,:])[:,0],modal(oracle,c)[:,0]+modal(oracle,hidden)[:,0],.005)
    n['hiddenResonance']={'frequencyHz':fh,'Q':1e7,'finiteHoldout':finite,'oracleAtPeak':miss,'condition':condition,'rejectedAsUncertified':finite['pass'] and not miss['pass'],'lesson':'Finite samples cannot establish authored family or exclude unknown intersample resonances.'}
    n['pass']=active<0 and bad and missing and n['sourceBand']['rejected'] and n['insufficientGrid']['rejected'] and n['hiddenResonance']['rejectedAsUncertified']
    return n

def plot():
    import matplotlib;matplotlib.use('Agg')
    import matplotlib.pyplot as plt
    for case in DEF['cases']:
        name=case['name'];ac=np.load(OUT/f'{name}-ac.npz');tr=np.load(OUT/f'{name}-traces.npz');n=len(case['modes'])
        fig,axes=plt.subplots(n,n,figsize=(8,5),squeeze=False)
        for i in range(n):
            for j in range(n):
                ax=axes[i,j];ax.semilogx(ac['frequency'],ac['reference'][:,i,j].real,label='analytic Re');ax.semilogx(ac['frequency'],ac['reference'][:,i,j].imag,label='analytic Im');ax.semilogx(ac['frequency'],ac['fit'][:,i,j].real,'--',label='fit Re');ax.semilogx(ac['frequency'],ac['fit'][:,i,j].imag,':',label='fit Im');ax.set_title(f'{case["parameter"]}{i+1}{j+1} / {scale(case):g} SI');ax.set_xlabel('Hz');ax.grid();ax.legend(fontsize=7)
        fig.tight_layout();fig.savefig(OUT/f'{name}-ac.png',dpi=150);plt.close(fig)
        fig,axes=plt.subplots(3,1,figsize=(8,8));us=tr['time']*1e6
        for i in range(n):axes[0].plot(us,tr['reference'][:,i],label=f'analytic port{i+1}');axes[0].plot(us,tr['output'][:,i],'--',label=f'fit RK4 port{i+1}')
        axes[0].set_ylabel('A' if case['parameter']=='Y' else 'V');axes[0].legend()
        for key in ['energy','energyReference','work','loss']:axes[1].plot(us,tr[key],label=key)
        axes[1].set_ylabel('J');axes[1].legend();axes[2].plot(us,tr['balance']);axes[2].set_ylabel('work - loss - E (J)');axes[2].set_xlabel('microseconds')
        for ax in axes:ax.grid()
        fig.tight_layout();fig.savefig(OUT/f'{name}-time-energy.png',dpi=150);plt.close(fig)
    print('Saved six static figures')

if __name__=='__main__':
    mode=sys.argv[1]
    if mode=='prepare':prepare()
    elif mode=='plot':plot()
    elif mode=='run':
        start=time.perf_counter();cpu=time.process_time();verify_frozen();OUT.mkdir(parents=True,exist_ok=True)
        name=sys.argv[2];result=negatives() if name=='negative' else run_case(name)
        result['resources']={'wallSeconds':time.perf_counter()-start,'cpuSeconds':time.process_time()-cpu,**stats(),'hardLimitMiB':512,'hardWallSeconds':60,'hardCpuSeconds':60,'numericalProcesses':1}
        result['frozenManifestSha256']=hashlib.sha256((ROOT/'frozen-manifest.json').read_bytes()).hexdigest()
        save(OUT/f'{name}-result.json',result);TIMER.cancel();print(json.dumps({'name':name,'pass':result['pass'],'resources':result['resources']}))
        sys.exit(0 if result['pass'] else 1)
    else:raise ValueError('prepare|run CASE|plot')
