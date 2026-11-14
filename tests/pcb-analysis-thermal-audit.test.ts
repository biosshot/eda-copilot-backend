import test from 'node:test';
import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {auditThermal,auditThermalRefinement,auditThermalComparisonTelemetry,type ThermalExpectations} from './helpers/pcb-analysis-thermal-audit.ts';
// Minimal independent audit-contract fixture; never numerical solver evidence.
const board={name:'audit-contract'};const identity=createHash('sha256').update(JSON.stringify(board)).digest('hex');
const expected:ThermalExpectations={fixture:'audit-contract',component:'U1',powerW:1,cases:['steady-coarse','steady-fine','time','time-refined'],timeS:[0,15,30,45,60],conditional:true,heatsink:false,maxSpatialRelativeChange:.02,maxTemporalRelativeChange:.001};
function fixture():any{
 const calculations=expected.cases.map(name=>{
  const time=name.startsWith('time'),axis=time?expected.timeS:[60],values=time?[25,30,35,40,45]:[50];const fieldNodes=['TOP','core','BOTTOM'].map((layer,i)=>({id:`pcb:${layer}:0`,layer,pointMm:[0,0],capacityJPerK:1,supportAreaMm2:1}));const nodeIds=[...fieldNodes.map(f=>f.id),'U1:junction','U1:case','pcb:ambient'];const linkIds=['U1:jc','contact:U1:0:0'];const zeros=axis.map(()=>0);const energy=axis.map(t=>Math.max(0,t-5));
  const inputs={name,domain:time?'time':'steady',heat:[{target:{kind:'component',designator:'U1'},node:'junction',powerW:{kind:'step',from:0,to:1,atS:5,riseS:0}}]};const material={source:'audit contract',state:'constant',thermalConductivityWPerMK:1,densityKgM3:1,specificHeatJPerKgK:1};
  const solution={id:"thermal-"+name,status:'complete',boardIdentity:identity,axis,system:{nodeIds,linkIds},temperaturesC:Object.fromEntries(nodeIds.map(id=>[id,[...values]])),heatFlowW:Object.fromEntries(linkIds.map(id=>[id,axis.map(()=>1)])),inputPowerW:axis.map(t=>t<5?0:1),outputPowerW:zeros,energyBalance:zeros,storedEnergyJ:[...energy],inputEnergyJ:[...energy],outputEnergyJ:zeros,memoryPlan:{requestBytes:1000,totalPlannedPeakBytes:900,physicalSystemBytes:300,transportReportReserveBytes:300,preconditionerPeakBytes:300},quality:{linearResidual:0,linearRoundoffBound:0,linearStoppingAllowance:1e-12,normalizedEnergyBalance:0},spatial:{boardIdentity:identity,conditional:true,units:{coordinates:'mm',temperature:'degC',power:'W',capacity:'J/K'},mesh:{fieldNodes,maxEdgeMm:name==='steady-coarse'?4:2,localRefinement:[{target:{kind:'component',designator:'U1'},minimumNominalPadWidthMm:.3,unsupportedWidthShapes:0,derivedSpacingMm:.15,haloMm:.15,candidatePoints:9,selectedHaloCandidates:3,insertedPoints:1,widthScope:'Authored primitive width'}]},contacts:[{node:'U1:case',target:{kind:'component',designator:'U1'},resistanceKPerW:8,weights:[{node:'pcb:TOP:0',weight:1}]}],layers:[{kind:'copper',material,background:material},{kind:'dielectric',material},{kind:'copper',material,background:material}]}};
  return{kind:'thermal',phase:'completed',result:{status:'complete',inputs,solution}};
 });
 // Give the contract fixture actual interpolation cells and independently
 // balanced signed paths; this remains a synthetic audit fixture, not a solve.
 for(const c of calculations){const s:any=c.result.solution;const layers=['TOP','core','BOTTOM'];s.spatial.mesh.fieldNodes=layers.flatMap(layer=>[[0,0],[1,0],[0,1]].map((pointMm,i)=>({id:`pcb:${layer}:${i}`,layer,pointMm,capacityJPerK:1,supportAreaMm2:1/6})));s.spatial.mesh.pointsMm=[[0,0],[1,0],[0,1]];s.spatial.mesh.triangles=[[0,1,2]];s.spatial.geometry={physicalDomain:[{outer:[[0,0],[1,0],[0,1]],holes:[]}],booleanErrorBoundMm:1e-8};s.spatial.layers.forEach((l:any,i:number)=>l.name=layers[i]);s.spatial.contacts[0].id='U1:0';s.system.nodeIds=[...s.spatial.mesh.fieldNodes.map((f:any)=>f.id),'U1:junction','U1:case','pcb:ambient'];s.temperaturesC=Object.fromEntries(s.system.nodeIds.map((id:string)=>[id,[...s.temperaturesC['U1:junction']]]));s.system.linkIds.push('surface:0:0');s.heatFlowW['surface:0:0']=s.axis.map(()=>1);s.quality.originalNodalResidual=0;s.request={network:{nodes:[{id:'U1:junction'},{id:'U1:case'}],links:[{id:'U1:jc',from:'U1:junction',to:'U1:case'}],sources:[{node:'U1:junction',powerW:c.result.inputs.heat[0].powerW}]}};if(c.result.inputs.domain==='steady')s.outputPowerW=[1];}
 const plots=['degC','degC','W','J','degC'].map((unit,i)=>({data:{kind:i===4?'map':'series',unit,validity:{status:'conditional'},values:i===4?Array(9).fill(45):i===2?[1,1,1,1,1]:i===3?[0,0,0,0,0]:[25,30,35,40,45],...(i===4?{data:{boardIdentity:identity,solutionId:'thermal-time',fieldReference:'spatial.mesh.fieldNodes'}}:{axis:{name:'timeS',unit:'s',values:expected.timeS}})}}));
 return{status:'complete',reports:[{calculations,plots,checks:[{status:'fail'},{status:'pass'},{status:'pass'}]}]};
}
test('thermal audit accepts full source-bound contract and rejects deleted/truncated/duplicate/unit/identity/zero substitutions',()=>{
 assert.equal(auditThermal(fixture(),board,expected).status,'pass');
 const mutations:((r:any)=>void)[]=[
  r=>r.reports[0].calculations.pop(),r=>r.reports[0].calculations.push(r.reports[0].calculations[0]),
  r=>delete r.reports[0].calculations[2].result.solution.temperaturesC['U1:case'],
  r=>r.reports[0].calculations[2].result.solution.temperaturesC['U1:junction'].pop(),
  r=>r.reports[0].calculations[2].result.solution.temperaturesC['U1:junction'].push(45),
  r=>r.reports[0].calculations[2].result.solution.temperaturesC['U1:junction'][2]=null,
  r=>delete r.reports[0].calculations[2].result.solution.heatFlowW['contact:U1:0:0'],
  r=>r.reports[0].calculations[2].result.solution.system.linkIds.push('U1:jc'),
  r=>r.reports[0].plots[0].data.axis.values=[0,15,30,60,60],r=>r.reports[0].plots[0].data.unit='K',
  r=>r.reports[0].plots[0].data.validity.status='valid',r=>r.reports[0].plots[4].data.data.boardIdentity='foreign',
  r=>r.reports[0].calculations[2].result.inputs.heat=[],r=>r.reports[0].calculations[2].result.inputs.heat[0].powerW.to=0,
  r=>r.reports[0].calculations[2].result.solution.inputPowerW[2]=0,r=>r.reports[0].calculations[2].result.solution.spatial.contacts=[],
  r=>r.reports[0].calculations[2].result.solution.spatial.contacts[0].weights[0].node='missing',
  r=>r.reports[0].calculations[2].result.solution.spatial.conditional=false,
  r=>r.reports[0].calculations[2].result.solution.spatial.units.power='J',
  r=>r.reports[0].calculations[2].result.solution.inputEnergyJ[3]=0,
  r=>r.reports[0].plots[4].data.values[0]=0,
  r=>r.reports[0].plots[2].data.values[2]=0,
  r=>r.reports[0].calculations[2].result.solution.memoryPlan.totalPlannedPeakBytes=1001,
  r=>r.reports[0].calculations[2].result.solution.quality.linearStoppingAllowance=.001,
  r=>r.reports[0].calculations[3].result.solution.temperaturesC['pcb:TOP:0'][4]=400,
  r=>r.reports[0].calculations[0].result.solution.temperaturesC['pcb:BOTTOM:0'][0]=400,
  r=>delete r.reports[0].calculations[2].result.solution.spatial.mesh.localRefinement,
  r=>r.reports[0].calculations[2].result.solution.spatial.mesh.localRefinement[0].derivedSpacingMm=.2,
  r=>r.reports[0].calculations[2].result.solution.spatial.mesh.localRefinement[0].minimumNominalPadWidthMm=null,
  r=>r.reports[0].calculations[2].result.solution.spatial.mesh.localRefinement[0].insertedPoints=10,
  r=>r.reports[0].calculations[0].result.solution.spatial.mesh.maxEdgeMm=2,
  r=>r.reports[0].calculations[2].result.inputs.heat[0].powerW.atS=7,
  r=>r.reports[0].plots[4].data.data.solutionId='foreign',
  r=>r.reports[0].calculations[0].result.solution.quality.originalNodalResidual=.001,
  r=>r.reports[0].calculations[0].result.solution.heatFlowW['surface:0:0'][0]=0,
  r=>r.reports[0].calculations[0].result.solution.request.network.links[0].from='missing',
  r=>delete r.reports[0].calculations[0].result.solution.spatial.geometry.physicalDomain,
  r=>r.reports[0].calculations[1].result.solution.spatial.geometry.physicalDomain[0].holes=[[[.2,.2],[.3,.2],[.2,.3]]],
 ];for(const [index,mutate]of mutations.entries()){const r=fixture();mutate(r);assert.throws(()=>auditThermal(r,board,expected),`Mutation ${index} must fail`);}
});

