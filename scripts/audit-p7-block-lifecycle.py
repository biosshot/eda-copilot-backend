"""Summarize emitted block/factor instrumentation; no extraction or fitting."""
import json,re,sys
from pathlib import Path
folder=Path(sys.argv[1]);active=None;blocks=[];max_active=0
for line in (folder/'stderr.txt').read_text(encoding='utf-8',errors='replace').splitlines():
 try:event=json.loads(line)
 except json.JSONDecodeError:event={}
 if event.get('event')=='P7_BLOCK_BEGIN':
  if active is not None:raise RuntimeError('Overlapping block scopes in trace')
  active={'id':event['id'],'kind':event['kind'],'before':event['memory'],'factors':[]};max_active=max(max_active,1)
 elif event.get('event')=='P7_BLOCK_END':
  if active is None or active['id']!=event['id']:raise RuntimeError('Unmatched block scope')
  active['lifecycle']=event['lifecycle'];blocks.append(active);active=None
 match=re.search(r'P7_SYMBOLIC n=(\d+) sharedPattern=(\d+) panelValues=(\d+) symbolicOwnedBound=(\d+)',line)
 if match and active is not None:active['factors'].append(dict(zip(('unknowns','sharedPatternEntries','factorPanelValues','symbolicOwnedBoundBytes'),map(int,match.groups()))))
 match=re.search(r'P7_FACTOR n=(\d+) orderingAndPrefill=([\d.]+)s symbolic=([\d.]+)s numeric=([\d.]+)s',line)
 if match and active is not None:
  n,ordering,symbolic,numeric=match.groups()
  if active['factors']:active['factors'][-1].update(orderingAndPrefillSeconds=float(ordering),symbolicSeconds=float(symbolic),numericSeconds=float(numeric))
report={'scope':'Sequential Node block scopes, native sparse factor dimensions/fill, OS snapshots. Factor ownership/release uses native preparation RAII; RSS is not live allocation bytes.','maximumActiveBlocks':max_active,'incompleteScope':active,'blocks':blocks}
(folder/'block-lifecycle.json').write_text(json.dumps(report,indent=2));print(json.dumps({'completedScopes':len(blocks),'maximumActiveBlocks':max_active,'incompleteScope':active is not None}))
