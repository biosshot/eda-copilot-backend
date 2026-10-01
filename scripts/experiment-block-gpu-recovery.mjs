// Exercise release CPU restart, including a panic inside the shared GPU runtime.
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {resolve,join} from 'node:path';
import {spawnSync} from 'node:child_process';
import assert from 'node:assert/strict';
const args=Object.fromEntries(process.argv.slice(2).map(s=>s.split('=')));
const base=resolve(args.out??'debugging/cubecl-block-migration-2026-09-30/recovery');mkdirSync(base,{recursive:true});
const modes=(args.modes??'disabled,hidden,batch,beam,singles,pairs').split(',');
const environment={...process.env};
for(const k of Object.keys(environment))if(k.startsWith('PCB_BLOCK_GPU_')||k.startsWith('CUBECL_DEBUG_')||k==='VK_DRIVER_FILES')delete environment[k];
const run=(name,backend,env={},reference=null,pairsOnly=false)=>{
  const out=join(base,name);const command=['scripts/experiment-block-cubecl.mjs',`backend=${backend}`,'blocks=00079,00097','runs=1',`out=${out}`];
  if(reference)command.push(`reference=${reference}`);
  if(pairsOnly)command.push('pairsOnly=1');
  const child=spawnSync(process.execPath,command,{env:{...environment,...env},encoding:'utf8',windowsHide:true,maxBuffer:8*1024*1024});
  assert.equal(child.status,0,`${name}: ${child.error??child.stderr}`);
  return JSON.parse(readFileSync(join(out,'results.json')));
};
run('reference','cpu');const reference=join(base,'reference','results.json');const rows=[];
for(const mode of modes){
  const overrides=mode==='disabled'?{PCB_BLOCK_GPU_DISABLED:'1'}:mode==='hidden'?{VK_DRIVER_FILES:join(base,'intentionally-missing-driver.json')}:['batch','pairs'].includes(mode)?{PCB_BLOCK_GPU_FAIL_BATCH:'8'}:{PCB_BLOCK_GPU_FAIL_AT:mode};
  const report=run(mode,'cubecl',overrides,reference,mode==='pairs');
  assert.equal(report.actualBackend,'cpu');assert.equal(report.rows[0].exactReferenceMatch,true);
  assert.equal(report.fallbacks.length,mode==='pairs'?2:4);
  const reasons=report.fallbacks.map(f=>f.reason);
  assert(reasons.some(r=>mode==='disabled'?r.includes('disabled'):mode==='hidden'?/Vulkan|GPU|adapter/i.test(r):r.includes('injected GPU')));
  assert(!reasons.some(r=>r.includes('owned by another process')),`${mode} was masked by a competing GPU process`);
  rows.push({mode,exactCpuResult:true,processAlive:true,actualBackend:report.actualBackend,reasons,addonSha256:report.addonSha256,sourceHashes:report.sourceHashes});
  writeFileSync(join(base,'summary.json'),JSON.stringify(rows,null,2));console.log(JSON.stringify(rows.at(-1)));
}
