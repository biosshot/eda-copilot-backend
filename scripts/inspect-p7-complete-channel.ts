import {readFileSync,mkdirSync,writeFileSync} from 'node:fs';
import {executeScenario} from '../src/analyze/analysis-dsl/scenario.ts';
import {localRequest} from '../src/analyze/analysis-dsl/local-request.ts';
const base='tests/pcb-analyze/STM32G0_CAN_RS485/';
const board=JSON.parse(readFileSync(base+'STM32G0_CAN_RS485.json','utf8'));
const code=readFileSync(base+'local-extraction-complete-channel.js','utf8').split('a.plot(')[0];
let request:any,plan:any;
const result=await executeScenario({board,code},{filename:'complete-channel-plan.js',maxEntries:1000,maxOutputBytes:64*1024*1024},()=>{},async task=>{
 request=localRequest(task as any,p=>{plan=p;});
 return {id:'plan-only',status:'complete',method:'Geometry planning only; no field solve or physical admission',diagnostics:[],assumptions:[],metrics:{}};
});
const folder='debugging/p7-channel-plan/remote-plan-'+Date.now();mkdirSync(folder,{recursive:true});
writeFileSync(folder+'/evidence.json',JSON.stringify({status:'planning-only',result,request,plan},null,2));
console.log(JSON.stringify({folder,status:result.status,error:result.error,contacts:plan?.contacts,regions:plan?.connectedIslandRegionIds.length}));

