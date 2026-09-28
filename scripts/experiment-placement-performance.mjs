import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {gzipSync} from 'node:zlib';
import {autoPlacePcbWithReportAsync} from '../src/pcb-layout/pcb-auto-place/auto-place.ts';
import {loadNativeBoardPacker} from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';

const [board='esp32c3',tag='before',routingMetric]=process.argv.slice(2);
if(routingMetric&&!['geometric','micro'].includes(routingMetric))throw Error('Unsupported routing metric');
const out=`docs/experiments/placement-performance-2026-09-28/${board}/${tag}`;
mkdirSync(out,{recursive:true});
const input=JSON.parse(readFileSync(`docs/experiments/global-placement-2026-09-27/${board}/input.json`));
const addon=loadNativeBoardPacker(),calls=[];
for(const method of ['solveBlockPrimitives','solveBlockPrimitivesBatch','solvePassiveNetIsland','solveBoardPacked','refinePostPlacement','scorePostPlace',
    'scoreRouteLayout','scoreRouteLayoutWithObstacles','prepareRouteLayoutComparison','compareRouteLayoutCandidate']){
    const original=addon[method];
    if(!original)continue;
    Object.defineProperty(addon,method,{configurable:true,value:(...args)=>{
        if(method==='solveBlockPrimitives'&&routingMetric&&args[0].experiments)
            args[0]={...args[0],experiments:{...args[0].experiments,routingMetric}};
        const start=performance.now();
        const result=original(...args);
        const ms=performance.now()-start;
        calls.push({method,ms,components:args[0].componentCount??args[0].components?.length,
            blocks:[...new Set(args[0].components?.map(c=>c.blockName).filter(Boolean)??[])],profile:result?.profile});
        if(method==='solveBlockPrimitives')writeFileSync(`${out}/block-${calls.length}.json.gz`,gzipSync(JSON.stringify({problem:args[0],solution:result,ms})));
        if(method==='solveBlockPrimitivesBatch')args[0].forEach((problem,i)=>writeFileSync(`${out}/block-${calls.length}-${i}.json.gz`,gzipSync(JSON.stringify({problem,solution:result[i],batchMs:ms}))));
        if(method.startsWith('solve'))console.log(JSON.stringify(calls.at(-1)));
        return result;
    }});
}
const start=performance.now();
const result=await autoPlacePcbWithReportAsync(input);
const ms=performance.now()-start;
const byMethod=Object.fromEntries([...new Set(calls.map(c=>c.method))].map(m=>[m,{calls:calls.filter(c=>c.method===m).length,
    ms:calls.filter(c=>c.method===m).reduce((s,c)=>s+c.ms,0)}]));
writeFileSync(`${out}/summary.json`,JSON.stringify({board,tag,routingMetric:routingMetric??'policy',ms,byMethod,calls,metrics:placementMetrics(input,result.placements),report:result.report},null,2));
writeFileSync(`${out}/result.json.gz`,gzipSync(JSON.stringify(result)));
console.log(JSON.stringify({board,tag,ms,byMethod}));
