import test from 'node:test';
import assert from 'node:assert/strict';
import {existsSync,readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
import {join} from 'node:path';

const root=fileURLToPath(new URL('./pcb-analyze/',import.meta.url));
const manifest=JSON.parse(readFileSync(join(root,'p7-matrix.json'),'utf8'));

test('P7 readiness matrix rows point at existing boards and scenarios with expected statuses',()=>{
 assert.equal(manifest.physicalAccuracy,'unverified');
 const ids=manifest.rows.map((r:any)=>r.id);
 assert.deepEqual(ids,['M1','M2','M3','M4','M5','M6','M7a','M7b','M8','M9','M10']);
 for(const row of manifest.rows){
  assert.ok(existsSync(join(root,row.board,`${row.board}.json`)),`${row.id}: board`);
  assert.ok(['complete','failed','refused'].includes(row.expected.status),`${row.id}: status`);
  if(row.expected.status!=='complete')assert.match(row.expected.code,/^[A-Z_]+$/,`${row.id}: refusal code`);
  if(row.scenario===null){assert.equal(row.expected.status,'refused');continue;}
  const code=readFileSync(join(root,row.scenario),'utf8');
  assert.match(code,/analysis\(\{board\}\)/,`${row.id}: scenario`);
 }
});
