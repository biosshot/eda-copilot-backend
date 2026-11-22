"""Hash the completed review packet; preserve earlier runtime evidence."""
import hashlib, json, pathlib
root = pathlib.Path(__file__).resolve().parents[3]
out = root/'debugging/p6-signal-kernel'
def digest(p): return hashlib.sha256(p.read_bytes()).hexdigest()
registry = pathlib.Path.home()/'.cargo/registry'
crate = next((registry/'cache').glob('*/rustfft-6.4.1.crate'))
source = next((registry/'src').glob('*/rustfft-6.4.1'))
audit = ['Cargo.toml','LICENSE-MIT','LICENSE-APACHE','src/algorithm/radix4.rs','src/algorithm/bluesteins_algorithm.rs']
dependency = dict(name='rustfft',version='6.4.1',license='MIT OR Apache-2.0',defaultFeatures=False,dispatch='direct scalar Radix4/Bluestein; no planner cache',externalRuntime=False,archiveBytes=crate.stat().st_size,archiveSha256=digest(crate),auditedSources={name:digest(source/name) for name in audit})
(out/'dependency.json').write_text(json.dumps(dependency,indent=2)+'\n')
old = json.loads((root/'debugging/p6-resonant-runtime/manifest.json').read_text())
extended = ['CHANGELOG.md','native/analyze/src/electrical/mod.rs','native/analyze/Cargo.toml','native/analyze/Cargo.lock','native/node-addon/Cargo.lock']
changed = [x['path'] for x in old['files'] if x['path'] not in extended and (not (root/x['path']).exists() or digest(root/x['path']) != x['sha256'])]
assert not changed, changed
(out/'prior-evidence.json').write_text(json.dumps(dict(status='pass',previousManifestFiles=len(old['files']),sharedExtensions=extended,changedOutsideExtensions=changed),indent=2)+'\n')
names = extended + ['native/analyze/src/electrical/signal.rs','native/analyze/src/electrical/signal_tests.rs','docs/experimental/analyze/next-task-electrical-spectra-kernel.md','docs/experimental/analyze/electrical-signal-kernel-verification.md','tests/fixtures/analysis-resonant-runtime/bounded.py','debugging/p6-target/debug/deps/pcb_analyze-3f2cee2378ff63b4.exe']
paths = [root/n for n in names]
paths += [p for p in pathlib.Path(__file__).parent.iterdir() if p.is_file()]
paths += [p for p in out.iterdir() if p.is_file() and p.name != 'manifest.json']
files = [dict(path=p.relative_to(root).as_posix(),bytes=p.stat().st_size,sha256=digest(p)) for p in sorted(set(paths))]
(out/'manifest.json').write_text(json.dumps(dict(status='pass',files=files),indent=2)+'\n')
assert all(digest(root/x['path']) == x['sha256'] for x in files)
print('Verified',len(files),'hashes; accepted prior runtime evidence unchanged outside shared extensions')
