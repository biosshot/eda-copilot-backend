import { runAutoFoil } from './auto-foil-response.ts';
import { declaredNetworkBand } from './analysis-dsl/frequency-model.ts';
import { boardChannelHybridBandHz, createBoardChannelPlanner } from './board-channel-plan.ts';
import type { BoardChannelRequest } from './channel-components.ts';
import type { LegacyChannelContact } from './channel-terminals.ts';
import type { FrequencyInterval } from './p7-frequency-planning.ts';
import type { BoardFoilProblem, BoardFoilReport } from './types.ts';

export function boardChannelApplicability(request: BoardChannelRequest): FrequencyInterval {
 const bands=(request.components??[]).flatMap(component=>{
  const band=declaredNetworkBand(component.model);
  return band?[band]:[];
 });
 const band=boardChannelHybridBandHz;
 return [Math.max(band[0],...bands.map(component=>component[0])),
  Math.min(band[1],...bands.map(component=>component[1]))];
}
/** The automatic board AC entry point uses the joint hybrid model for every supported band. */
export function solveHybridBoardChannel(
 execute:(problem:BoardFoilProblem)=>BoardFoilReport,
 board:unknown,base:unknown,contacts:LegacyChannelContact[]|BoardChannelRequest,
 frequencies:number[],budget:BoardFoilProblem['request']['budget'],interval:FrequencyInterval|undefined,
) {
 const foil=createBoardChannelPlanner(board,base,contacts)({closedJointDeadlineMs:Infinity,guardedReturn:true});
 return {foil,report:runAutoFoil(foil,frequencies,budget,interval,execute)};
}
