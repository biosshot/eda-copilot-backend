import type { AnalysisLoadedInstanceLoss } from '../../native/analyze/index.js';

export interface ChannelLossGram {
 frequencyHz: number;
 copperWPerA2: number[][];
 componentWPerA2: number[][];
 oppositeCopperW: number;
 oppositeComponentW: number;
}

const fail=(reason:string):never=>{throw new Error('CHANNEL_COPPER_LOSS_EVIDENCE: '+reason);};

/** Aggregate the native resistance ownership partition. The Cu and package
 * labels come from the plan's separate physical instance inventories. */
export function channelLossGrams(
 loss:AnalysisLoadedInstanceLoss|undefined,
 frequencies:readonly number[], ports:number,
 copperIds:readonly string[], componentIds:readonly string[],
):ChannelLossGram[]{
 const owners=[...copperIds,...componentIds];
 const copper=new Set(copperIds);
 if(new Set(owners).size!==owners.length||loss?.instanceIds.length!==owners.length||
    new Set(loss.instanceIds).size!==owners.length||owners.some(id=>!loss.instanceIds.includes(id))||
    loss.samples.length!==frequencies.length) return fail('Missing or ambiguous native instance ownership');
 const zero=()=>Array.from({length:ports},()=>Array<number>(ports).fill(0));
 return loss.samples.map(({frequencyHz,jouleWPerA2,oppositeDriveW},index)=>{
  if(frequencyHz!==frequencies[index]||jouleWPerA2.length!==owners.length||oppositeDriveW.length!==owners.length)
   fail('Loss samples must match the complete physical frequency axis and owners');
  const copperWPerA2=zero(),componentWPerA2=zero();
  let oppositeCopperW=0,oppositeComponentW=0;
  for(const [owner,matrix] of jouleWPerA2.entries()){
   if(matrix.length!==ports||matrix.some(row=>row.length!==ports))
    fail('Loss matrix does not match ordered output ports');
   const isCopper=copper.has(loss.instanceIds[owner]);
   const sum=isCopper?copperWPerA2:componentWPerA2;
   const opposite=oppositeDriveW[owner];
   if(!Number.isFinite(opposite)||opposite<0)fail('Nonfinite or negative opposite-drive owner loss');
   if(isCopper)oppositeCopperW+=opposite;
   else oppositeComponentW+=opposite;
   for(let i=0;i<ports;i++)for(let j=0;j<ports;j++){
    const value=matrix[i][j];
    if(!Number.isFinite(value)||value!==matrix[j][i])fail('Loss matrix must be finite and symmetric');
    sum[i][j]+=value;
    if(!Number.isFinite(sum[i][j]))fail('Nonfinite category loss');
   }
  }
  if(!Number.isFinite(oppositeCopperW)||!Number.isFinite(oppositeComponentW))fail('Nonfinite opposite-drive category loss');
  return {frequencyHz,copperWPerA2,componentWPerA2,oppositeCopperW,oppositeComponentW};
 });
}
