"""Independent direct product-volume QMC reference, bounded to24 finite cells.

No production potential, integration transform or magnetic operator is imported.
The randomized-QMC uncertainty is an empirical screen, not a certified bound.
"""
import os
os.environ['OPENBLAS_NUM_THREADS']='1'
from pathlib import Path
import hashlib,json,time,sys
import numpy as np
from scipy.stats import qmc
import psutil

root=Path(__file__).resolve().parents[4]
start=time.perf_counter();peak=0
conditioned='--condition-segment' in sys.argv[1:]
def check():
    global peak
    m=psutil.Process().memory_info();peak=max(peak,m.rss,getattr(m,'peak_wset',m.rss))
    if time.perf_counter()-start>110 or peak>512*1024*1024:
        raise RuntimeError('independent reference deadline/memory refusal')

# Independent authored eight-prism geometry; production node ordering is checked
# explicitly before this reference may be compared with a candidate matrix.
tetra=[]
for i in range(8):
    x=i*.002 if i<4 else .1+(i-4)*.002
    p=np.array([[x,0,0],[x+.001,0,0],[x,.0005,0],
                [x,0,.0005],[x+.001,0,.0005],[x,.0005,.0005]])
    tetra.extend([p[[0,1,2,5]],p[[0,1,4,5]],p[[0,3,4,5]]])
tetra=np.array(tetra);n=len(tetra)
volume=abs(np.linalg.det(tetra[:,1:]-tetra[:,:1]))/6
powers=[14,16,18];seeds=[1307,2719,4513,6337]
selected_seed=next((int(arg.split('=',1)[1]) for arg in sys.argv[1:] if arg.startswith('--seed=')),None)
aggregate='--aggregate' in sys.argv[1:]
reference_power=next((int(arg.split('=',1)[1]) for arg in sys.argv[1:] if arg.startswith('--reference-power=')),18)
if reference_power not in [18,19,20]:raise ValueError('reference power outside predeclared refinement')
powers.extend(range(19,reference_power+1))
if selected_seed is not None:
    if selected_seed not in seeds:raise ValueError('seed outside predeclared set')
    seeds=[selected_seed]
