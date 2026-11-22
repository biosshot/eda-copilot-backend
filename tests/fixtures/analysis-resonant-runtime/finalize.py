"""Saved-array parity, frozen-input and resource audit; no numerical solves."""
import hashlib,json,pathlib,re,sys
root=pathlib.Path.cwd();out=root/'debugging/p6-resonant-runtime'
digest=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
frozen=root/'tests/fixtures/analysis-resonant-checkpoint/frozen-manifest.json'
assert digest(frozen)=='342a2af63f6dc2140c987e6a56d6955178b41cfb30a824aa88cd2a3ae58796a2'
for name,sha in json.loads(frozen.read_text()).items():assert digest(frozen.parent/name)==sha
def read(name):return json.loads((out/name).read_text())
def numeric(result):
    done=[c['result']['solution'] for report in result['reports'] for c in report['calculations'] if c['phase']=='completed']
    return [{k:s[k] for k in ['axis','portVoltages','driverCurrentsA','energyJ','quality'] if k in s} if index<2 else {k:s[k] for k in ['frequenciesHz','parameters']} for index,s in enumerate(done)]
packages={};baseline=None;native=None;scenario=None
for mode in ['direct','compiled','installed']:
    result=read(mode+'/result.json');audit=read(mode+'/audit.json');resources=read(mode+'-supervisor.json')
    assert audit['status']=='pass' and resources['exitCode']==0
    assert resources['wallSeconds']<=60 and resources['peakJobCommitBytes']<=512*1048576 and resources['peakProcessCommitBytes']<=512*1048576
    actual=numeric(result['result'])
    if baseline is None:baseline=actual;native=result['metadata']['nativeSha256'];scenario=result['metadata']['scenarioSha256']
    else:assert actual==baseline
    assert result['metadata']['nativeSha256']==native and result['metadata']['scenarioSha256']==scenario
    packages[mode]={'audit':audit,'resources':resources,'numericSha256':hashlib.sha256(json.dumps(actual,separators=(',',':')).encode()).hexdigest()}
addons=list((root/'native/node-addon').glob('*.node'));assert len(addons)==1 and digest(addons[0])==native
old=read('direct-before-harness-fix/result.json');assert numeric(old['result'])==baseline
assert old['metadata']['nativeSha256']==native
counts=lambda name:[int(x) for x in re.findall(r'test result: ok\. (\d+) passed;', (out/name).read_text()) if int(x)]
assert counts('native-focused-final.log')==[5]
assert counts('native-regression.log')==[9,13,8,6]
build=read('native-build-meta.json');assert build['exitCode']==0 and build['jobs']==1
js=read('js-check-meta.json');assert js['buildExitCode']==0 and js['typecheckExitCode']==0
ts=read('ts-supervisor.json');assert ts['exitCode']==0 and ts['wallSeconds']<=60 and ts['peakJobCommitBytes']<=512*1048576
test_log=(out/'ts-final.log').read_text(encoding='utf-8');assert 'pass 8' in test_log and 'fail 0' in test_log and 'skipped 0' in test_log
summary={'status':'pass','nativeSha256':native,'scenarioSha256':scenario,'unchangedFrozenManifestSha256':digest(frozen),
    'focusedRustPassed':5,'regressionRustPassed':36,'typescriptPassed':8,'typescriptSkipped':0,'directCompiledInstalledExactNumericParity':True,
    'preHarnessFixExactNumericParity':True,'packages':packages,'nativeBuild':build,'jsChecks':js,'tsSupervisor':ts,
    'scope':'Conditional authored one-section/modal Y only. Existing energy guards; no PCB extraction or general sampled model identification.'}
(out/'metrics.json').write_text(json.dumps(summary,indent=2)+'\n')
paths=[root/p for p in [
    'CHANGELOG.md','docs/experimental/analyze/electrical-core-completion-semantics.md','docs/experimental/analyze/next-task-resonant-model-runtime.md',
    'docs/experimental/analyze/electrical-resonant-runtime-verification.md','native/analyze/src/electrical/resonant.rs',
    'native/analyze/src/electrical/resonant_tests.rs','native/analyze/src/electrical/mod.rs','native/analyze/src/electrical/coupled/composition.rs',
    'native/analyze/src/electrical/coupled/time.rs','src/analyze/analysis-dsl/frequency-model.ts','src/analyze/analysis-dsl/spec-doc.d.ts',
    'src/analyze/analysis-dsl/transient-executor.ts','src/analyze/analysis-dsl/prepare.ts','tests/electrical-resonant.test.ts','tests/electrical-resource-propagation.test.ts']]
paths+=list((root/'tests/helpers').glob('electrical-resonant-*'))
paths+=[p for p in (root/'tests/fixtures/analysis-resonant-runtime').glob('*') if p.is_file()]
paths+=list(frozen.parent.glob('*.json'))
paths+=[p for p in out.rglob('*') if p.is_file() and 'consumer' not in p.relative_to(out).parts and p.name not in ['manifest.json','finalize.log']]
paths+=[p for p in (root/'dist').rglob('*') if p.is_file()]+addons
paths=sorted(set(paths))
manifest={'status':'pass','nativeSha256':native,'files':[{'path':p.relative_to(root).as_posix(),'bytes':p.stat().st_size,'sha256':digest(p)} for p in paths]}
(out/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print(json.dumps({'status':'pass','files':len(paths),'nativeSha256':native,'nativeTests':41,'parity':True,'packages':{k:v['resources'] for k,v in packages.items()}},indent=2))
