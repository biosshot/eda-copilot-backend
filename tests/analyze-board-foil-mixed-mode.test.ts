import test from 'node:test';
import assert from 'node:assert/strict';
import { boardFoilMixedMode } from '../src/analyze/board-foil-mixed-mode.ts';
import type { BoardFoilReport } from '../src/analyze/types.ts';
import { boardFoilResult } from '../src/analyze/analysis-dsl/board-foil-result.ts';
import type { AutoFoilPlan } from '../src/analyze/auto-foil-plan.ts';

function report(names:string[],refs:number[],s:{real:number;imaginary:number}[][]):BoardFoilReport {
 return {status:'complete',coverage:'unverified',representation:'S',frequenciesHz:[1e6],referenceOhm:refs,ports:names.map((_,i)=>[String(i),String(i)]),parameters:[s],samples:[{frequencyHz:1e6,columns:names.map((_,input)=>({input,kclRelative:0,kvlRelative:0,copperLossW:0,powerBalanceErrorW:0}))}],enrichment:[],quality:{maxKclRelative:0,maxKvlRelative:0,maxColumnPowerBalanceErrorW:0},provenance:{method:'test',modelKind:'mixed',sourceMapping:[],barrelIds:[],electrodeAreasM2:{},maxCopperVolumeChange:0,prisms:0,nativeCopperTetrahedra:0,volumeTetrahedra:0,fullPotentialCount:1,fullCurrentCount:1,retainedPotentials:1,currentModes:1,plannedBytes:1,maxSeconds:1,maxMemoryMiB:1,magneticQuadratureCoverage:'unverified',spatialCoverage:'unverified',bandCoverage:'unverified',driveConvention:'1A RMS',attachments:[]}} as unknown as BoardFoilReport;
}

test('board result publishes the explicitly paired mixed-mode response',()=>{
 const names=['p','n'],pairs=[{positive:'p',negative:'n',differential:'d',common:'c'}];
 const terminals=Array.from({length:4},(_,i)=>({id:'t'+i,sourcePath:'pads['+i+']',layer:'TOP'}));
 const plan={boardJson:'{}',ports:names.map((name,i)=>({name,positive:'t'+(2*i),negative:'t'+(2*i+1)})),terminals,
  electrodePairs:[['native0','native1'],['native2','native3']],sourceCopper:[],sourceBarrels:[],attachments:[],geometry:{electrodes:[]},roiMm:[[0,0],[1,1]],modePairs:pairs} as AutoFoilPlan;
 const source=report(names,[50,50],[[{real:.1,imaginary:0},{real:.8,imaginary:0}],[{real:.8,imaginary:0},{real:.1,imaginary:0}]]);
 const result=boardFoilResult('mixed',plan,source,[]),solution=result.solution as any;
 assert.deepEqual(solution.mixedMode.portNames,['d','c']);
 assert.ok(Math.abs(solution.mixedMode.parameters[0].parameters[0][0].real+.7)<1e-12);
 assert.equal(solution.mixedMode.physicalCoverage.status,'unverified');
 assert.equal((result.network!.data as any).representation,'S');
 assert.deepEqual(result.network!.ports,names);
});

test('orthogonal differential/common transform gives expected symmetric coupled modes',()=>{
 const input=report(['p','n'],[50,50],[[{real:.1,imaginary:0},{real:.8,imaginary:0}],[{real:.8,imaginary:0},{real:.1,imaginary:0}]]);
 const out=boardFoilMixedMode(input,['p','n'],[{positive:'p',negative:'n',differential:'d',common:'c'}]);
 assert.deepEqual(out.portNames,['d','c']);assert.deepEqual(out.referenceOhm,[100,25]);
 assert.deepEqual(out.sourcePortNames,['p','n']);assert.equal(out.modePairs[0].positive,'p');
 assert.ok(Math.abs(out.parameters[0].parameters[0][0].real-(-.7))<1e-12);
 assert.ok(Math.abs(out.parameters[0].parameters[1][1].real-.9)<1e-12);
 assert.ok(Math.abs(out.parameters[0].parameters[0][1].real)<1e-12);
 assert.equal(out.physicalCoverage.status,'unverified');
});