if aggregate and selected_seed is not None:raise ValueError('choose generation or aggregation')
matrices=np.zeros((len(seeds),len(powers),n,n))
for seed_index,seed in enumerate(seeds):
    seed_path=root/f'debugging/p7-board-integration/tetra-space-conditioned-seed-{seed}{"" if reference_power==18 else "-p"+str(reference_power)}.json'
    if aggregate:
        saved=json.loads(seed_path.read_text())
        assert saved['seed']==seed and saved['powers']==powers and saved['conditionedSegment']==conditioned
        assert np.array_equal(tetra,np.array(saved['tetraVerticesM']))
        matrices[seed_index]=saved['matrixByPower']
        continue
    sampler=qmc.Sobol(5 if conditioned else 6,scramble=True,seed=seed)
    previous=None;previous_hash=None
    if reference_power>18:
        if selected_seed is None or not conditioned:raise ValueError('extension requires one conditioned seed')
        previous_path=root/f'debugging/p7-board-integration/tetra-space-conditioned-seed-{seed}{"" if reference_power==19 else "-p"+str(reference_power-1)}.json'
        previous=json.loads(previous_path.read_text());previous_hash=hashlib.sha256(previous_path.read_bytes()).hexdigest()
        assert previous['powers']==powers[:-1] and previous['seed']==seed and previous['conditionedSegment']==conditioned
        assert np.array_equal(tetra,np.array(previous['tetraVerticesM']))
        matrices[seed_index,:-1]=previous['matrixByPower']
        sampler.fast_forward(2**(reference_power-1))
        samples=sampler.random_base2(reference_power-1)
    else:samples=sampler.random_base2(powers[-1])
    def barycentric(u):
        # Order statistics of3 independent uniforms give the uniform simplex
        # measure. Every sample stays inside the actual finite tetrahedron.
        s=np.sort(u,axis=1)
        return np.column_stack((s[:,0],s[:,1]-s[:,0],s[:,2]-s[:,1],1-s[:,2]))
    a=barycentric(samples[:,:3])
    if conditioned:
        radial=np.cbrt(samples[:,3]);fraction=radial*np.sqrt(samples[:,4])
        c=1-radial;d=radial-fraction
        b=None
    else:b=barycentric(samples[:,3:])
    del samples
    for i in range(n):
        pa=a@tetra[i]
        for j in range(i,n):
            check()
            if conditioned:
                first,last=max(((r,s) for r in range(4) for s in range(r+1,4)),key=lambda pair:np.linalg.norm(tetra[j,pair[0]]-tetra[j,pair[1]]))
                others=[k for k in range(4) if k not in [first,last]]
                origin=tetra[j,first];edge=tetra[j,last]-origin
                length=np.linalg.norm(edge);axis=edge/length
                p=origin-pa+c[:,None]*(tetra[j,others[0]]-origin)+d[:,None]*(tetra[j,others[1]]-origin)
                axial=p@axis;rho=np.linalg.norm(np.cross(p,axis),axis=1)
                segment=fraction*length
                values=(np.arcsinh((axial+segment)/rho)-np.arcsinh(axial/rho))/segment
            else:
                distance=np.linalg.norm(pa-b@tetra[j],axis=1)
                if np.any(distance<=0):raise RuntimeError('direct reference sampled singular point')
                values=1/distance
            for level,power in enumerate(powers):
                if previous is not None and level<len(powers)-1:continue
                value=1e-7*volume[i]*volume[j]*values[:2**power].mean()
                if previous is not None:value=.5*(matrices[seed_index,level-1,i,j]+value)
                matrices[seed_index,level,i,j]=matrices[seed_index,level,j,i]=value
        del pa
    del a,b
    if selected_seed is not None:
        check()
        saved=dict(seed=seed,powers=powers,conditionedSegment=conditioned,tetraVerticesM=tetra.tolist(),matrixByPower=matrices[seed_index].tolist(),previousReferenceSha256=previous_hash,newSamplesPerPair=len(values),wallSeconds=time.perf_counter()-start,observedPeakBytes=peak,scriptSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
        if seed_path.exists():raise FileExistsError('immutable seed reference already exists')
        seed_path.write_text(json.dumps(saved,indent=2))
        print(json.dumps({k:saved[k] for k in ['seed','wallSeconds','observedPeakBytes']}))
        sys.exit(0)
mean=matrices.mean(axis=0)
chol=np.linalg.cholesky(mean[-1]);inverse=np.linalg.inv(chol)
whiten=lambda m: inverse@m@inverse.T
tail=2*np.linalg.norm(whiten(mean[-1]-mean[-2]))
seed_deviation=np.array([whiten(m-mean[-1]) for m in matrices[:,-1]])
standard_error=np.sqrt(np.sum(seed_deviation**2)/(len(seeds)*(len(seeds)-1)))
screen=tail+6*standard_error
check()
out=dict(tetraVerticesM=tetra.tolist(),matrix=mean[-1].tolist(),conditionedSegment=conditioned,
    matrixByPower=mean.tolist(),powers=powers,seeds=seeds,
    empiricalWhitenedTail=tail,empiricalWhitenedStandardError=standard_error,
    empiricalWhitenedReferenceScreen=screen,admittedReference=bool(screen<.01),
    wallSeconds=time.perf_counter()-start,observedPeakBytes=peak,
    scriptSha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
dest=root/f'debugging/p7-board-integration/tetra-space-reference-v{int(conditioned)}{"" if reference_power==18 else "-p"+str(reference_power)}.json'
if dest.exists():raise FileExistsError('immutable reference already exists')
dest.write_text(json.dumps(out,indent=2))
print(json.dumps({k:out[k] for k in ['empiricalWhitenedReferenceScreen','admittedReference','wallSeconds','observedPeakBytes']}))
packets=[arg for arg in sys.argv[1:] if not arg.startswith('--')]
if packets:
    packet=root/'debugging/p7-board-integration'/packets[0]
    line=next(x for x in (packet/'error.txt').read_text().splitlines() if x.startswith('complete-tetra-matrix '))
    candidate=json.loads(line.removeprefix('complete-tetra-matrix '))
    assert np.array_equal(tetra,np.array(candidate['tetraVerticesM']))
    error=np.linalg.norm(whiten(np.array(candidate['matrix'])-mean[-1]))
    print(json.dumps(dict(completeSpaceError=error,totalScreen=error+screen,passed=bool(error+screen<.01))))
    if error+screen>=.01:sys.exit(2)
elif not out['admittedReference']:sys.exit(2)
