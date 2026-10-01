// Separate Node address spaces compete for the GPU lease; native workers share it.
import {spawn,spawnSync} from 'node:child_process';
import {mkdirSync,readFileSync,writeFileSync,openSync,closeSync} from 'node:fs';
import {resolve,join} from 'node:path';
import assert from 'node:assert/strict';
const base=resolve(process.argv[2]??'debugging/cubecl-block-migration-2026-09-30/processes');mkdirSync(base,{recursive:true});
const reference=resolve(process.argv[3]??'debugging/cubecl-block-migration-2026-09-30/recovery-mid/reference/results.json');
const env={...process.env};for(const k of Object.keys(env))if(k.startsWith('PCB_BLOCK_GPU_')||k.startsWith('CUBECL_DEBUG_')||k==='VK_DRIVER_FILES')delete env[k];
const usedMemory=()=>{const r=spawnSync('nvidia-smi',['--query-gpu=memory.used','--format=csv,noheader,nounits'],{encoding:'utf8',windowsHide:true});return r.status===0?Number(r.stdout.trim().split(/\r?\n/)[0]):null;};
const rows=[];
for(const processes of [1,2,4]) {
  const before=usedMemory();let peak=before;
  const samples=[];const sample=()=>{const used=usedMemory();if(used!==null){samples.push({elapsedMs:performance.now()-started,deviceUsedMiB:used});peak=Math.max(peak??0,used);}};
  const started=performance.now();const timer=setInterval(sample,500);
  const children=Array.from({length:processes},(_,i)=>new Promise((resolveChild,reject)=>{
    const out=join(base,`${processes}-processes`,String(i));mkdirSync(out,{recursive:true});
    const fd=openSync(join(out,'launcher.log'),'w');
    const child=spawn(process.execPath,['scripts/experiment-block-cubecl.mjs','backend=cubecl','blocks=00079,00097','runs=1','workers=1',`reference=${reference}`,`out=${out}`],{env,stdio:['ignore',fd,fd],windowsHide:true});
    child.on('error',reject);child.on('close',code=>{closeSync(fd);if(code!==0){reject(Error(`child ${i} exited ${code}`));return;}
      const r=JSON.parse(readFileSync(join(out,'results.json')));assert.equal(r.rows[0].exactReferenceMatch,true);
      assert(r.runtime.length<=1&&r.runtime.every(x=>x.initializations===1));
      resolveChild({process:i,actualBackend:r.actualBackend,fullCycleMs:r.rows[0].totalMs,initializations:r.runtime.length,
        fallbacks:r.fallbacks.map(f=>f.reason),exactCpuResult:true,addonSha256:r.addonSha256});
    });
  }));
  let results;try{results=await Promise.all(children);}finally{clearInterval(timer);}
  const wallMs=performance.now()-started;sample();
  const row={processes,fullBlockCycles:processes*2,wallMs,cyclesPerSecond:processes*2/(wallMs/1000),deviceMemoryBeforeMiB:before,
    deviceMemoryPeakMiB:peak,deviceMemoryAfterMiB:usedMemory(),memoryNote:'Whole-device usage includes other applications; samples every 500 ms.',samples,results};
  rows.push(row);writeFileSync(join(base,'summary.json'),JSON.stringify(rows,null,2));
  console.log(JSON.stringify({processes,wallMs,peakMiB:peak,backends:results.map(r=>r.actualBackend),exactCpuResults:true}));
}
