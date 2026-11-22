"""Compile actual unchanged arithmetic modules with cached crates and -O.

No new solver/dependency, no implicit replay; preserve each measured binary.
"""
from pathlib import Path
import json,hashlib,subprocess,time,re,sys
ROOT=Path(__file__).resolve().parents[4];OUT=ROOT/'debugging/p7-design-checkpoint/copper-context'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
assert sys.argv[1]=='kernel'
assert not (OUT/'kernel-build.json').exists()
s=(ROOT/'native/analyze/examples/copper_context.rs').read_text();s=s[s.index('use serde_json'):]
start=re.search(r'if mode\s*==\s*"geometry"\s*\{',s).start();brace=s.index('{',start);depth=1;end=brace+1
while depth:
    depth+=(s[end]=='{')-(s[end]=='}');end+=1
s=s[:start]+s[end:]
extra=r'''
    if mode=="self" {
        let mut cells=vec![];let mut seen=std::collections::BTreeSet::new();
        for witness in v.as_array().unwrap() {
            let index=witness["postfixIndex"].as_u64().unwrap();if !seen.insert(index){continue;}
            let p=prism(&witness["cell"],true);let source=polyhedron::PointPotential::new(&p).unwrap();let mut values=vec![];
            for (xy,z) in [(12,8),(24,16)] {
                let result=polyhedron::volume_integral_anisotropic(&p,&source,&prism::GaussRule::new(xy).unwrap(),&prism::GaussRule::new(z).unwrap());
                values.push(json!({"xy":xy,"z":z,"resultM5":result}));
            }
            cells.push(json!({"postfixIndex":index,"triangleM":p.triangle_m,"zM":p.z_m,"values":values}));
        }
        println!("{}",json!({"cells":cells}));return;
    }
'''
# The mode uses source text, so do not retain Python raw-string quote escapes.
extra=extra.replace('\\"','"')
pos=re.search(r'if mode\s*==\s*"plans"',s).start();s=s[:pos]+extra+s[pos:]
prefix='#![allow(dead_code)]\n';sources={}
for n in ['far','near_far','polyhedron','prism']:
    p=ROOT/f'native/analyze/src/extraction/local/{n}.rs';prefix+=f'#[path="{p.as_posix()}"] mod {n};\n';sources[str(p.relative_to(ROOT))]=sha(p)
prefix+='type Result<T>=std::result::Result<T,String>;fn error(c:&str,s:&str)->String{format!("LOCAL_{c}: {s}")}\nuse near_far::{Budget,Plan,Operator,ExactCache};use prism::Prism;\n'
probe=OUT/'kernel-probe.rs';probe.write_text(prefix+s);deps=ROOT/'native/analyze/target/debug/deps';cmd=['rustc','--edition=2021','-O','-C','codegen-units=1','-L','dependency='+str(deps)]
libs={}
for lib in ['serde_json']:
    ps=list(deps.glob('lib'+lib+'-*.rlib'));assert len(ps)==1;cmd+=['--extern',lib+'='+str(ps[0])];libs[lib]=sha(ps[0])
cmd+=[str(probe),'-o',str(OUT/'kernel-probe.exe')];start=time.perf_counter();p=subprocess.run(cmd,capture_output=True,timeout=120)
(OUT/'kernel-build.log').write_bytes(p.stdout+p.stderr);assert p.returncode==0,p.stderr.decode()
(OUT/'kernel-build.json').write_text(json.dumps({'command':cmd,'sources':sources,'libraries':libs,'seconds':time.perf_counter()-start,'probeSha256':sha(probe),'binarySha256':sha(OUT/'kernel-probe.exe')},indent=2))
print('optimized actual-source kernel compiled',time.perf_counter()-start)
