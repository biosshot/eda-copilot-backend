import test from 'node:test';
import assert from 'node:assert/strict';
import { createProblemExecutor } from '../src/analyze/execute-problem.ts';
import { sectionTransitionFrequencyRequest } from '../src/analyze/segmented-channel.ts';

type Complex = readonly [number,number];
const add=(a:Complex,b:Complex):Complex=>[a[0]+b[0],a[1]+b[1]];
const multiply=(a:Complex,b:Complex):Complex=>[a[0]*b[0]-a[1]*b[1],a[0]*b[1]+a[1]*b[0]];
const scale=(a:Complex,v:number):Complex=>[a[0]*v,a[1]*v];
const divide=(a:Complex,b:Complex):Complex=>scale(multiply(a,[b[0],-b[1]]),1/(b[0]*b[0]+b[1]*b[1]));

test('isolated finite-conductivity section matches constant RLGC propagation at its exact frequency',()=>{
 const frequencyHz=1e6,lengthMm=5,r=400,l=4e-7,c=1e-10;
 const contour={outerM:[[0,0],[.001,0],[0,.001]] as [number,number][]};
 const input:Parameters<typeof sectionTransitionFrequencyRequest>[0]={
  id:'one-section',
  line:{kind:'polygon-section',lengthMm,geometry:{conductors:[
   {id:'return',role:'reference',contour}, {id:'signal',role:'signal',contour},
  ]}},
  electric:{reference:'return',conductorOrder:['signal'],capacitanceFPerM:[[c]],modes:[{delaySPerM:Math.sqrt(l*c)}]},
  sample:{frequencyHz,conductorOrder:['return','signal'],signalIds:['signal'],referenceId:'return',
   resistanceOhmPerM:[[r]],totalInductanceHPerM:[[l]]},
  terminals:{ends:[{signal:'near-s',return:'near-r'},{signal:'far-s',return:'far-r'}],
   cuts:[{positive:'near-s',negative:'near-r'},{positive:'far-r',negative:'far-s'}]},
 };
 const request=sectionTransitionFrequencyRequest(input);
 const model=request.instances[0].model;
 const data=model.data as {portTerminals:unknown;currentDofs:string[]};
 assert.deepEqual(data.portTerminals,[
  {positive:{node:'0'},negative:{node:'1'}},
  {positive:{node:'3'},negative:{node:'2'}},
 ]);
 assert.deepEqual(model.analyses,['networkParameters']);
 assert.deepEqual(request.frequencies,[frequencyHz]);
 assert.ok(Object.isFrozen(model.data));
 assert.deepEqual(request.connections,[]);
 assert.equal(request.instanceInductanceH,undefined);
 assert.equal(data.currentDofs.length,1);
 const refined=sectionTransitionFrequencyRequest({...input,line:{...input.line,lengthMm:100}});
 assert.ok((refined.instances[0].model.data as typeof data).currentDofs.length>1);
 assert.throws(()=>sectionTransitionFrequencyRequest({...input,line:{...input.line,lengthMm:100000}}),/cell budget/);
 const canonical=sectionTransitionFrequencyRequest({...input,sample:{...input.sample,conductorOrder:['signal','return']}});
 assert.deepEqual(canonical.instances[0].model.data,model.data);
 const lossOrder=(resource:typeof model)=>(resource.validity as {domain:{conductorLossOrder:string[]}}).domain.conductorLossOrder;
 assert.deepEqual(lossOrder(model),['return','signal']);
 assert.deepEqual(lossOrder(canonical.instances[0].model),['signal','return']);
 for(const conductorOrder of [['signal'],['signal','signal'],['signal','unknown']]){
  assert.throws(()=>sectionTransitionFrequencyRequest({...input,sample:{...input.sample,conductorOrder}}),/conductor IDs.*bijection/);
 }
 const coupled={...input,line:{...input.line,geometry:{conductors:[...input.line.geometry.conductors,
   {id:'victim',role:'signal' as const,contour}]}},
  electric:{...input.electric,conductorOrder:['signal','victim'],capacitanceFPerM:[[c,0],[0,c]],
   modes:[...input.electric.modes,...input.electric.modes]},
  sample:{...input.sample,conductorOrder:['signal','victim','return'],signalIds:['signal','victim'],
   resistanceOhmPerM:[[r,0],[0,r]],totalInductanceHPerM:[[l,0],[0,l]]},
  terminals:{...input.terminals,ends:[
   {...input.terminals.ends[0],victim:'near-v'},{...input.terminals.ends[1],victim:'far-v'},
  ] as const},
 };
 assert.doesNotThrow(()=>sectionTransitionFrequencyRequest(coupled));
 assert.throws(()=>sectionTransitionFrequencyRequest({...coupled,sample:{...coupled.sample,signalIds:['victim','signal']}}),/conductor.*order/);
 assert.throws(()=>sectionTransitionFrequencyRequest({...input,directSelf:true as never}),/direct self/);

 const execute=createProblemExecutor();
 const result=execute({kind:'frequency-network',request});
 assert.equal(result.status,'complete',JSON.stringify(result.diagnostics));
 assert.deepEqual(result.bandHz,[frequencyHz,frequencyHz]);
 const rejected=execute({kind:'frequency-network',request:{...request,frequencies:[frequencyHz+1]}});
 assert.equal(rejected.status,'rejected');
 assert.ok(rejected.diagnostics.some(d=>d.code==='COUPLED_BAND'));

 // Exact distributed-line ABCD evaluated with entire functions of (γℓ)^2.
 // Here |γℓ| < .01, so the omitted terms are below the test tolerance.
 const length=lengthMm*1e-3,w=2*Math.PI*frequencyHz;
 const z:Complex=[r*length,w*l*length],y:Complex=[0,w*c*length];
 const q=multiply(z,y),q2=multiply(q,q);
 const a=add(add([1,0],scale(q,.5)),scale(q2,1/24));
 const sinhc=add(add([1,0],scale(q,1/6)),scale(q2,1/120));
 const b=multiply(z,sinhc),cross=multiply(y,sinhc),reference=50;
 const denominator=add(scale(a,2),add(scale(b,1/reference),scale(cross,reference)));
 const reflection=divide(add(scale(b,1/reference),scale(cross,-reference)),denominator);
 // The far port is oriented from the return conductor to the signal.
 const transfer=divide([-2,0],denominator);
 for(const [row,col,expected] of [[0,0,reflection],[1,1,reflection],[0,1,transfer],[1,0,transfer]] as const){
  const actual=result.parameters![0][row][col];
  assert.ok(Math.hypot(actual.real-expected[0],actual.imaginary-expected[1])<1e-7,
   JSON.stringify({row,col,actual,expected}));
 }
});
