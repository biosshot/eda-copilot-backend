import {readFileSync,writeFileSync,mkdirSync,existsSync,openSync,closeSync} from 'node:fs';
import {spawn,execFileSync} from 'node:child_process';
import {resolve} from 'node:path';
const current=resolve('.'),baseline=resolve('..','eda-copilot-baseline-run');
const output=resolve('docs/experiments/placement-regression-2026-09-28');
const bank=resolve('docs/experiments/global-placement-2026-09-27');
const measurements=JSON.parse(readFileSync(`${bank}/measurements.json`));
const names=process.argv.slice(2);
const boards=measurements.filter(b=>b.entities?.length&&!b.duplicateOf&&(!names.length||names.includes(b.name)));
const results=[];
mkdirSync(output,{recursive:true});
async function run(board,tag,root){
    const dir=resolve(output,board.name,tag);
    mkdirSync(dir,{recursive:true});
    if(existsSync(`${dir}/summary.json`)&&existsSync(`${dir}/result.json.gz`)&&existsSync(`${dir}/assembly.json`))return {fixture:board.name,tag,code:0,cached:true};
    const expected=tag==='before'?'b8fe403':'8311f7b';
    const revision=execFileSync('git',['rev-parse','--short=7','HEAD'],{cwd:root,encoding:'utf8'}).trim();
    if(revision!==expected)throw Error(`${tag} requires checkout ${expected}, found ${revision}. Use a dedicated checkout; do not mix new results into this historical report.`);
    const fd=openSync(`${dir}/run.log`,'w');
    const input=resolve(bank,board.name,'input.json');
    const t=Date.now();
    const outcome=await new Promise(resolveResult=>{
        const child=spawn(process.execPath,['--import','tsx','scripts/experiment-placement-regression-run.mjs',input,dir],{
            cwd:root,windowsHide:true,stdio:['ignore',fd,fd],
            env:{...process.env,PCB_LAYOUT_SUBTREE_WORKERS:'0',PCB_POST_PLACE_THREADS:'4',PCB_BOARD_PACKER_THREADS:'4'},
        });
        const timer=setTimeout(()=>child.kill(),60*60_000);
        child.on('error',e=>resolveResult({error:e.message}));
        child.on('close',(code,signal)=>{clearTimeout(timer);resolveResult({code,signal});});
    });
    closeSync(fd);
    return {fixture:board.name,tag,...outcome,wallMs:Date.now()-t};
}
async function worker(){
    while(boards.length){
        const board=boards.shift();
        for(const [tag,root]of [['before',baseline],['after',current]]){
            const result=await run(board,tag,root);results.push(result);
            writeFileSync(`${output}/runs.json`,JSON.stringify(results,null,2));
            console.log(`${results.length}/${measurements.filter(b=>b.entities?.length&&!b.duplicateOf).length*2} ${result.fixture} ${tag}: ${JSON.stringify(result)}`);
        }
    }
}
await Promise.all([worker(),worker()]);
if(results.some(r=>r.code!==0))process.exitCode=1;
