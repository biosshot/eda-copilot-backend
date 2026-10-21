import test from 'node:test';
import assert from 'node:assert/strict';
import {createRequire} from 'node:module';
import {minimalPrimitive} from './helpers/native-board-problem.ts';
import {defaultSolverOptions} from '../src/pcb-layout/pcb-auto-place/utils.ts';
import {buildPlacementGraph} from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import {encodeNativeBoardPackProblem} from '../src/pcb-layout/pcb-auto-place-v2/native/encode-board-problem.ts';
import {encodeNativeBlockSolveProblem} from '../src/pcb-layout/pcb-auto-place-v2/native/encode-block-problem.ts';
import {encodeNativePostPlaceScoreProblem} from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-score.ts';
import {encodeNativePostPlaceRefineProblem} from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-refine.ts';
import {createClearanceResolver} from '../src/pcb-layout/pcb-auto-place/clearance-resolver.ts';
const addon=createRequire(import.meta.url)('../native/node-addon/index.cjs');
const endpoint=d=>({type:'component',designator:d});
function fixture(distance=2, layer='top', hints=[]) {
  const components=['A','B'].map(designator=>({designator,value:designator,block_name:designator,pins:[],search_query:'',part_uuid:null,footprint_uuid:null,
    footprint:{name:'audit',width:1,height:1,pads:[]},pcb:{role:'passive',allowedLayers:['top','bottom'],allowedRotations:[0,90,180,270]}}));
  const input={board:{coordinateSystem:'centered',outline:{type:'rect',width:100,height:100},defaultLayer:'top',allowedLayers:['top','bottom'],clearances:{component:.2,edge:.2}},
    boardHoles:[],constraintRegions:[],silkscreen:[],blocks:components.map(c=>({name:c.designator,description:'audit',component_designators:[c.designator],role:'generic'})),modules:[],components,hints,paths:[],refineGroups:[],solverOptions:{...defaultSolverOptions,ignoredRatsnestSignals:[]}};
  const placements=components.map((c,i)=>({designator:c.designator,x:i*distance,y:0,rotate:0,layer:i?layer:'top',score:0}));
  const primitives=placements.map(p=>({...minimalPrimitive(),id:p.designator,label:p.designator,sourceNodeId:`tree:block:${p.designator}`,sourceNodeIds:[`tree:block:${p.designator}`],
    locked:true,canRotate:false,allowedOrientations:[0],placements:[p],bbox:{left:p.x-.5,right:p.x+.5,top:-.5,bottom:.5},width:1,height:1,
    collisionBoxes:[{left:p.x-.5,right:p.x+.5,top:-.5,bottom:.5}],connectionPoints:[],children:[]}));
  const graph=buildPlacementGraph(input);
  const params={node:graph.root,primitives,relations:graph.relations.filter(r=>r.data?.hintIndex!==undefined),options:{grid:.5,clearance:.2,searchWidth:1,compactness:'normal',bounds:{left:-50,right:50,top:-50,bottom:50},board:input.board,
    edgeClearance:input.board.clearances.edge,ignoredNets:input.solverOptions.ignoredRatsnestSignals,componentByDesignator:new Map(components.map(c=>[c.designator,c])),blockRoleByName:new Map(),clearanceResolver:createClearanceResolver(input)}};
  return {input,placements,params,board:encodeNativeBoardPackProblem(params),block:encodeNativeBlockSolveProblem(params)};
}
function measure(distance,layer,hint) {
  const f=fixture(distance,layer,[hint]);
  const delta=(p,fn)=>fn(p)-fn({...p,relations:[]});
  const post=addon.scorePostPlace(encodeNativePostPlaceScoreProblem(f.input,f.placements));
  const refine=encodeNativePostPlaceRefineProblem(f.input,f.placements,1);refine.iterations=0;
  return {distance,layer,hint,graph:f.params.relations,native:f.board.relations,
    boardRank:addon.evaluateBoardQuality(f.board).rank,
    block:delta(f.block,p=>addon.evaluateBlockQuality(p).score),board:delta(f.board,p=>addon.evaluateBoardQuality(p).rank.score),post,
    refine: addon.refinePostPlacement(refine).scoreBefore};
}

