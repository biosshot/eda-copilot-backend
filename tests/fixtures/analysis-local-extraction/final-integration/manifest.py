"""Bind the final checked source/binary packet; do not rewrite earlier runs."""
from pathlib import Path
import hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-final-integration'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
run=json.loads((OUT/'checked/run.json').read_text())
for group in ['sources','boardsBeforeAfter','historyBeforeAfter']:
 for name,expected in run[group].items():assert sha(ROOT/name)==expected,(group,name)
manifest={'base':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
 'sourceFiles':run['sources'],'capturedBoards':run['boardsBeforeAfter'],'preservedHistory':run['historyBeforeAfter'],
 'binaries':{},'compiledJavaScript':{},'documents':{},'artifacts':{}}
for name in ['native/analyze/target/release/examples/automatic_reduction.exe','native/node-addon/eda-copilot-node.win32-x64-msvc.node']:
 manifest['binaries'][name]=sha(ROOT/name)
assert manifest['binaries']['native/analyze/target/release/examples/automatic_reduction.exe']==run['binarySha256']
for p in (ROOT/'dist').rglob('*.js'):manifest['compiledJavaScript'][str(p.relative_to(ROOT))]=sha(p)
for name in ['CHANGELOG.md','docs/experimental/analyze/implementation-plan.md','docs/experimental/analyze/next-task-p7-final-integration.md','docs/experimental/analyze/p7-final-integration-verification.md']:
 manifest['documents'][name]=sha(ROOT/name)
for p in OUT.rglob('*'):
 if p.is_file() and p.name!='evidence-manifest.json':manifest['artifacts'][str(p.relative_to(ROOT))]=sha(p)
(OUT/'evidence-manifest.json').write_text(json.dumps(manifest,indent=2))
print(json.dumps({k:len(v) for k,v in manifest.items() if isinstance(v,dict)}))
