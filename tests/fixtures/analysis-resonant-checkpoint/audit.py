"""Read-only numerical evidence audit; no fit or transient reruns."""
import hashlib,json,platform,sys
import checkpoint as c
import numpy as np
c.verify_frozen()
summary={'scope':'fine-step authored one-section class only; coarse failures preserved',
    'python':sys.version,'numpy':np.__version__,'platform':platform.platform(),'cases':[]}
for case in c.DEF['cases']:
    name=case['name'];r=json.loads((c.OUT/f'{name}-result.json').read_text())
    a=np.load(c.OUT/f'{name}-ac.npz');t=np.load(c.OUT/f'{name}-traces.npz');g=c.DEF['gates']
    entries=[]
    for i in range(len(case['modes'])):
        for j in range(len(case['modes'])):
            e=c.error(a['fit'][:,i,j],a['reference'][:,i,j],g['matrixRelative'])
            entries.append({'row':i+1,'column':j+1,**e})
    fine=r['time'][-1]; vf=r['grids'][-1]['orders'][0]['vectorFitting']
    assert fine['stepSeconds']==1e-9 and fine['pass']
    assert np.max(abs(t['output']-t['reference']))<=g['transientFractionOfScale']*case['outputScaleSI']+g['transientAbsoluteSI']
    assert np.max(abs(t['energy']-t['energyReference']))<=g['energyReferenceFractionOfScale']*case['energyScaleJ']+g['energyAbsoluteJ']
    assert np.all(abs(t['work']-t['loss']-t['energy'])<=g['energyBalanceRelative']*(abs(t['work'])+t['loss']+t['energy'])+g['energyAbsoluteJ'])
    assert np.min(t['energy'])>=0 and np.min(t['loss'])>=0 and np.min(np.diff(t['loss']))>=-g['energyAbsoluteJ']
    assert vf['accepted'] and vf['holdout']['pass'] and r['vectorTimeFraction']<=.01
    for grid in r['grids']:
        for order in grid['orders']:
            assert order['training']['pass'] and order['holdout']['pass']
            assert order.get('refinement',{'pass':True})['pass']
    assert r['resources']['wallSeconds']<60 and r['resources']['cpuSeconds']<60
    assert max(r['resources']['peakCommitBytes'],r['resources']['peakWorkingSetBytes'])<512*1024**2
    assert all(e['pass'] for e in entries)
    summary['cases'].append({'case':name,'strictAllStepsPass':r['pass'],'fineGatePass':True,
        'allDenseMatrixEntries':entries,'fineTime':fine,'vectorTimeFraction':r['vectorTimeFraction'],
        'timeRefinementFraction':r['timeRefinementFraction'],
        'gridStudy':[{'count':x['count'],'samplesPerLinewidth':x['samplesPerLinewidth'],'resolutionPass':x['resolutionPass'],
            'structuralHoldoutMax':x['orders'][0]['holdout']['normalizedMax'],
            'vectorHoldoutMax':x['orders'][0]['vectorFitting'].get('holdout',{}).get('normalizedMax'),
            'orderRefinementMax':max(y.get('refinement',{'normalizedMax':0})['normalizedMax'] for y in x['orders'])} for x in r['grids']],
        'resources':r['resources']})
negative=json.loads((c.OUT/'negative-result.json').read_text());assert negative['pass']
summary['negative']=negative;summary['fineCheckpointPass']=True
summary['strictAllStepsPass']=all(x['strictAllStepsPass'] for x in summary['cases'])
c.save(c.OUT/'metrics.json',summary)
paths=[*c.ROOT.glob('*.py'),*c.ROOT.glob('*.json'),*c.OUT.glob('*.json'),*c.OUT.glob('*.npz'),*c.OUT.glob('*.png')]
paths += [c.OUT/'journal.md',c.ROOT.parents[2]/'docs/experimental/analyze/electrical-resonant-checkpoint.md',
    c.ROOT.parents[2]/'docs/experimental/analyze/next-task-resonant-model-checkpoint.md']
paths=[p for p in paths if p.name!='evidence-manifest.json']
c.save(c.OUT/'evidence-manifest.json',{'files':[{ 'path':str(p.relative_to(c.ROOT.parents[2])).replace('\\','/'),
    'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in sorted(paths)],
    'frozenManifestSha256':hashlib.sha256((c.ROOT/'frozen-manifest.json').read_bytes()).hexdigest(),
    'fineCheckpointPass':True,'strictAllStepsPass':summary['strictAllStepsPass']})
print(json.dumps(summary,indent=2))
