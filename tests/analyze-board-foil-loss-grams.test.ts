import test from 'node:test';
import assert from 'node:assert/strict';
import type {AnalysisBoardFoilCheckedReport} from '../native/analyze/index.js';
import {validateBoardFoilLossGrams,projectBoardFoilLossGrams} from '../src/analyze/board-foil-loss-grams.ts';
import {boardFoilMixedMode} from '../src/analyze/board-foil-mixed-mode.ts';
const z=(real:number,imaginary=0)=>({real,imaginary});
function fixture(){
 const current=Math.sqrt(.5);
 return {referenceOhm:[4,4],frequenciesHz:[1000],parameters:[[[z(0),z(0)],[z(0),z(0)]]],samples:[{
  frequencyHz:1000,columns:[z(current),z(0,-current)].map((currentA,input)=>({input,copperLossW:.5,powerBalanceErrorW:0,kclRelative:0,kvlRelative:0,packages:[{id:'r',currentA,lossW:.5}]})),
  lossGrams:{copperWPerA2:[[z(.5),z(0,.5)],[z(0,-.5),z(.5)]],packages:[{id:'r',powerWPerA2:[[z(.5),z(0,-.5)],[z(0,.5),z(.5)]]}]},
 }]} as unknown as AnalysisBoardFoilCheckedReport;
}
const owners=[{id:'r',resistanceOhm:1}];
test('complex loss Grams retain destructive and constructive interference',()=>{
 const report=fixture();validateBoardFoilLossGrams(report,owners);
 const value=projectBoardFoilLossGrams(report.samples[0].lossGrams!,[z(1),z(0,1)]);
 assert.ok(Math.abs(value.copperLossW)<1e-12);
 assert.ok(Math.abs(value.packages[0].lossW-2)<1e-12);
 assert.ok(Math.abs(value.totalLossW-2)<1e-12);
 const modal=boardFoilMixedMode(report,['p','n'],[{positive:'p',negative:'n',differential:'d',common:'c'}]);
 assert.equal(modal.lossRecovery.separateCopperAndPackages,'computed');
 assert.ok(Math.abs(modal.parameters[0].losses![0].copperLossW-1)<1e-12);
 assert.ok(Math.abs(modal.parameters[0].losses![1].packages[0].lossW-.25)<1e-12);
});
test('loss Gram validation rejects false owners, diagonals, phases and indefinite matrices',()=>{
 const changes=[
  (r:AnalysisBoardFoilCheckedReport)=>{r.samples[0].lossGrams!.packages[0].id='missing';},
  (r:AnalysisBoardFoilCheckedReport)=>{r.samples[0].lossGrams!.copperWPerA2[0][0].real=.6;},
  (r:AnalysisBoardFoilCheckedReport)=>{r.samples[0].lossGrams!.packages[0].powerWPerA2[0][1].imaginary=.5;},
  (r:AnalysisBoardFoilCheckedReport)=>{r.samples[0].lossGrams!.copperWPerA2=[[z(.5),z(.6)],[z(.6),z(.5)]];},
  (r:AnalysisBoardFoilCheckedReport)=>{r.samples[0].lossGrams!.copperWPerA2=[[z(.5),z(0,-.5)],[z(0,.5),z(.5)]];},
 ];
 for(const change of changes){const report=fixture();change(report);assert.throws(()=>validateBoardFoilLossGrams(report,owners),/CHANNEL_FOIL_EVIDENCE/);}
 assert.throws(()=>projectBoardFoilLossGrams(fixture().samples[0].lossGrams!,[z(Infinity),z(0)]),/nonfinite/);
 assert.throws(()=>projectBoardFoilLossGrams({copperWPerA2:[[z(-1e-15),z(0)],[z(0),z(0)]],packages:[]},[z(1),z(0)]),/positive semidefinite|zero pivot/);
});
