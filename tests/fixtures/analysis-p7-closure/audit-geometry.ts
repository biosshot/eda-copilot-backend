import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {capturedBoardGeometry} from '../../../src/analyze/geometry-snapshot.ts';
const m=JSON.parse(readFileSync('tests/fixtures/analysis-p7-closure/manifest.json','utf8'));mkdirSync('debugging/p7-closure',{recursive:true});const results:any[]=process.argv.includes('--coupons')?JSON.parse(readFileSync('debugging/p7-closure/geometry-audit.json','utf8')).filter((c:any)=>m.cases.some((x:any)=>x.id===c.id)):[];
for(const c of [...(process.argv.includes('--coupons')?[]:m.cases.filter((c:any)=>c.board)),...m.coupons]){
 try{const board=JSON.parse(readFileSync(c.board,'utf8')),g=capturedBoardGeometry(JSON.stringify(board));
 const contacts=(c.contacts??[[board.pads.find((p:any)=>p.component==='J0'&&p.number==='1'),board.pads.find((p:any)=>p.component==='J0'&&p.number==='2')],[board.pads.find((p:any)=>p.component==='J1'&&p.number==='1'),board.pads.find((p:any)=>p.component==='J1'&&p.number==='2')]]).map((pair:any[])=>pair.map(p=>({id:p.id,net:p.net,islands:g.islands.filter(i=>i.region_ids.some(id=>g.regions.find(r=>r.id===id)?.source.source_id===p.id)).map(i=>i.id)})));
 const connected=[0,1].map(i=>contacts[0][i].islands.some((id:string)=>contacts[1][i].islands.includes(id)));
 results.push({id:c.id,status:g.status,contacts,signalConnected:connected[0],returnConnected:connected[1],findings:g.findings,diagnostics:g.diagnostics,regions:g.regions,islands:g.islands,physicalContacts:g.contacts});
 }catch(e){results.push({id:c.id,status:'blocked',diagnostic:String(e)});}
}
writeFileSync('debugging/p7-closure/geometry-audit.json',JSON.stringify(results));console.log(JSON.stringify(results.map(({regions,islands,physicalContacts,...c})=>c)));
