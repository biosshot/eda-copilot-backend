// Keep the failed solver process alive while a second process acquires its lease.
import {spawn,spawnSync} from 'node:child_process';
import {mkdirSync,openSync,closeSync,readFileSync,writeFileSync} from 'node:fs';
import {resolve,join} from 'node:path';
import assert from 'node:assert/strict';
const out=resolve(process.argv[2]??'debugging/cubecl-block-migration-2026-09-30/lease-after-failure');mkdirSync(out,{recursive:true});
const reference=resolve(process.argv[3]??'debugging/cubecl-block-migration-2026-09-30/recovery-mid/reference/results.json');
const original=JSON.parse(readFileSync(reference));
const env={...process.env};for(const k of Object.keys(env))if(k.startsWith('PCB_BLOCK_GPU_')||k.startsWith('CUBECL_DEBUG_')||k==='VK_DRIVER_FILES')delete env[k];
const fd=openSync(join(out,'failed-owner.log'),'w');
const code=`const fs=require('fs');process.env.PCB_BLOCK_BACKEND='cubecl';process.env.PCB_BLOCK_GPU_FAIL_BATCH='8';process.env.PCB_BLOCK_SOLVER_PROFILE='1';
const addon=require(${JSON.stringify(resolve(process.env.PCB_BOARD_PACKER_NATIVE_PATH??'native/pcb-board-packer/pcb-board-packer.win32-x64-msvc.node'))});
const solution=addon.solveBlockPrimitives(JSON.parse(fs.readFileSync(${JSON.stringify(original.blocks[0].path)})));process.send({solution});process.on('message',()=>process.exit(0));`;
const owner=spawn(process.execPath,['-e',code],{env,stdio:['ignore',fd,fd,'ipc'],windowsHide:true});
try {
 const message=await new Promise((ok,fail)=>{owner.once('message',ok);owner.once('error',fail);owner.once('exit',c=>fail(Error(`owner exited before message: ${c}`)));});
 assert.deepEqual(message.solution,original.rows[0].results[0].initial);assert.equal(owner.exitCode,null);
 const contender=join(out,'new-owner');const r=spawnSync(process.execPath,['scripts/experiment-block-cubecl.mjs','backend=cubecl','blocks=00079,00097','runs=1',`reference=${reference}`,`out=${contender}`],{env,encoding:'utf8',windowsHide:true});
 assert.equal(r.status,0,r.stderr);const report=JSON.parse(readFileSync(join(contender,'results.json')));
 assert.equal(report.actualBackend,'cubecl');assert.equal(report.rows[0].exactReferenceMatch,true);assert.equal(report.runtime.length,1);assert.equal(owner.exitCode,null);
 writeFileSync(join(out,'summary.json'),JSON.stringify({failedProcessAlive:true,failedProcessExactCpuResult:true,newProcessBackend:report.actualBackend,newProcessExactCpuResult:true,leaseReleasedAfterRuntimeFailure:true,addonSha256:report.addonSha256},null,2));
 console.log('Runtime failure releases the lease while the original process remains alive; both results match CPU.');
} finally {owner.kill();closeSync(fd);}
