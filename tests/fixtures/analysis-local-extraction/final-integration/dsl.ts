import {readFile,writeFile} from 'node:fs/promises';
import {resolve,join} from 'node:path';
import {createHash} from 'node:crypto';
import {runAnalysisDsl} from '../../../../src/analyze/run-analysis-dsl.ts';
const out=resolve(process.argv[2]??'debugging/p7-final-integration');
const packet=JSON.parse(await readFile(join(out,'automatic.json'),'utf8'));
const candidate=packet.runs[0];
if(candidate.diagnostic||!candidate.reducedModel)throw Error('Automatic reduction was not admitted');
// Existing agreed builders; the native extraction packet is a bounded coupon.
// This checks the existing P6/worker interchange, not board extraction through DSL.
const scenario=await readFile('tests/fixtures/analysis-local-extraction/current-space/scenario.js','utf8');
const code='const candidate='+JSON.stringify(candidate)+';\n'+scenario;
const start=performance.now();
const result=await runAnalysisDsl({code},{filename:'p7-final-integration-bounded-reduction.js',timeoutMs:120000,maxOutputBytes:8*1024*1024,maxEntries:2000});
await writeFile(join(out,'scenario.js'),code);
await writeFile(join(out,'dsl-result.json'),JSON.stringify({codeSha256:createHash('sha256').update(code).digest('hex'),wallMs:performance.now()-start,result}));
if(result.status!=='complete')throw Error(JSON.stringify(result.error));
const logs=result.reports.flatMap(r=>r.logs),checks=result.reports.flatMap(r=>r.checks),plots=result.reports.flatMap(r=>r.plots);
if(logs.filter(l=>l.message==='p6-p7.structural-gauge.complete').length!==2||!logs.some(l=>l.message==='p6-p7.structural-gauge.continued')||plots.length!==2)throw Error('Incomplete worker interchange or continuation');
const axis=candidate.rows.filter((r:{drive:string})=>r.drive==='opposing').map((r:{frequencyHz:number})=>r.frequencyHz);
for(const plot of plots as Array<{kind?:string;data?:{kind?:string;values?:number[];axis?:{values?:number[]}}}>) {
 if(plot.kind!=='line'||plot.data?.kind!=='series'||plot.data.values?.length!==17||JSON.stringify(plot.data.axis?.values)!==JSON.stringify(axis))throw Error('Incomplete original-axis native graph series');
}
console.log(JSON.stringify({status:result.status,checks,plotCount:plots.length,wallMs:performance.now()-start}));
