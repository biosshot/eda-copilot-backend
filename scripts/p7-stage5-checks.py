"""Internal checks of completed stage 5 models: KCL/KVL residuals, residual policy tier, power balance,
return-current audit, and S-matrix passivity (max singular value) and reciprocity from the saved dsl-result.json.
Usage: python scripts/p7-stage5-checks.py debugging/p7-v1-stage5/final/cold [more folders]"""
import json, sys
from pathlib import Path
import numpy as np


def solution(path):
    d = json.loads(path.read_text(encoding='utf-8'))
    calc = [c for r in d.get('reports', []) for c in r['calculations'] if c.get('phase') == 'completed']
    return next((c['result']['solution'] for c in reversed(calc) if c.get('result', {}).get('solution')), None)


for root in map(Path, sys.argv[1:]):
    for f in sorted(root.glob('*/dsl-result.json')):
        s = solution(f)
        if s and 'quality' not in s and isinstance(s.get('evidence'), dict): s = {**s['evidence'], 'network': s.get('network')}
        if not s or 'quality' not in s:
            print(f.parent.name, 'no model'); continue
        q = s['quality']; rs = s.get('returnSearch', {})
        frequencies = s.get('frequenciesHz') or s.get('evidence', {}).get('frequenciesHz')
        net = s.get('network') or s.get('evidence', {}).get('network')
        params = s.get('parameters', {}).get('sMatrix') if isinstance(s.get('parameters'), dict) else None
        passive = reciprocal = None
        table = (net or {}).get('data', {}).get('parameters') if isinstance(net, dict) else None
        if table is None and isinstance(s.get('parameters'), list): table = s['parameters']
        if table:
            worst, asym = 0., 0.
            for m in table:
                a = np.array([[complex(x['real'], x['imaginary']) for x in row] for row in m])
                worst = max(worst, np.linalg.svd(a, compute_uv=False)[0])
                asym = max(asym, abs(a - a.T).max())
            passive, reciprocal = worst, asym
        print(json.dumps(dict(run=f.parent.name, status=s.get('status'), frequenciesHz=frequencies, ports=len(s.get('portNames', [])),
              maxKclRelative=q.get('maxKclRelative'), maxKvlRelative=q.get('maxKvlRelative'), powerBalanceW=q.get('maxColumnPowerBalanceErrorW'),
              residualTier=q.get('residualPolicy', {}).get('tier'), residualLimit=q.get('residualPolicy', {}).get('limit'),
              sliverTier=q.get('sliverPolicy', {}).get('tier'), droppedVolumeRelative=q.get('sliverPolicy', {}).get('droppedVolumeRelative'),
              returnSearch=rs.get('status'), returnMaxRelative=max([x.get('maximumRelative', 0) for x in rs.get('steps', [])] or [0]),
              sMaxSingularValue=passive, sReciprocityAbs=reciprocal, physicalError=q.get('physicalError'))))
