import {readFileSync,writeFileSync} from 'node:fs';
import {createProblemExecutor} from '../src/analyze/execute-problem.ts';
const [composedDir,referenceDir]=process.argv.slice(2);
if(!composedDir||!referenceDir)throw new Error('Supply composed and full-reference evidence directories');
const packet=JSON.parse(readFileSync(composedDir+'/result.json','utf8'));
const composed=packet.reports.flatMap((r:any)=>r.calculations).find((c:any)=>c.phase==='completed').result.solution.result.assembledModel;
const fullPacket=JSON.parse(readFileSync(referenceDir+'/result.json','utf8'));
const full=fullPacket.report.model;
const rows=[];
for(const [name,original] of Object.entries({composed,full}))for(const control of ['original','zero-L'] as const){
 const model=structuredClone(original) as any;if(control==='zero-L')model.inductanceH=[];
 const response=createProblemExecutor()({kind:'frequency-network',request:{instances:[{id:'control',ports:['input','output'],model:{id:'counterfactual',kind:'network',ports:['input','output'],data:model}}],connections:[],ports:[{name:'input',index:0},{name:'output',index:1}],frequencies:[1000],representation:'S',referenceOhm:[50,50]}});
 if(response.status!=='complete')throw new Error(JSON.stringify(response.diagnostics));
 const s=response.parameters![0][1][0];rows.push({name,control,phaseDelayS:-Math.atan2(s.imaginary,s.real)/(2*Math.PI*1000)});
}
const result={composedDir,referenceDir,frequencyHz:1000,rows,scope:'Counterfactual operator diagnostic, not a physical extracted model or additive error certificate. Original models are unchanged. P6 node welding cannot introduce omitted off-block electromagnetic energy terms.'};
writeFileSync(composedDir+'/interaction-diagnostic.json',JSON.stringify(result,null,2));console.log(JSON.stringify(result));
