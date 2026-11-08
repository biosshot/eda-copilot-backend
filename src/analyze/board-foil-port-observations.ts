import type { AutoFoilPlan } from './auto-foil-plan.ts';
import type { BoardFoilReport } from './types.ts';

type Complex = Readonly<{real:number;imaginary:number}>;
type ContactCurrent = {sourcePath:string;layer:string;currentA:Complex};

/** Recover only finite physical-port quantities from a checked matched-S report.
 * This does not recover a spatial copper-current map. */
export function boardFoilPortObservations(plan:AutoFoilPlan,report:BoardFoilReport) {
 const n=plan.ports.length, nf=report.frequenciesHz.length;
 if(!n||n>16||nf===0||nf*n>256||report.parameters.length!==nf||report.samples.length!==nf||
    report.referenceOhm.length!==n||plan.electrodePairs.length!==n)
  throw new Error('CHANNEL_FOIL_OBSERVABLES: inconsistent or oversized port/frequency dimensions');
 const terminals=new Map(plan.terminals.map(t=>[t.id,t]));
 const add=(a:Complex,b:Complex):Complex=>({real:a.real+b.real,imaginary:a.imaginary+b.imaginary});
 const finite=(x:number)=>Number.isFinite(x);
 const output=[];
 for(let f=0;f<nf;f++){
  const frequencyHz=report.frequenciesHz[f],s=report.parameters[f],sample=report.samples[f];
  if(!finite(frequencyHz)||frequencyHz<=0||sample.frequencyHz!==frequencyHz||s.length!==n||sample.columns.length!==n)
   throw new Error('CHANNEL_FOIL_OBSERVABLES: malformed checked sample dimensions or frequency');
  const references=report.referenceOhm;
  if(references.some(r=>!finite(r)||r<=0)||s.some(row=>row.length!==n||row.some(z=>!finite(z.real)||!finite(z.imaginary))))
   throw new Error('CHANNEL_FOIL_OBSERVABLES: finite square S matrix and positive finite references required');
  const drives=[];
  for(let j=0;j<n;j++){
   const col=sample.columns[j],nativeCopperLossW=col.copperLossW;
   if(!finite(nativeCopperLossW)||nativeCopperLossW<0||!finite(col.powerBalanceErrorW)||col.packages?.some(p=>!finite(p.lossW)||p.lossW<0||!finite(p.currentA.real)||!finite(p.currentA.imaginary)))
    throw new Error('CHANNEL_FOIL_OBSERVABLES: non-finite loss/current observation');
   const scale=Math.sqrt(references[j])/2;
   const currents:ContactCurrent[]=[];let totalPowerW=nativeCopperLossW;
   for(let i=0;i<n;i++){
    const coefficient=s[i][j];
    const multiplier=Math.sqrt(references[i])*scale;
    const voltage={real:multiplier*((i===j?1:0)+coefficient.real),imaginary:multiplier*coefficient.imaginary};
    const portCurrent={real:(i===j?1:0)-voltage.real/references[i],imaginary:-voltage.imaginary/references[i]};
    if(!finite(voltage.real)||!finite(voltage.imaginary)||!finite(portCurrent.real)||!finite(portCurrent.imaginary))
     throw new Error('CHANNEL_FOIL_OBSERVABLES: recovered port quantity overflow');
    const port=plan.ports[i],positive=terminals.get(port.positive),negative=terminals.get(port.negative);
    if(!positive||!negative)throw new Error('CHANNEL_FOIL_OBSERVABLES: physical port terminal missing');
    currents.push({sourcePath:positive.sourcePath,layer:positive.layer,currentA:portCurrent},
      {sourcePath:negative.sourcePath,layer:negative.layer,currentA:{real:-portCurrent.real,imaginary:-portCurrent.imaginary}});
   }
   const aggregated=new Map<string,ContactCurrent>();
   for(const contact of currents){
    const key=JSON.stringify([contact.sourcePath,contact.layer]);
    const prior=aggregated.get(key);
    const currentA=prior?add(prior.currentA,contact.currentA):contact.currentA;
    if(!finite(currentA.real)||!finite(currentA.imaginary))throw new Error('CHANNEL_FOIL_OBSERVABLES: aggregated contact current overflow');
    aggregated.set(key,prior?{...prior,currentA}:contact);
   }
   const packages=(col.packages??[]).map(p=>({id:p.id,lossW:p.lossW}));
   for(const item of packages)totalPowerW+=item.lossW;
   if(!finite(totalPowerW))throw new Error('CHANNEL_FOIL_OBSERVABLES: total power overflow');
   drives.push({drivePort:plan.ports[j].name,driveCurrentA:1,portObservations:plan.ports.map((port,i)=>{
    const positive=terminals.get(port.positive),negative=terminals.get(port.negative);
    if(!positive||!negative)throw new Error('CHANNEL_FOIL_OBSERVABLES: named port terminal missing');
    const multiplier=Math.sqrt(references[i])*scale;
    const voltageV={real:multiplier*((i===j?1:0)+s[i][j].real),imaginary:multiplier*s[i][j].imaginary};
    const currentA={real:(i===j?1:0)-voltageV.real/references[i],imaginary:-voltageV.imaginary/references[i]};
    return {port:port.name,positive:{sourcePath:positive.sourcePath,layer:positive.layer},negative:{sourcePath:negative.sourcePath,layer:negative.layer},voltageV,currentA};
   }),contactCurrents:[...aggregated.values()],nativeCopperLossW,packageLossesW:packages,totalCopperAndPackageLossW:totalPowerW});
  }
  output.push({frequencyHz,drives});
 }
 return {frequencies:output,spatialRecovery:{status:'unverified' as const,reason:'Port-contact currents and voltages are recovered from S only; no spatial copper return-current map is computed.'}};
}
