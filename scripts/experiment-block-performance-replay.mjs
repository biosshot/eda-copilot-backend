import {readFileSync,writeFileSync,readdirSync} from 'node:fs';
import {gunzipSync} from 'node:zlib';
import assert from 'node:assert/strict';
import {loadNativeBoardPacker} from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';

const [tag='optimized',dir='docs/experimental/pcb/placement-performance-2026-09-28/esp32c3/before']=process.argv.slice(2);
const files=readdirSync(dir).filter(f=>/^block-\d+\.json.gz$/.test(f)).sort((a,b)=>a.localeCompare(b,undefined,{numeric:true}));
const fixtures=files.map(file=>({file,...JSON.parse(gunzipSync(readFileSync(`${dir}/${file}`)))}));
const addon=loadNativeBoardPacker(),calls=[];
const start=performance.now();
for(const f of fixtures){
    const t=performance.now();
    const solution=addon.solveBlockPrimitives(f.problem);
    calls.push({file:f.file,ms:performance.now()-t});
    assert.deepEqual(solution,f.solution,`${f.file}: differs from original native result`);
}
const result={tag,ms:performance.now()-start,blocks:fixtures.length,exactResults:true,calls};
writeFileSync(`${dir}/benchmark-${tag}.json`,JSON.stringify(result,null,2));
console.log(JSON.stringify(result));
