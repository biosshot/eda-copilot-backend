import test from 'node:test';
import assert from 'node:assert/strict';
import { boardFoilPortObservations } from '../src/analyze/board-foil-port-observations.ts';
import type { AutoFoilPlan } from '../src/analyze/auto-foil-plan.ts';
import type { BoardFoilReport } from '../src/analyze/types.ts';

function fixture(refs:number[],s:{real:number;imaginary:number}[][],contacts?:Array<[string,string]>,losses?:number[],packageLosses?:number[][]){
 const n=refs.length, terminals=Array.from({length:n*2},(_,i)=>({id:'t'+i,sourcePath:'pad['+(i%2===0?i/2:'shared')+']',layer:'TOP'}));
 const physicalPairs=contacts??Array.from({length:n},(_,i)=>['t'+(2*i),'t'+(2*i+1)] as [string,string]);
 const electrodePairs=Array.from({length:n},(_,i)=>['foil-terminal-'+(2*i),'foil-terminal-'+(2*i+1)] as [string,string]);
 const plan={ports:Array.from({length:n},(_,i)=>({name:'p'+i,positive:physicalPairs[i][0],negative:physicalPairs[i][1]})),terminals,electrodePairs,attachments:[],geometry:{electrodes:[]}} as unknown as AutoFoilPlan;
 const frequenciesHz=[1000],params=[s],samples=[{frequencyHz:1000,columns:Array.from({length:n},(_,i)=>({input:i,kclRelative:0,kvlRelative:0,copperLossW:losses?.[i]??0,powerBalanceErrorW:0,packages:(packageLosses?.[i]??[]).map((lossW,j)=>({id:'pkg'+j,currentA:{real:0,imaginary:0},lossW}))}))}];
 const report={status:'complete',coverage:'unverified',representation:'S',frequenciesHz,referenceOhm:refs,ports:electrodePairs,parameters:params,samples,enrichment:[],quality:{maxKclRelative:0,maxKvlRelative:0,maxColumnPowerBalanceErrorW:0},provenance:{method:'test',modelKind:'mixed',sourceMapping:[],barrelIds:[],electrodeAreasM2:{},maxCopperVolumeChange:0,prisms:0,nativeCopperTetrahedra:0,volumeTetrahedra:0,fullPotentialCount:1,fullCurrentCount:1,retainedPotentials:1,currentModes:1,plannedBytes:1,maxSeconds:1,maxMemoryMiB:1,magneticQuadratureCoverage:'unverified',spatialCoverage:'unverified',bandCoverage:'unverified',driveConvention:'1 A RMS Norton',attachments:[]}} as unknown as BoardFoilReport;
 return {plan,report};
}

test('derives one-port matched reference V/I and retains loss/power scope',()=>{
 const {plan,report}=fixture([50],[[{real:1/3,imaginary:0}]],undefined,[11.11111111111111]);
 const out=boardFoilPortObservations(plan,report),drive=out.frequencies[0].drives[0];
 assert.ok(Math.abs(drive.portObservations[0].voltageV.real-33.33333333333333)<1e-12);
 assert.ok(Math.abs(drive.contactCurrents[0].currentA.real-1/3)<1e-12);
 assert.ok(Math.abs(drive.nativeCopperLossW-11.11111111111111)<1e-12);
 assert.equal(out.spatialRecovery.status,'unverified');
});

test('supports unequal references, sums native copper and package powers, and aggregates only identical contacts',()=>{
 const s=[[{real:1/3,imaginary:0},{real:0,imaginary:0}],[{real:0,imaginary:0},{real:0,imaginary:0}]];
 const {plan,report}=fixture([50,100],s,[['t0','t1'],['t2','t1']],[10,20],[[1.11111111111111],[5]]);
 plan.terminals.find(t=>t.id==='t2')!.sourcePath='pad[shared]';plan.terminals.find(t=>t.id==='t2')!.layer='BOTTOM';
 const out=boardFoilPortObservations(plan,report),d=out.frequencies[0].drives[0];
 assert.equal(d.nativeCopperLossW,10);assert.equal(d.packageLossesW[0].lossW,1.11111111111111);assert.equal(d.totalCopperAndPackageLossW,11.11111111111111);
 assert.equal(d.contactCurrents.filter(c=>c.sourcePath==='pad[shared]').length,2,'same sourcePath on different layers remains distinct');
 const networkPower=d.portObservations.reduce((sum,p)=>sum+p.voltageV.real*p.currentA.real+p.voltageV.imaginary*p.currentA.imaginary,0);
 assert.ok(Math.abs(networkPower-d.totalCopperAndPackageLossW)<1e-12);
 assert.equal(d.portObservations.length,2);
});

test('spatial observation references the canonical sample without duplicating supports',()=>{
 const {plan,report}=fixture([50],[[{real:0,imaginary:0}]]);
 report.samples[0].columns[0].spatialRecovery={status:'recovered-discrete',physicalAccuracy:'unverified',driveConvention:'1 A RMS matched Norton',copperLossW:0,supports:[{kind:'foil',layer:'TOP',triangle:0,zM:[0,1],xyM:[[0,0],[1,0],[0,1]],volumeM3:1,currentDensityRealAPerM2:[0,0,0],currentDensityImaginaryAPerM2:[0,0,0],lossW:0}]};
 const out=boardFoilPortObservations(plan,report);
 const drive=out.frequencies[0].drives[0];
 assert.deepEqual(drive.spatialRecoveryRef,{sampleIndex:0,columnIndex:0});
 assert.equal(out.spatialRecovery.status,'recovered-discrete');
 assert.equal(report.samples[drive.spatialRecoveryRef!.sampleIndex].columns[drive.spatialRecoveryRef!.columnIndex].spatialRecovery!.supports.length,1);
 assert.equal(JSON.stringify(out).includes('currentDensityRealAPerM2'),false);
});
