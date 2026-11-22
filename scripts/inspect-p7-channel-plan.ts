/** Bounded developer evidence, never a substitute for numerical acceptance. */
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {createProblemExecutor} from '../src/analyze/execute-problem.ts';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {localRequest} from '../src/analyze/analysis-dsl/local-request.ts';
import {channelPlan} from '../src/analyze/analysis-dsl/channel-plan.ts';
const filename=process.argv.slice(2).find(v=>v!=='--reuse')??'tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json';
const board=JSON.parse(readFileSync(filename,'utf8'));
const signal=board.copper.fixed.vias.findIndex((v:any)=>v.id==='da3574692d072a0d');
const reference=board.copper.fixed.vias.findIndex((v:any)=>v.id==='aa8ff23739620a07');
if(signal<0||reference<0)throw new Error('This evidence scenario requires its original STM32 signal/return vias');
const contacts=[{sourcePath:`copper.fixed.vias[${signal}]`,layer:'TOP'},{sourcePath:`copper.fixed.vias[${signal}]`,layer:'BOTTOM'},{sourcePath:`copper.fixed.vias[${reference}]`,layer:'TOP'}];
const reuse=process.argv.includes('--reuse');const execute=createProblemExecutor();const samples=[];
for(const deviation of reuse?[.001]:[.001,.0001]){
 const start=performance.now();const g=reuse?JSON.parse(readFileSync('debugging/p7-channel-plan/prepared-geometry-original.json','utf8')):execute({kind:'prepare-geometry',boardJson:JSON.stringify(board),geometry:{max_deviation_mm:deviation}});
 const plan=channelPlan(g,contacts);
 if(deviation===.001&&!reuse){mkdirSync('debugging/p7-channel-plan',{recursive:true});writeFileSync('debugging/p7-channel-plan/prepared-geometry-original.json',JSON.stringify(g));}
 const byPath=new Map<string,unknown>();board.pads.forEach((p:any,i:number)=>byPath.set(`pads[${i}]`,p));
 for(const kind of ['tracks','vias','zones'])board.copper.fixed[kind].forEach((p:any,i:number)=>byPath.set(`copper.fixed.${kind}[${i}]`,p));
 const portIds=g.regions.filter(r=>contacts.some(c=>c.sourcePath===r.source.path&&c.layer===r.layer)).map(r=>r.id);
 const selectedIsland=g.islands.find((i:any)=>i.region_ids.includes(g.regions.find((r:any)=>r.source.path===contacts[0].sourcePath&&r.layer===contacts[0].layer)!.id))!;
 const witnesses=g.contacts.filter((c:any)=>c.reliable&&selectedIsland.region_ids.includes(c.a)&&selectedIsland.region_ids.includes(c.b)&&(portIds.includes(c.a)||portIds.includes(c.b)));
 samples.push({reusedImmutableSnapshot:reuse,deviationMm:deviation,seconds:(performance.now()-start)/1000,plan,
  portRegions:g.regions.filter(r=>portIds.includes(r.id)),confirmedSignalWitnesses:witnesses,
  diagnosticSources:plan.diagnosticDecisions.filter(d=>['contact_uncertain','drill_topology_uncertain'].includes(d.diagnostic.code)).map(d=>({...d,source:byPath.get(d.diagnostic.path)})),
  statistics:g.statistics});
}
const out='debugging/p7-channel-plan';mkdirSync(out,{recursive:true});
writeFileSync(out+'/stm32-diagnostic-sensitivity-v3.json',JSON.stringify({board:filename,samples,peakRssBytes:process.resourceUsage().maxRSS*1024,scope:'Geometry/topology evidence only, not a 5% observable sensitivity certificate'},null,2));
console.log(JSON.stringify(samples.map(s=>({deviationMm:s.deviationMm,seconds:s.seconds,regions:s.plan.selectedRegionIds.length,roi:s.plan.roiMm,uncertain:s.diagnosticSources.length,signalWitnesses:s.confirmedSignalWitnesses.length})),null,2));

if(reuse)process.exit(0);
const script=readFileSync('tests/pcb-analyze/STM32G0_CAN_RS485/local-extraction.js','utf8');
let request:unknown;
const transport=await executeScenario({board,code:script.slice(0,script.indexOf('const cold=')).replace('scope:[scope]','')+'await run();'},{filename:'automatic-request.js',maxEntries:1000,maxOutputBytes:4*1024*1024},()=>{},async task=>{request=localRequest(task as any);return{id:'transport',status:'complete',method:'request capture only',diagnostics:[],assumptions:[],metrics:{}};});
if(transport.status!=='complete')throw new Error(JSON.stringify(transport.error));writeFileSync(out+'/automatic-request.json',JSON.stringify(request,null,2));