// Synthetic provenance contracts only; actual adaptive evidence is native.
function adaptiveContract(){const s=fixture().reports[0].calculations[1].result.solution;const p={maxPasses:2,relativeTolerance:.02,validationMaxEdgeMm:1};s.spatial.mesh.maxEdgeMm=1;s.spatial.mesh.nested={nestedProof:{checkedFineCells:1,maximumFineToParentDiameterRatio:1,maximumParentContainmentDeviationMm:0}};s.coordinateFrame={originC:25,nodeOrder:'system.nodeIds',temperaturesK:[s.system.nodeIds.map((id:string)=>s.temperaturesC[id][0]-25)]};s.refinement={policy:p,converged:true,finalRole:'independent-control',totalPhysicalAnalyses:3,totalElapsedMs:3,maximumPlannedPeakBytes:900,history:['seed','directional','independent-control'].map((role,pass)=>({pass,role,elapsedMs:1,cumulativeElapsedMs:pass+1,retainedPreviousFieldMeshBytes:pass*10,preparationPlannedPeakBytes:800,comparisonPlannedPeakBytes:800,summary:{quality:structuredClone(s.quality),memoryPlan:structuredClone(s.memoryPlan)},comparison:pass?{timeSamples:1,observationAxisReference:'axis',sampledPoints:10,missingPhysicalOrBoundarySamples:0,absoluteAllowanceK:1e-9,maximumGateUtilization:.5,converged:true,compactConverged:true}:null}))};Object.assign(s.spatial.mesh.nested,{independentGlobalVertices:2,addedGlobalVertices:1,matchedExistingGlobalVertices:1,preservedCandidateVertices:s.spatial.mesh.pointsMm.length-1,preservedCandidateEdges:3});return{s,mesh:{refinement:p}};}
test('thermal refinement audit requires independent final map, original guards and consistent complete coordinates',()=>{
 const contract=adaptiveContract;
 const covered=()=>{const c=contract();Object.assign(c.s.spatial.mesh.nested,{independentGlobalVertices:2,addedGlobalVertices:1,matchedExistingGlobalVertices:1,preservedCandidateVertices:c.s.spatial.mesh.pointsMm.length-1,preservedCandidateEdges:3});return c;};
 const base=covered();assert.equal(auditThermalRefinement(base.s,base.mesh).converged,true);
 const mutations:((s:any)=>void)[]=[s=>s.refinement.converged=false,s=>s.refinement.finalRole='candidate',s=>s.refinement.history.at(-1).comparison.maximumGateUtilization=2,s=>s.refinement.history[0].summary.quality.originalNodalResidual=.1,s=>s.refinement.maximumPlannedPeakBytes=1001,s=>s.refinement.history.shift(),s=>s.refinement.history[1].comparison.timeSamples=0,s=>s.refinement.history[2].comparison.missingPhysicalOrBoundarySamples=1,s=>delete s.coordinateFrame,s=>s.coordinateFrame.temperaturesK[0].pop(),s=>s.coordinateFrame.temperaturesK[0][0]=NaN,s=>s.coordinateFrame.temperaturesK[0][0]+=1,s=>s.spatial.mesh.nested.nestedProof.checkedFineCells=0];
 mutations.push(s=>s.refinement.history.at(-2).comparison.converged=false,s=>s.spatial.mesh.nested.addedGlobalVertices++,s=>delete s.spatial.mesh.nested.independentGlobalVertices);
 for(const [i,mutate]of mutations.entries()){const c=covered();mutate(c.s);assert.throws(()=>auditThermalRefinement(c.s,c.mesh),`Adaptive mutation${i} must fail`);}
});

