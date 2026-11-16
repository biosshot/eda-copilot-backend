/** Refusals that exhaust a time, memory or mesh budget get one sentence naming the budget and how to raise it (the scenario's own `.budget()`). */
export function withBudgetHint(code:string,message:string,budget:{maxSeconds:number;maxMemoryMiB:number},machineLimited=false){
 if(!/(MEMORY_LIMIT|MESH_LIMIT|DEADLINE|BUDGET|TIMEOUT)/.test(code)&&!/(MEMORY_LIMIT|MESH_LIMIT|deadline)/i.test(message))return message;
 return message+' (budget in use: '+budget.maxSeconds+' s, '+budget.maxMemoryMiB+' MiB'+(machineLimited?', derived from the machine: available RAM minus 15%; this is a machine-memory limit outside the extractor, not a request budget':'')+'; raise it with .budget({maxSeconds, maxMemoryMiB}) on the calculation in the scenario)';
}
