import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import { blockQuality, comparableBlockQuality, selectBlockCandidates, legalBlockCandidate } from '../src/pcb-layout/pcb-auto-place-v2/block-quality.ts';
import { createClearanceResolver } from '../src/pcb-layout/pcb-auto-place/clearance-resolver.ts';
import { suspiciousBlockRoles, withRoleHypotheses } from '../src/pcb-layout/pcb-auto-place-v2/block-role-hypotheses.ts';
import { componentBox } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import type { PlacementInput, Placement } from '../src/types/pcb/layout-model.ts';
import type { BlockSolveParams } from '../src/pcb-layout/pcb-auto-place-v2/block-solver.ts';
import type { NativePostPlaceScoreProblemV1 } from '../src/pcb-layout/pcb-auto-place-v2/native/contract.ts';
import type { PlacementPrimitive } from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';
import { withBlockSolverCapture } from '../src/pcb-layout/pcb-auto-place-v2/block-solver-engine.ts';
import { solvePlacementTreeBottomUp } from '../src/pcb-layout/pcb-auto-place-v2/tree-solver.ts';
import { buildPlacementGraph } from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';

test('internal IC segment ignores only its own IC pads; every foreign pad still costs', () => {
    const addon = loadNativeBoardPacker();
    const problem: NativePostPlaceScoreProblemV1 = {
        version: 1, padCrossingWeight: 180, nets: [{ name: 'N', points: [{x:0,y:0},{x:4,y:0}],
            weight: 1, layers: ['top','top'], internalOwners: ['U1','U1'] }],
        routingObstacles: ['U1.3','U1.4','R1.1','R2.1'].map((ref,i)=>({ref, net:'OTHER',layer:'top',
            box:{left:.5+i*.5,right:.7+i*.5,top:-.1,bottom:.1}})),
        distances:[], clearances:[], fixedPenalties:[], edges:[], paths:[],
    };
    const baseline = addon.scorePostPlace({...problem,routingObstacles:[]});
    assert.equal(addon.scorePostPlace(problem)-baseline, 360);
    problem.nets[0].internalOwners = ['U1',null];
    assert.equal(addon.scorePostPlace(problem)-baseline, 720, 'external line still sees unrelated IC pads');
    problem.routingObstacles!.push(problem.routingObstacles![2]);
    assert.equal(addon.scorePostPlace(problem)-baseline, 720, 'duplicate physical pad is charged once');
});

const input = (): PlacementInput => JSON.parse(readFileSync(new URL('./fixtures/block-placement/Telemetry/input.json', import.meta.url),'utf8'));
function primitive(data: PlacementInput, pose: Placement): PlacementPrimitive {
    const c=data.components.find(c=>c.designator===pose.designator)!;
    const bbox=componentBox(c,pose);
    return {id:pose.designator,label:pose.designator,kind:'component',sourceNodeId:pose.designator,
        placements:[pose],bbox,width:bbox.right-bbox.left,height:bbox.bottom-bbox.top,children:[],connectionPoints:[]};
}

test('role trial does not mutate input, clearances, or make an unchanged layout cheaper', () => {
    const data=input(), before=JSON.stringify(data);
    const primitives=['U2','C9'].map((designator,i)=>primitive(data,{designator,x:i*8,y:0,rotate:0,layer:'top',score:0}));
    const params={primitives,options:{componentByDesignator:new Map(data.components.map(c=>[c.designator,c])),clearanceResolver:()=>.6}} as unknown as BlockSolveParams;
    const roles=suspiciousBlockRoles(params);
    assert.deepEqual(roles.map(r=>[r.designator,r.to]),[['C9','passive']]);
    const trial=withRoleHypotheses(params,roles);
    assert.equal(trial.options.componentByDesignator!.get('C9')!.pcb.role,'passive');
    assert.equal(trial.options.clearanceResolver,params.options.clearanceResolver);
    const alternate={...data,components:[...trial.options.componentByDesignator!.values()]};
    assert.deepEqual(blockQuality(data,primitives),blockQuality(alternate,primitives));
    assert.equal(JSON.stringify(data),before);
    params.primitives=primitives.filter(p=>p.label!=='C9');
    assert.deepEqual(suspiciousBlockRoles(params),[]);
});