test('single-case public policy audit retains final arrays and rejects false scalar/map provenance',()=>{
 const make=()=>{const r=fixture(),c=r.reports[0].calculations[1],{s,mesh}=adaptiveContract();c.result.solution=s;c.result.inputs.mesh=mesh;
  const old=s.spatial.mesh.localRefinement[0];s.spatial.mesh.nested.seedRefinement=[{...old,healingLengthMm:.3,haloMm:1.2,graded:true}];
  r.reports[0].calculations=[c];const vals=[50,50,1,0];r.reports[0].plots=r.reports[0].plots.map((p:any,i:number)=>i===4?{data:{...p.data,data:{...p.data.data,solutionId:s.id},values:Array(9).fill(50)}}:{data:{kind:'scalar',unit:i<2?'degC':'W',value:vals[i],validity:{status:'conditional'}}});return r;};
 const e:ThermalExpectations={...expected,mode:'single-case',cases:['steady-fine'],timeS:[60]};assert.equal(auditThermal(make(),board,e).status,'pass');
 for(const mutate of [(r:any)=>r.reports[0].plots[0].data.value=0,(r:any)=>r.reports[0].plots[4].data.data.solutionId='foreign',(r:any)=>r.reports[0].calculations[0].result.solution.refinement.converged=false,(r:any)=>r.reports[0].plots[3].data.unit='J']){const r=make();mutate(r);assert.throws(()=>auditThermal(r,board,e));}
});