const close=(a,b)=>assert.ok(Math.abs(a-b)<.1,`${a} != ${b}`);
const allStages=r=>{close(r.block,r.post);close(r.board,r.post);close(r.refine,r.post);};
test('away_from has a finite clearance dead zone in all stages, never attraction',()=>{
  const hint={relation:'away_from',source:endpoint('A'),target:endpoint('B'),priority:'normal'};
  const near=measure(2,'top',hint),far=measure(10,'top',hint),further=measure(20,'top',hint);
  allStages(near);allStages(far);allStages(further);assert.equal(near.post,68600);assert.equal(far.post,0);assert.equal(further.post,0);
});
test('authored clearance minimum survives graph and both native encoders',()=>{
  for(const min of [2,8,12]) allStages(measure(4,'top',{relation:'clearance',source:endpoint('A'),target:endpoint('B'),priority:'normal',min}));
  const r=measure(4,'top',{relation:'clearance',source:endpoint('A'),target:endpoint('B'),priority:'critical',min:12});
  assert.equal(r.boardRank.hardCount,1);assert.equal(r.boardRank.hardSeverity,9);
});
test('same_side depends only on layers, including group targets',()=>{
  for(const distance of [2,10,20]) for(const layer of ['top','bottom']) for(const source of [endpoint('A'),{type:'block',block_name:'A'}]) {
    const r=measure(distance,layer,{relation:'same_side',source,target:endpoint('B'),priority:'normal'});
    allStages(r);assert.equal(r.post,layer==='top'?0:1400);
  }
});
test('prefer_layer retains its layer and applies to authored groups',()=>{
  for(const layer of ['top','bottom']) for(const source of [endpoint('B'),{type:'block',block_name:'B'}]) {
    const r=measure(10,layer,{relation:'prefer_layer',source,layer:'bottom',priority:'normal'});
    allStages(r);assert.equal(r.post,layer==='bottom'?0:700);
  }
});
test('edge uses selected board edge; local Block defers until a board frame exists',()=>{
  for(const edge of ['left','right','top','bottom']) {
    const r=measure(10,'top',{relation:'edge',source:endpoint('B'),edge,orientation:'any',priority:'normal'});
    close(r.board,r.post);close(r.refine,r.post);assert.equal(r.block,0);
    assert.equal(r.native[0].condition.edge,edge);
    const f=fixture(10,'top',[r.hint]);f.block.world={bounds:f.board.fullBoardBounds,outline:f.board.boardOutline,edgeClearance:.2,obstacles:[]};
    close(addon.evaluateBlockQuality(f.block).score-addon.evaluateBlockQuality({...f.block,relations:[]}).score,r.post);
  }
});
test('clearance(group, all) excludes every member, including across rigid owners',()=>{
  const f=fixture(10,'top',[{relation:'clearance',source:{type:'block',block_name:'A'},target:'all',priority:'normal',min:12}]);
  f.input.blocks=[{...f.input.blocks[0],component_designators:['A','B']}];f.input.components[1].block_name='A';
  f.params.relations=buildPlacementGraph(f.input).relations.filter(r=>r.data?.hintIndex!==undefined);
  f.params.options.componentByDesignator=new Map(f.input.components.map(c=>[c.designator,c]));
  const b=encodeNativeBoardPackProblem(f.params),c=encodeNativeBlockSolveProblem(f.params);
  assert.equal(addon.evaluateBoardQuality(b).rank.score,addon.evaluateBoardQuality({...b,relations:[]}).rank.score);
  assert.equal(addon.evaluateBlockQuality(c).score,addon.evaluateBlockQuality({...c,relations:[]}).score);
  assert.equal(addon.scorePostPlace(encodeNativePostPlaceScoreProblem(f.input,f.placements)),0);
  const refine=encodeNativePostPlaceRefineProblem(f.input,f.placements,1);refine.iterations=0;
  assert.equal(addon.refinePostPlacement(refine).scoreBefore,0);
});
test('mixed-layer group is not silently accepted by same_side/prefer_layer',()=>{
  for(const relation of ['same_side','prefer_layer']) {
    const f=fixture(10,'bottom',[{relation,source:{type:'block',block_name:'A'},...(relation==='same_side'?{target:endpoint('A')}:{layer:'top'}),priority:'normal'}]);
    f.input.blocks[0].component_designators=['A','B'];f.input.components[1].block_name='A';
    f.params.options.componentByDesignator=new Map(f.input.components.map(c=>[c.designator,c]));
    const board=encodeNativeBoardPackProblem(f.params), block=encodeNativeBlockSolveProblem(f.params);
    const expected=relation==='same_side'?1400:700;
    close(addon.evaluateBoardQuality(board).rank.score-addon.evaluateBoardQuality({...board,relations:[]}).rank.score,expected);
    close(addon.evaluateBlockQuality(block).score-addon.evaluateBlockQuality({...block,relations:[]}).score,expected);
    assert.equal(addon.scorePostPlace(encodeNativePostPlaceScoreProblem(f.input,f.placements)),expected);
    const refine=encodeNativePostPlaceRefineProblem(f.input,f.placements,1);refine.iterations=0;
    assert.equal(addon.refinePostPlacement(refine).scoreBefore,expected);
  }
});
test('Board FastRoute receives case-insensitive exclusions without deleting geometry',()=>{
  const f=fixture(10);f.params.options.ignoredNets=['ignore'];
  for(const [i,p] of f.params.primitives.entries())p.connectionPoints=[{ref:`${i?'B':'A'}.1`,net:'IGNORE',x:i*10,y:0}];
  const p=encodeNativeBoardPackProblem(f.params);p.routingObstacles=[{box:{left:4,right:6,top:-1,bottom:1},ref:'X.1',net:'BLOCK',layer:'top'}];assert.deepEqual(p.ignoredNets,['ignore']);
  assert.ok(addon.scoreFastRouteLayout({...p,ignoredNets:[]})>0);assert.equal(addon.scoreFastRouteLayout(p),0);
  assert.equal(p.primitives.flatMap(p=>p.connectionPoints).length,2);
});

test('clearance(block, all) still penalizes foreign details',()=>{
    const r=measure(10,'top',{relation:'clearance',source:{type:'block',block_name:'A'},target:'all',priority:'normal',min:12});
    allStages(r);assert.equal(r.post,12600);
});