test('a small area saving cannot buy several times longer C9 links; bad variants do not fill the portfolio', () => {
    const data=input();
    const snapshots=JSON.parse(readFileSync(new URL('./fixtures/block-placement/Telemetry/c9-checkpoints.json',import.meta.url),'utf8'));
    const near=snapshots.singles.map((p:Placement)=>primitive(data,p)),far=snapshots.pairs.map((p:Placement)=>primitive(data,p));
    const a=blockQuality(data,near),b=blockQuality(data,far);
    assert.ok(a.links['C9.1->U2']<3 && b.links['C9.1->U2']>7);
    assert.ok(b.area<a.area, 'the damaging historical move really did save area');
    assert.ok(b.localStretch>a.localStretch+200);
    assert.ok(b.score>a.score+200);
    const worse=b;
    assert.equal(comparableBlockQuality(worse,a),false);
    const candidate={stage:'singles',hypothesis:'role:C9=passive',primitives:near,quality:a};
    const chosen=selectBlockCandidates([candidate,{...candidate,stage:'pairs',primitives:far,quality:worse},
        {...candidate,stage:'beam'}]);
    assert.deepEqual(chosen,[candidate]);
});

test('parallel capacitors on the same two nets do not multiply role trials', () => {
    const data=input();
    const designators=['C13','C14','C15','C16','C9'];
    const primitives=designators.map((designator,i)=>primitive(data,{designator,x:i*8,y:0,rotate:0,layer:'top',score:0}));
    const params={primitives,options:{componentByDesignator:new Map(data.components.map(c=>[c.designator,c]))}} as unknown as BlockSolveParams;
    assert.deepEqual(suspiciousBlockRoles(params).map(r=>r.designator),['C9']);
});

test('checkpoint admission rejects a violated explicit hard pin distance', () => {
    const data=input();
    const snapshots=JSON.parse(readFileSync(new URL('./fixtures/block-placement/Telemetry/c9-checkpoints.json',import.meta.url),'utf8'));
    const ps=snapshots.singles.map((p:Placement)=>primitive(data,p));
    assert.equal(legalBlockCandidate(data,ps,createClearanceResolver(data)),true);
    data.hints.push({relation:'critical_pair',source:{type:'pin',designator:'C9',pin_number:'1'},
        target:{type:'pin',designator:'U2',pin_number:'6'},maxDistance:2,hard:true,priority:'critical'});
    assert.equal(legalBlockCandidate(data,ps,createClearanceResolver(data)),false);
});

test('plain blocks above twelve primitives receive the same beam policy', () => {
    const data=input(), source=data.components.find(c=>c.designator==='C9')!;
    data.components=Array.from({length:13},(_,i)=>({...structuredClone(source),designator:`C${i+1}`,block_name:'large',
        pcb:{...source.pcb,role:'passive' as const},pins:source.pins.map(p=>({...p,signal_name:`N${i}_${p.pin_number}`}))}));
    data.blocks=[{name:'large',description:'',role:'generic',component_designators:data.components.map(c=>c.designator)}];
    data.hints=[];data.paths=[];data.modules=[];data.refineGroups=[];data.constraintRegions=[];data.boardHoles=[];
    const stop=new Error('captured');
    assert.throws(()=>withBlockSolverCapture(p=>{
        assert.equal(p.primitives.length,13);
        assert.equal(p.options.searchWidth,4);
        assert.equal(p.options.experiments?.orderBranching,true);
        throw stop;
    },()=>solvePlacementTreeBottomUp(data,buildPlacementGraph(data))),e=>e===stop);
});
