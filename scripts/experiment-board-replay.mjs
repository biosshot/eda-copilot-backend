// Replay complete resolved boards; only ordinary block solves receive experiments.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { autoPlacePcbWithReportAsync, renderPlacementSvg } from '../src/pcb-layout/pcb-auto-place/auto-place.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import { createPlacementDebugArtifacts, writePlacementArtifacts } from '../src/pcb-layout/artifacts.ts';
import { getPadWorld } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { variants } from './experiment-block-replay.mjs';
import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { resolve } from 'node:path';

process.env.PCB_LAYOUT_SUBTREE_WORKERS = '0';
process.env.PCB_NATIVE_SOLVE_CACHE = '0';
const [fixture = 'Telemetry', variant = 'B0'] = process.argv.slice(2);
if (!(variant in variants)) throw Error(`Unknown variant ${variant}`);
const input = JSON.parse(readFileSync(`tests/fixtures/block-placement/${fixture}/input.json`));
const addon = loadNativeBoardPacker(); const original = addon.solveBlockPrimitives;
const captures = [];
Object.defineProperty(addon, 'solveBlockPrimitives', { configurable:true,value:problem=>{
 const p = structuredClone(problem);
 const ordinary = new Set(p.components.map(c=>c.blockName)).size === 1 && p.primitives.length <= 12;
 if (ordinary) { const {width,...experiments}=variants[variant]; p.experiments={...experiments, ignoredNets:input.solverOptions.ignoredRatsnestSignals}; if(width) p.searchWidth=width; }
 const start=performance.now(); const solution=original(p);
 captures.push({problem:p,solution,ms:performance.now()-start}); return solution;
}});
const start=performance.now(); const result=await autoPlacePcbWithReportAsync(input);
const dir=`debugging/board-experiments/${fixture}/${variant}`; mkdirSync(dir,{recursive:true});
writePlacementArtifacts(dir,input,result.placements,result.report,result.layout,result.stages,renderPlacementSvg(input,result.placements),createPlacementDebugArtifacts(input,result.placements));
const stageMetrics = result.stages.map(stage=>{
 const nets = new Map();
 for(const c of input.components) {
  const placement = stage.placements.find(p=>p.designator===c.designator); if(!placement) continue;
  for(const pin of c.pins) {
   if(!pin.signal_name || input.solverOptions.ignoredRatsnestSignals.includes(pin.signal_name)) continue;
   const point=getPadWorld(c,placement,pin.pin_number); if(!point) continue;
   const pts=nets.get(pin.signal_name)??[]; pts.push({...point,ref:`${c.designator}.${pin.pin_number}`}); nets.set(pin.signal_name,pts);
  }
 }
 let hpwl=0; const pairs={};
 for(const [net,pts] of nets) {
  hpwl+=Math.max(...pts.map(p=>p.x))-Math.min(...pts.map(p=>p.x))+Math.max(...pts.map(p=>p.y))-Math.min(...pts.map(p=>p.y));
  if(pts.length===2) pairs[net]=Math.hypot(pts[0].x-pts[1].x,pts[0].y-pts[1].y);
 }
 return {stage:stage.name,hpwl,pairSum:Object.values(pairs).reduce((a,b)=>a+b,0),pairMax:Math.max(0,...Object.values(pairs)),pairs};
});
const require=createRequire(import.meta.url);
const nativePath=resolve(process.env.PCB_BOARD_PACKER_NATIVE_PATH??`native/pcb-board-packer/${require('../native/pcb-board-packer/platform.cjs').nativeFilename()}`);
const fixedChanges=input.components.filter(c=>c.pcb.fixedPlacement).flatMap(c=>{
 const p=result.placements.find(p=>p.designator===c.designator), fixed=c.pcb.fixedPlacement;
 return !p || Math.abs(p.x-fixed.x)>.005 || Math.abs(p.y-fixed.y)>.005 || p.rotate!==fixed.rotate || p.layer!==fixed.layer ? [c.designator]:[];
});
const summary={fixture,variant,ms:performance.now()-start,ok:result.report.ok,stageMetrics,report:result.report,fixedChanges,
 nativeHash:createHash('sha256').update(readFileSync(nativePath)).digest('hex'),
 inputHash:createHash('sha256').update(JSON.stringify(input)).digest('hex'),node:process.version,
 settings:variants[variant],scope:'single-block native solves with at most 12 primitives; board and postrefine unchanged'};
writeFileSync(`${dir}/summary.json`,JSON.stringify(summary,null,2));
writeFileSync(`${dir}/captures.json`,JSON.stringify(captures));
console.log(JSON.stringify({...summary,report:undefined,stageMetrics:stageMetrics.map(s=>({...s,pairs:undefined}))}));