test('steady-pair audit checks both original balances, complete fine plots and physical spatial gate',()=>{
 const make=()=>{const r=fixture();r.reports[0].calculations=r.reports[0].calculations.slice(0,2);r.reports[0].plots=r.reports[0].plots.map((p:any,i:number)=>i===4?{data:{...p.data,data:{...p.data.data,solutionId:'thermal-steady-fine'},values:Array(9).fill(50)}}:{data:{kind:'scalar',unit:i<2?'degC':'W',value:[50,50,1,0][i],validity:{status:'conditional'}}});return r;};
 const e:ThermalExpectations={...expected,mode:'steady-pair',cases:['steady-coarse','steady-fine'],timeS:[60]};assert.equal(auditThermal(make(),board,e).status,'pass');
 for(const mutate of [(r:any)=>r.reports[0].calculations[1].result.solution.spatial.mesh.maxEdgeMm=4,(r:any)=>r.reports[0].plots[4].data.values.pop(),(r:any)=>r.reports[0].calculations[0].result.solution.quality.originalNodalResidual=1e-4]){const r=make();mutate(r);assert.throws(()=>auditThermal(r,board,e));}
});

test('required native comparison telemetry rejects missing/misnormalized witnesses and unproven boundary displacement',()=>{
 const {s}=adaptiveContract();s.spatial.geometry.outline={outer:[[0,0],[1,0],[0,1]]};s.spatial.geometry.physicalDomain[0].outer=[[1e-9,0],[1+1e-9,0],[1e-9,1]];
 const witness={pointMm:[.25,.25],layer:'TOP',timeS:60,coarseTemperatureC:49,fineTemperatureC:50,ambientC:50,denominatorK:100,differenceK:1,relativeChange:.01,gateAllowanceK:2+1e-9,temperatureUnit:'degC',differenceUnit:'K'};
 const base={maximumDifferenceK:1,maximumRelativeChange:.01,absoluteAllowanceK:1e-9,absoluteWitness:witness,relativeWitness:structuredClone(witness),missingCoverage:[],boundaryLocalization:{count:1,maximumDisplacementMm:1e-9,sourceIdentityMatches:true,records:[{mesh:'coarse',correspondence:{pointMm:[.25+1e-9,0],localizedPointMm:[.25,0],fraction:.25,polygon:0,edge:0,sourceEdge:[[0,0],[1,0]],transportedEdge:s.spatial.geometry.physicalDomain[0].outer.slice(0,2),displacementMm:1e-9,limitMm:1.1e-9,representationLimitMm:1e-8,roundoffMm:1e-14}}]}};
 assert.equal(auditThermalComparisonTelemetry(base,s).witnesses,2);
 const mutations=[(c:any)=>delete c.absoluteWitness,(c:any)=>c.relativeWitness.timeS=0,(c:any)=>c.relativeWitness.denominatorK=0,(c:any)=>c.relativeWitness.coarseTemperatureC=0,(c:any)=>c.boundaryLocalization.sourceIdentityMatches=false,(c:any)=>c.boundaryLocalization.records[0].correspondence.displacementMm=1,(c:any)=>c.boundaryLocalization.records[0].correspondence.sourceEdge=[[.1,0],[1.1,0]],(c:any)=>c.missingCoverage.push({pointMm:[0,0]})];
 for(const mutate of mutations){const c=structuredClone(base);mutate(c);assert.throws(()=>auditThermalComparisonTelemetry(c,s));}
});
