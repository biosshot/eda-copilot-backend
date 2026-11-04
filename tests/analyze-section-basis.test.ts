import assert from 'node:assert/strict';
import test from 'node:test';
import {createProblemExecutor} from '../src/analyze/execute-problem.ts';
import {
 channelTopology,sectionDescriptor,sectionMagneticBlock,solveSegmentedChannel,
 type ChannelBlock,type ChannelCut,type SectionEnds,type SegmentedChannel,
} from '../src/analyze/segmented-channel.ts';

const ids=['signal','upper-return','lower-return'];
const frequencies=[1e5,1e6];
const cut=(positive:string,negative:string):ChannelCut=>({positive,negative});
const terminals=(side:string)=>Object.fromEntries(ids.map(id=>[id,side+'-'+id]));
const left=terminals('left'),middle=terminals('middle'),right=terminals('right');
const leftPort=cut(left.signal,left['upper-return']),rightPort=cut(right.signal,right['lower-return']);
const ports=[{name:'input',cut:leftPort},{name:'output',cut:rightPort}];

function polygon(order:string[],gauge:string){
 return {kind:'polygon-section',lengthMm:2,geometry:{conductors:order.map(id=>{
  const z=ids.indexOf(id)*.0002;
  return {id,role:id===gauge?'reference':'signal',contour:{outerM:[[0,z],[.001,z],[.001,z+.0001],[0,z+.0001]]}};
 })}};
}

// Pair capacitances in physical coordinates and diagonal conductor R/L.
// Restrict C to a gauge slice and lift loop currents through e_i-e_g for R/L.
function coefficients(order:string[],gauge:string){
 const physicalC=[[7,-3,-4],[-3,5,-2],[-4,-2,6]];
 const independent=order.filter(id=>id!==gauge).map(id=>ids.indexOf(id));
 const resistance=[2,3,5],inductance=[3,5,7],g=ids.indexOf(gauge);
 const loop=(physical:number[],scale:number)=>independent.map(i=>independent.map(j=>scale*(physical[g]+(i===j?physical[i]:0))));
 return {
  parameters:{capacitanceFPerM:independent.map(i=>independent.map(j=>physicalC[i][j]*1e-11)),inductanceHPerM:loop(inductance,1e-8),modes:[{delaySPerM:5e-9}]},
  dc:{resistanceOhmPerM:loop(resistance,1),internalInductanceHPerM:loop(inductance,1e-9)},
 };
}

test('physical section port incidence is independent of gauge and permits a return-to-return port',()=>{
 const order=[ids[1],ids[2],ids[0]],line=polygon(order,ids[2]),{parameters,dc}=coefficients(order,ids[2]);
 const cuts=[rightPort,cut(left['lower-return'],left['upper-return']),leftPort];
 const data=sectionDescriptor(parameters,line,frequencies,dc,{ends:[left,right],cuts});
 assert.deepEqual(data.portIncidence,[[4,0,1],[5,0,-1],[2,1,1],[0,1,-1],[1,2,1],[0,2,-1]]);
 assert.equal(data.potentialNodes.length,6);
 assert.equal(data.currentDofs.length,2);
 assert.throws(()=>sectionDescriptor(parameters,line,frequencies,dc,{ends:[left,right],cuts:[cut(left.signal,right.signal)]}),/same end/);
 assert.throws(()=>sectionDescriptor(parameters,line,frequencies,dc,{ends:[left,{...right,signal:left.signal}],cuts}),/distinct physical terminal/);
});

test('section magnetic currents use the same independent return basis as the RLC descriptor',()=>{
 const line=polygon([ids[2],ids[0],ids[1]],ids[1]),{parameters,dc}=coefficients([ids[2],ids[0],ids[1]],ids[1]);
 const block=sectionMagneticBlock('section',line,parameters,frequencies,{originMm:[0,0],direction:[1,0]});
 const data=sectionDescriptor(parameters,line,frequencies,dc,{ends:[left,right],cuts:[leftPort,rightPort]});
 assert.equal(block.dofs,data.currentDofs.length);
 for(const [row,conductor] of [ids[2],ids[0],ids[1]].entries()){
  const z=ids.indexOf(conductor)*.0002;
  const cells=block.cells.filter(c=>c.verticesM.every((p:number[])=>p[2]>=z&&p[2]<=z+.0001));
  assert.equal(cells.length,6);
  const expected=row===2?[-1,-1]:[row===0?1:0,row===1?1:0];
  for(const cell of cells)for(const [j,density] of cell.currentsPerAmpM2.entries()){
   assert.ok(Math.abs(density[0]*1e-7-expected[j])<1e-14);
   assert.deepEqual(density.slice(1).map(Math.abs),[0,0]);
  }
  for(const [j,amp] of expected.entries())assert.equal(data.incidence.find(([i,k])=>i===row&&k===j)?.[2]??0,amp);
 }
});

