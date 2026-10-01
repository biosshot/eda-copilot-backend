// One exact native refiner pass. Full-board captures use the existing board harness.
import { readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { createRequire } from 'node:module';
import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
const require = createRequire(import.meta.url);
const args = Object.fromEntries(process.argv.slice(2).map(s => { const i=s.indexOf('='); return [s.slice(0,i),s.slice(i+1)]; }));
const out=resolve(args.out); if(existsSync(out)) throw Error('Immutable output already exists'); mkdirSync(out,{recursive:true});
const read=p=>JSON.parse(readFileSync(p,'utf8'),(_,v)=>v && typeof v==='object' && '$nativeNumber' in v ? ({'-0':-0,NaN,Infinity,'-Infinity':-Infinity}[v.$nativeNumber]) : v);
const hash=b=>createHash('sha256').update(b).digest('hex');
let problem;
if(args.input) problem=read(args.input);
else {
 const {encodeNativePostPlaceRefineProblem}=await import('../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-refine.ts');
 const input=read(`tests/fixtures/block-placement/${args.fixture}/input.json`);
 const placements=read(args.stage).placements;
 problem=encodeNativePostPlaceRefineProblem(input,placements,Number(args.workers??1));
}
if(args.metric) problem.routingMetric=args.metric;
if(args.iterations) problem.iterations=Number(args.iterations);
if(args.timeout) problem.timeoutMs=Number(args.timeout);
const bytes=JSON.stringify(problem,(_,v)=>typeof v==='number'&&Object.is(v,-0)?{$nativeNumber:'-0'}:v);
writeFileSync(join(out,'problem.json'),bytes);
process.env.PCB_POST_PLACE_BACKEND=args.backend??'cpu';
process.env.PCB_BOARD_PACKER_PROFILE='1';
const addonPath=resolve(process.env.PCB_BOARD_PACKER_NATIVE_PATH??join('native/pcb-board-packer',require('../native/pcb-board-packer/platform.cjs').nativeFilename()));
const addon=require(addonPath);
if(args.warmBoard) {
 process.env.PCB_BOARD_BACKEND='cubecl';
 const warmStarted=performance.now();addon.solveBoardPacked(read(args.warmBoard));
 console.log(JSON.stringify({warmBoard:args.warmBoard,wallMs:performance.now()-warmStarted}));
}
const start=performance.now();
const result=addon.refinePostPlacement(problem),wallMs=performance.now()-start;
const value=({profile,...rest})=>rest;
const ref=args.reference?read(args.reference):null;
if(ref && ref.inputSha256!==hash(bytes)) throw Error('CPU input identity differs');
const exact=ref?isDeepStrictEqual(value(result),value(ref.result)):null;
const valid=addon.validatePlacementChange(problem,result.placements);
const report={inputSha256:hash(bytes),addonSha256:hash(readFileSync(addonPath)),harnessSha256:hash(readFileSync(new URL(import.meta.url))),comparisonMode:args.comparison??'exact',backend:process.env.PCB_POST_PLACE_BACKEND,wallMs,valid,exact,result};
writeFileSync(join(out,'results.json'),JSON.stringify(report,null,2));
console.log(JSON.stringify({out,wallMs,valid,exact,passes:result.profile.iterations.length,stop:result.profile.stopReason}));
// Quality mode retains mismatch evidence for separate geometry/term review.
// Default exact mode remains available for historical same-precision replays.
if(!valid || (args.comparison!=='quality'&&exact===false&&!result.profile.timedOut&&!ref.result.profile.timedOut)) throw Error('Refiner validation failed');
