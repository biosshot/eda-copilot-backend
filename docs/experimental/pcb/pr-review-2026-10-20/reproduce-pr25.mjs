// Run with Node + tsx from a checkout containing PR #25 and its rebuilt addon:
// node --import tsx PATH_TO_THIS_FILE
// Exit 1 is expected on reviewed head 453130f: it demonstrates lost parity.
import {readFileSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {isDeepStrictEqual} from 'node:util';

process.env.PCB_BLOCK_BACKEND='cpu';
process.env.PCB_BOARD_BACKEND='cpu';
process.env.PCB_POST_PLACE_BACKEND='cpu';
const checkout=process.cwd();
const {refineBoardAlignment}=await import(pathToFileURL(resolve(checkout,'src/pcb-layout/pcb-auto-place-v2/board-alignment.ts')).href);
const saved=JSON.parse(readFileSync(new URL('./alignment-counterexample.json',import.meta.url),'utf8'));
let failed=false;
for(const threads of [1,4]){
 const {input,roots,placements}=structuredClone(saved.case);
 const actual=JSON.parse(JSON.stringify(refineBoardAlignment(input,roots,placements,threads)));
 const equal=isDeepStrictEqual(actual,saved.old);
 console.log(JSON.stringify({threads,equal,expectedY:saved.old.placements.map(p=>p.y),actualY:actual.placements.map(p=>p.y),
  expectedMoves:saved.old.moves.length,actualMoves:actual.moves.length,expectedRejections:saved.old.rejected,actualRejections:actual.rejected}));
 failed ||= !equal;
}
if(failed){
 console.error('REGRESSION: native board alignment differs from the captured serial reference.');
 process.exitCode=1;
}