test('P6 welds opposite physical contact polarities without merging the two returns',()=>{
 const join=[cut(middle.signal,middle['upper-return']),cut(middle['lower-return'],middle['upper-return'])];
 const reverse=(c:ChannelCut)=>cut(c.negative,c.positive);
 const solve=(swap:boolean)=>{
  const descriptors=[
   {id:'a',order:ids,gauge:ids[2],ends:[left,middle] as SectionEnds,cuts:[leftPort,...join]},
   {id:'b',order:swap?[ids[1],ids[2],ids[0]]:ids,gauge:swap?ids[0]:ids[2],ends:[middle,right] as SectionEnds,cuts:[...join.map(c=>swap?reverse(c):c),rightPort]},
  ];
  const blocks:ChannelBlock[]=descriptors.map(({id,order,gauge,ends,cuts})=>({id,kind:'section',line:polygon(order,gauge),sectionEnds:ends,cuts,ownership:[],bandHz:[1e5,1e6]}));
  const topology=channelTopology({blocks,ports,coverage:[]} satisfies SegmentedChannel);
  if(swap){
   assert.ok(topology.connectionContacts.includes('positive-negative'));
   assert.ok(topology.connectionContacts.includes('negative-positive'));
  }
  const instances=descriptors.map(({id,order,gauge,ends,cuts})=>{
   const {parameters,dc}=coefficients(order,gauge),ports=cuts.map((_,i)=>'port-'+i);
   return {id,ports,model:{id,ports,kind:'network',data:sectionDescriptor(parameters,polygon(order,gauge),frequencies,dc,{ends,cuts})}};
  });
  const result=createProblemExecutor(true)({kind:'frequency-network',request:{instances,...topology,frequencies,representation:'S',referenceOhm:[50,50]}});
  assert.equal(result.status,'complete',JSON.stringify(result.diagnostics));
  const physical=result.physicalRecovery as {potentials:{instanceId:string;node:string;index:number}[]};
  const index=(instanceId:string,node:string)=>physical.potentials.find(p=>p.instanceId===instanceId&&p.node===node)?.index;
  assert.equal(index('a','4'),index('b',swap?'0':'1')); // upper return
  assert.equal(index('a','5'),index('b',swap?'1':'2')); // lower return
  assert.notEqual(index('a','4'),index('a','5'));
  return result.parameters!;
 };
 const reference=solve(false),alternate=solve(true);
 for(const [f,matrix] of reference.entries())for(const [i,row] of matrix.entries())for(const [j,x] of row.entries()){
  const y=alternate[f][i][j];
  assert.ok(Math.hypot(x.real-y.real,x.imaginary-y.imaginary)<1e-10);
 }
});

test('native polygon mesh and three physical ports meet gauge sensitivity for a bounded vacuum coupon',t=>{
 const physicalPorts=[...ports,{name:'return-transfer',cut:cut(left['upper-return'],left['lower-return'])}];
 const solve=(gauge:string)=>{
  const fixture=polygon(ids,gauge);
  const domain={outerM:[[-.0006,-.0004],[.0016,-.0004],[.0016,.0009],[-.0006,.0009]]};
  const line={...fixture,geometry:{...fixture.geometry,
   domain,dielectrics:[{id:'vacuum',relativePermittivity:1,contour:domain}],
   maxTriangleAreaM2:2e-9,maxNodes:100000,maxSeconds:30,maxMemoryMib:512,
  }};
  const block:ChannelBlock={id:'polygon',kind:'section',line,mesh:{maxRefinements:3},sectionEnds:[left,right],cuts:physicalPorts.map(p=>p.cut),ownership:[],bandHz:[1e5,1e6]};
  const channel=solveSegmentedChannel({blocks:[block],ports:physicalPorts,coverage:[]},frequencies,{maxSeconds:30,maxMemoryMiB:512});
  const report=channel.evidence[0] as {parameters:{capacitanceFPerM:number[][];modes:{delaySPerM:number}[]};sectionRealization:{conductorOrder:string[];voltageGaugeConductor:string}};
  assert.deepEqual(report.sectionRealization.conductorOrder,ids.filter(id=>id!==gauge));
  assert.equal(report.sectionRealization.voltageGaugeConductor,gauge);
  assert.equal(report.parameters.modes.length,2);
  assert.deepEqual(channel.result.ports?.map(p=>p.name),physicalPorts.map(p=>p.name));
  for(const mode of report.parameters.modes)assert.ok(Math.abs(mode.delaySPerM*299792458-1)<1e-8);
  const independent=report.sectionRealization.conductorOrder.map(id=>ids.indexOf(id)),g=ids.indexOf(gauge);
  const physicalC=ids.map(()=>ids.map(()=>0));
  for(const [i,a] of independent.entries())for(const [j,b] of independent.entries()){
   const c=report.parameters.capacitanceFPerM[i][j];
   physicalC[a][b]+=c;physicalC[a][g]-=c;physicalC[g][b]-=c;physicalC[g][g]+=c;
  }
  return {physicalC,waves:channel.result.parameters!};
 };
 const upper=solve(ids[1]),lower=solve(ids[2]);
 const floor=1e-3*Math.max(...lower.physicalC.map((row,i)=>row[i]));
 const cSensitivity=Math.max(...upper.physicalC.flatMap((row,i)=>row.map((c,j)=>Math.abs(c-lower.physicalC[i][j])/Math.max(floor,Math.abs(lower.physicalC[i][j])))));
 let maxAbsolute=0,maxNormalized=0;
 for(const [f,matrix] of upper.waves.entries())for(const [i,row] of matrix.entries())for(const [j,x] of row.entries()){
  const y=lower.waves[f][i][j],delta=Math.hypot(x.real-y.real,x.imaginary-y.imaginary);
  maxAbsolute=Math.max(maxAbsolute,delta);
  maxNormalized=Math.max(maxNormalized,delta/Math.max(.01,Math.hypot(y.real,y.imaginary)));
 }
 t.diagnostic(JSON.stringify({maxAbsoluteWaveChange:maxAbsolute,maxNormalizedWaveChange:maxNormalized,physicalCapacitanceSensitivity:cSensitivity}));
 assert.ok(maxNormalized<=.02);
 assert.ok(cSensitivity<=.02);
});
