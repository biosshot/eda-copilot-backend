import {totalmem} from 'node:os';
/** Budget `maxMemoryMiB` of a calculation: the request's `.budget()` when given, else a cap derived from the machine
 * (total physical RAM minus a 15% margin, read once per process; total, not free, so the cap is reproducible). `P7_VERIFY_MEMORY_MIB` / `P7_ROI_BUDGET_MIB` override both natively. */
const MARGIN=0.15,MIN_MIB=256,MAX_MIB=65536;
let cached:number|undefined;
export function machineMemoryCapMiB(totalBytes?:number):number{
 if(totalBytes===undefined&&cached!==undefined)return cached;
 const value=Math.min(MAX_MIB,Math.max(MIN_MIB,Math.floor((totalBytes??totalmem())*(1-MARGIN)/1048576)));
 if(totalBytes===undefined)cached=value;
 return value;
}
export function resolveMemoryMiB(requested:number|undefined,totalBytes?:number):{maxMemoryMiB:number;machineLimited:boolean}{
 return requested!==undefined?{maxMemoryMiB:requested,machineLimited:false}:{maxMemoryMiB:machineMemoryCapMiB(totalBytes),machineLimited:true};
}
