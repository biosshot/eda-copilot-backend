import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import type {PlacementInput,PlacementTreeNode} from '../src/types/pcb/layout-model.ts';
import {buildPlacementGraph} from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import {createFixedPlacement} from '../src/pcb-layout/pcb-auto-place/fixed.ts';
import {createClearanceResolver} from '../src/pcb-layout/pcb-auto-place/clearance-resolver.ts';
import {solvePlacementSubtreeSync} from '../src/pcb-layout/pcb-auto-place-v2/tree-solver.ts';
import {legalBlockCandidate} from '../src/pcb-layout/pcb-auto-place-v2/block-quality.ts';
import {boardSpacingPolicy,boardSpacingPenalty} from '../src/pcb-layout/pcb-auto-place-v2/board-spacing.ts';
import {translatePrimitive} from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';
import {applyNativeBoardPackSolution} from '../src/pcb-layout/pcb-auto-place-v2/native/apply-board-solution.ts';

const fixture=()=>JSON.parse(readFileSync('docs/experiments/global-placement-2026-09-27/Telemetry/input.json','utf8')) as PlacementInput;

test('native transforms preserve the exact fixed pose, including sub-grid precision',()=>{
    const input=fixture();
    input.components.find(c=>c.designator==='R30')!.pcb.fixedPlacement={x:12.123456,y:3.765432,rotate:90,layer:'top'};
    const graph=buildPlacementGraph(input);
    const find=(n:PlacementTreeNode):PlacementTreeNode|undefined=>n.kind==='component'&&n.label==='R30'?n:n.children.map(find).find(Boolean);
    const p=solvePlacementSubtreeSync({input,graph,node:find(graph.root)!}).root;
    const state={primitiveId:p.id,rotation:0,translationX:0,translationY:0};
    const solution={version:4,states:[state],rank:{hardCount:0,hardSeverity:0,score:0}};
    assert.equal(applyNativeBoardPackSolution([p],solution)[0],p);
    state.translationX=.5;
    assert.throws(()=>applyNativeBoardPackSolution([p],solution),/moved locked primitive/);
});

test('comfort spacing vanishes on a dense board and saturates on a spacious board',()=>{
    const input=fixture();
    const policy=boardSpacingPolicy(input);
    assert.ok(policy.gap>0&&policy.gap<=3);
    input.board.outline={type:'rect',width:40,height:40};
    assert.equal(boardSpacingPolicy(input).gap,0);
    input.board.outline={type:'rect',width:300,height:300};
    assert.equal(boardSpacingPolicy(input).gap,3);
});

test('Telemetry USB family uses fixed J5 frame, board cutout and all fixed obstacles',()=>{
    const input=fixture(), graph=buildPlacementGraph(input);
    const find=(n:PlacementTreeNode):PlacementTreeNode|undefined=>n.kind==='block'&&n.label==='mechanic_J5'?n:n.children.map(find).find(Boolean);
    const node=find(graph.root)!;
    assert.ok(node);
    const original=JSON.stringify(input);
    const result=solvePlacementSubtreeSync({input,graph,node});
    assert.ok(result.root.anchored);
    assert.equal(result.root.placements.length,5);
    const fixed=createFixedPlacement(input,input.components.find(c=>c.designator==='J5')!)!;
    const variants=[result.root,...result.root.layoutAlternatives??[]];
    for(const p of variants){
        const actual=p.placements.find(q=>q.designator==='J5')!;
        for(const k of ['x','y','rotate','layer'] as const)assert.equal(actual[k],fixed[k]);
        assert.ok(legalBlockCandidate(input,p.children,createClearanceResolver(input),true));
    }
    assert.equal(JSON.stringify(input),original);
    assert.equal(legalBlockCandidate(input,[translatePrimitive(result.root,1,0)],createClearanceResolver(input),true),false,
        'an anchored variant cannot move the fixed connector');
    // The strict world validator rejects a neighbour occupying a support part.
    const support=result.root.placements.find(p=>p.designator!=='J5')!;
    const neighbour=input.components.find(c=>c.designator==='J2')!;
    neighbour.pcb.fixedPlacement={x:support.x,y:support.y,rotate:0,layer:support.layer};
    assert.equal(legalBlockCandidate(input,result.root.children,createClearanceResolver(input),true),false);
    const obstructed=solvePlacementSubtreeSync({input,graph,node});
    assert.ok(obstructed.root.anchored);
    assert.ok(legalBlockCandidate(input,obstructed.root.children,createClearanceResolver(input),true));
    const free={...result.root,locked:false};
    const close=translatePrimitive(free,free.width+.5,0);
    const far=translatePrimitive(free,free.width+5,0);
    const unrelated={...input,blocks:[],hints:[]};
    assert.ok(boardSpacingPenalty(unrelated,[free,close],3)>boardSpacingPenalty(unrelated,[free,far],3));
    assert.equal(boardSpacingPenalty(unrelated,[free,far],3),0);
});
