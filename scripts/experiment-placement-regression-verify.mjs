import {readFileSync,existsSync} from 'node:fs';
import {resolve} from 'node:path';
import {gunzipSync} from 'node:zlib';
import {BoardAssembleSchema} from '../src/types/pcb/board-assemble.ts';
const dir=resolve('docs/experimental/pcb/placement-regression-2026-09-28');
const data=JSON.parse(readFileSync(`${dir}/summary.json`));
const errors=[];
for(const board of data.boards){
    if(board.error){errors.push(`${board.name}: ${board.error}`);continue;}
    if(board.summaryBefore.inputSha256!==board.summaryAfter.inputSha256)errors.push(`${board.name}: different inputs`);
    const input=JSON.parse(readFileSync(`docs/experimental/pcb/global-placement-2026-09-27/${board.name}/input.json`));
    const expectedAssembly=new Set(input.components.filter(c=>!(c.pcb.fixedPlacement||c.pcb.edgeMount||c.pcb.edgePlace)).map(c=>c.designator));
    for(const tag of ['before','after']){
        const paths=[`${board.name}/${tag}/board.svg`,`${board.name}/${tag}/assembly.json`,`${board.name}/${tag}/result.json.gz`,
            ...board.entries.flatMap(e=>[`${board.name}/${tag}/${e.id}.svg`,
                ...(e.local?.length===2?[`${board.name}/${tag}/${e.id}-local.svg`]:[])])];
        for(const path of paths)if(!existsSync(`${dir}/${path}`))errors.push(`missing ${path}`);
        const asm=BoardAssembleSchema().parse(JSON.parse(readFileSync(`${dir}/${board.name}/${tag}/assembly.json`)));
        const placement=JSON.parse(gunzipSync(readFileSync(`${dir}/${board.name}/${tag}/result.json.gz`)));
        if(placement.placements.length!==board.components)errors.push(`${board.name}/${tag}: incomplete placement`);
        const placed=new Set(placement.placements.map(p=>p.designator));
        const assemblyComponents=asm.components??[];
        const assembled=new Set(assemblyComponents.map(c=>c.designator));
        if(assembled.size!==assemblyComponents.length)errors.push(`${board.name}/${tag}: duplicate assembly components`);
        if(assembled.size!==expectedAssembly.size||[...expectedAssembly].some(ref=>!assembled.has(ref)))
            errors.push(`${board.name}/${tag}: assembly inventory differs from movable components`);
        if([...assembled].some(ref=>!placed.has(ref)))errors.push(`${board.name}/${tag}: assembly has unknown components`);
    }
    if(!board.validation.inventory)errors.push(`${board.name}: inventory changed`);
    if(board.validation.fixedChanges.length)errors.push(`${board.name}: fixed components moved`);
}
if(!existsSync(`${dir}/comparison.html`))errors.push('missing report');
console.log(JSON.stringify({boards:data.boards.length,blocks:data.boards.reduce((n,b)=>n+(b.blocks??0),0),
    modules:data.boards.reduce((n,b)=>n+(b.modules??0),0),errors},null,2));
if(errors.length)process.exitCode=1;
