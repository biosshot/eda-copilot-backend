import {readFileSync,writeFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {resolve} from 'node:path';
import assert from 'node:assert/strict';
import {variants} from './experiment-block-replay.mjs';
const require=createRequire(import.meta.url);
const addon=require(resolve(process.env.PCB_BOARD_PACKER_NATIVE_PATH??`native/pcb-board-packer/${require('../native/pcb-board-packer/platform.cjs').nativeFilename()}`));
const {problem}=JSON.parse(readFileSync('tests/fixtures/block-placement/Telemetry/block-23.json'));
const rows=[];
for(const variant of ['B0','N','L','ALLX','NCLR']) {
 const {width,...experiments}=variants[variant]; const input={...problem,experiments,searchWidth:width??1};
 const expected=addon.solveBlockPrimitives(input); const times=[];
 for(let i=0;i<3;i++) { const start=performance.now(); const result=addon.solveBlockPrimitives(input); times.push(performance.now()-start); assert.deepEqual(result,expected); }
 rows.push({variant,times,median:times.toSorted((a,b)=>a-b)[1],deterministic:true});
}
writeFileSync('.test-output/usb-timing.json',JSON.stringify(rows,null,2));console.log(JSON.stringify(rows));