test('complex orthogonal transform preserves column wave norm and retains unpaired port',()=>{
 const s=[[{real:.1,imaginary:.2},{real:.3,imaginary:-.1},{real:.2,imaginary:.4}],
  [{real:-.2,imaginary:.1},{real:.4,imaginary:.3},{real:.1,imaginary:-.2}],
  [{real:.05,imaginary:0},{real:.02,imaginary:.03},{real:.6,imaginary:0}]];
 const input=report(['p','n','u'],[50,50,75],s),before=structuredClone(input);
 const out=boardFoilMixedMode(input,['p','n','u'],[{positive:'p',negative:'n',differential:'d',common:'c'}]);
 const transformed=out.parameters[0].parameters;
 assert.deepEqual(out.portNames,['d','c','u']);assert.deepEqual(out.referenceOhm,[100,25,75]);
 const norm=(m:{real:number;imaginary:number}[][])=>m.flat().reduce((sum,z)=>sum+z.real**2+z.imaginary**2,0);
 assert.ok(Math.abs(norm(s)-norm(transformed))<1e-12);
 assert.deepEqual(transformed[2][2],s[2][2]);
 assert.deepEqual(input,before);
});

test('accepted power uses each transformed input column for asymmetric S',()=>{
 const input=report(['p','n'],[50,50],[[{real:.1,imaginary:0},{real:.2,imaginary:0}],[{real:.3,imaginary:0},{real:.4,imaginary:0}]]);
 const out=boardFoilMixedMode(input,['p','n'],[{positive:'p',negative:'n',differential:'d',common:'c'}]);
 const transformed=out.parameters[0].parameters;
 assert.ok(Math.abs(transformed[0][0].real)<1e-12&&Math.abs(transformed[0][1].real+.2)<1e-12);
 assert.ok(Math.abs(transformed[1][0].real+.1)<1e-12&&Math.abs(transformed[1][1].real-.5)<1e-12);
 assert.ok(Math.abs(out.parameters[0].acceptedPowerW[0]-24.75)<1e-12);
 assert.ok(Math.abs(out.parameters[0].acceptedPowerW[1]-4.4375)<1e-12);
});

test('rejects materially negative accepted power for non-passive samples',()=>{
 const input=report(['p','n'],[50,50],[[{real:1.2,imaginary:0},{real:0,imaginary:0}],[{real:0,imaginary:0},{real:1.2,imaginary:0}]]);
 assert.throws(()=>boardFoilMixedMode(input,['p','n'],[{positive:'p',negative:'n',differential:'d',common:'c'}]),/non-passive transformed model/);
});

test('rejects unequal pair references and reused physical ports',()=>{
 const input=report(['p','n'],[50,75],[[{real:0,imaginary:0},{real:0,imaginary:0}],[{real:0,imaginary:0},{real:0,imaginary:0}]]);
 assert.throws(()=>boardFoilMixedMode(input,['p','n'],[{positive:'p',negative:'n',differential:'d',common:'c'}]),/equal positive finite/);
 assert.throws(()=>boardFoilMixedMode(report(['p','n','x'],[50,50,50],[[{real:0,imaginary:0},{real:0,imaginary:0},{real:0,imaginary:0}],[{real:0,imaginary:0},{real:0,imaginary:0},{real:0,imaginary:0}],[{real:0,imaginary:0},{real:0,imaginary:0},{real:0,imaginary:0}]]),['p','n','x'],[
  {positive:'p',negative:'n',differential:'d1',common:'c1'},{positive:'x',negative:'n',differential:'d2',common:'c2'}]),/disjoint physical ports/);
});
