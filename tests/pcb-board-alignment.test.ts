import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { alignmentAnchor, alignmentHardHintsNoWorse, blockSimilarity, findAlignmentPairs, refineBoardAlignment, BOARD_ALIGNMENT_POLICY } from '../src/pcb-layout/pcb-auto-place-v2/board-alignment.ts';
import { componentBox } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import { createPlacementReport } from '../src/pcb-layout/pcb-auto-place/placement-report.ts';
import type { PlacementInput, Placement, PlacementReport } from '../src/types/pcb/layout-model.ts';
import type { PlacementPrimitive } from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';
import { boardAlignmentPolicy, boardAlignmentScore, footprintOrientationOffset } from '../src/pcb-layout/pcb-auto-place-v2/board-alignment.ts';
import { boardElectricalQuality, boardElectricalRegression } from '../src/pcb-layout/pcb-auto-place-v2/board-alignment.ts';
import { buildPlacementGraph } from '../src/pcb-layout/pcb-auto-place/placement-graph.ts';
import { encodeNativeBoardPackProblem } from '../src/pcb-layout/pcb-auto-place-v2/native/encode-board-problem.ts';
import { loadNativeBoardPacker } from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import { translatePrimitive } from '../src/pcb-layout/pcb-auto-place-v2/primitives.ts';

function telemetry() {
    const input = JSON.parse(readFileSync('docs/experimental/pcb/global-placement-2026-09-27/Telemetry/input.json','utf8')) as PlacementInput;
    const saved = JSON.parse(gunzipSync(readFileSync('docs/experimental/pcb/telemetry-anchored-2026-09-27/after-final.json.gz')).toString());
    return { input, placements: saved.placements as Placement[], roots: saved.stages[0].data.root.children as PlacementPrimitive[] };
}
test('structural similarity tolerates a diode and ignores reference/net names and enumeration order', () => {
    const { input } = telemetry();
    const block = (name:string) => input.components.filter(c=>c.block_name===name);
    const a=block('hv_pos'), b=block('hv_neg');
    assert.ok(blockSimilarity(a,b) > .9);
    assert.equal(alignmentAnchor(a),'L3'); assert.equal(alignmentAnchor(b),'L4');
    const renamed = a.toReversed().map(c=>({...c,designator:c.designator+'99',pins:c.pins.toReversed().map(p=>({...p,signal_name:p.signal_name?'renamed:'+p.signal_name:p.signal_name}))}));
    assert.ok(Math.abs(blockSimilarity(a,renamed)-1) < 1e-9);
    assert.ok(blockSimilarity(a,block('usb_charge')) < BOARD_ALIGNMENT_POLICY.similarity);
    assert.ok(blockSimilarity(block('esp_enable'),block('temperature')) < BOARD_ALIGNMENT_POLICY.similarity);
});
test('anchor falls back to a substantial IC or block center when sizes are equal', () => {
    const { input }=telemetry();
    const caps=input.components.filter(c=>['C49','C50'].includes(c.designator));
    assert.equal(alignmentAnchor(caps),undefined);
    const ic=structuredClone(input.components.find(c=>c.designator==='U13')!);
    ic.footprint.width=caps[0].footprint.width;ic.footprint.height=caps[0].footprint.height;
    assert.equal(alignmentAnchor([...caps,ic]),'U13');
});
function simple() {
    const { input }=telemetry();
    input.components=input.components.filter(c=>['L3','L4'].includes(c.designator));
    input.components.forEach(c=>c.pins.forEach(p=>p.signal_name=''));
    input.blocks=input.components.map(c=>({name:c.block_name!,role:'power',description:'',component_designators:[c.designator]}));
    input.modules=[];input.hints=[];input.paths=[];input.refineGroups=[];input.boardHoles=[];input.constraintRegions=[];
    input.board.outline={type:'rect',width:100,height:100};
    const placements:Placement[]=input.components.map((c,i)=>({designator:c.designator,x:i?10:-10,y:i?1:0,rotate:0,layer:'top',score:0}));
    const roots:PlacementPrimitive[]=placements.map((p,i)=>{
        const bbox=componentBox(input.components[i],p);
        return {id:p.designator,label:p.designator,sourceNodeId:p.designator,kind:'block',bbox,width:bbox.right-bbox.left,height:bbox.bottom-bbox.top,
            placements:[p],children:[],connectionPoints:[]};
    });
    return {input,roots,placements};
}
test('soft pass aligns a free pair and is deterministic without mutating inputs',()=>{
    const {input,roots,placements}=simple();
    const original=JSON.stringify({input,roots,placements});
    const result=refineBoardAlignment(input,roots,placements);
    assert.ok(result.moves.length>0);assert.equal(result.after[0].error,0);
    assert.equal(createPlacementReport(input,result.placements).ok,true);
    assert.deepEqual(refineBoardAlignment(input,roots,placements).placements,result.placements);
    assert.equal(JSON.stringify({input,roots,placements}),original);
});
test('fixed anchors stay exact and similarity does not attract distant or opposite-side blocks',()=>{
    const {input,roots,placements}=simple();
    roots[0].locked=true;
    input.components[0].pcb.fixedPlacement={...placements[0]};
    const result=refineBoardAlignment(input,roots,placements);
    assert.equal(result.after[0].error,0);assert.deepEqual(result.placements[0],placements[0]);
    assert.equal(findAlignmentPairs(input,roots,placements.map((p,i)=>i?{...p,x:35}:p)).length,0);
    assert.equal(findAlignmentPairs(input,roots,placements.map((p,i)=>i?{...p,layer:'bottom'}:p)).length,0);
});
test('alignment rejects a collision at the desired axis and retains safe geometry',()=>{
    const {input,roots,placements}=simple();
    roots[0].locked=true; input.components[0].pcb.fixedPlacement={...placements[0]};
    // At y=1 the right footprint clears this keepout; at y=0 it collides.
    input.constraintRegions=[{name:'under-right',layers:['top'],allowBlocks:[],box:{left:0,right:30,top:-20,bottom:-5.8}}];
    assert.equal(createPlacementReport(input,placements).ok,true);
    const result=refineBoardAlignment(input,roots,placements);
    assert.ok(result.after[0].error>0);
    assert.equal(createPlacementReport(input,result.placements).ok,true);
});
test('hard hint guard rejects deepening an existing violation, even if its key stays unchanged',()=>{
    const {input,placements}=simple();
    const before=createPlacementReport(input,placements);
    const hint={relation:'clearance' as const,source:{type:'block' as const,block_name:'a'},target:{type:'block' as const,block_name:'b'},min:3.2,priority:'critical' as const};
    before.hintViolations=[{hint,actual:1.6,expected:'>= 3.2mm clearance'}];
    const after:PlacementReport={...before,hintViolations:[{hint,actual:.9,expected:'>= 3.2mm clearance'}]};
    assert.equal(alignmentHardHintsNoWorse(before,after),false);
    after.hintViolations[0].actual=2;
    assert.equal(alignmentHardHintsNoWorse(before,after),true);
});
test('Telemetry pass preserves hard hint magnitudes, fixed poses, and rigid block interiors',()=>{
    const {input,roots,placements}=telemetry();
    const result=refineBoardAlignment(input,roots,placements);
    assert.ok(result.pairs.some(p=>p.anchorA==='L4'&&p.anchorB==='L3'));
    assert.ok(alignmentHardHintsNoWorse(createPlacementReport(input,placements),createPlacementReport(input,result.placements)));
    for(const root of roots){
        const diffs=root.placements.map(p=>{
            const a=placements.find(q=>q.designator===p.designator)!,b=result.placements.find(q=>q.designator===p.designator)!;
            assert.equal(a.rotate,b.rotate);assert.equal(a.layer,b.layer);
            assert.ok(Math.hypot(a.x-b.x,a.y-b.y)<=BOARD_ALIGNMENT_POLICY.maxShift+.001);
            if(input.components.find(c=>c.designator===p.designator)!.pcb.fixedPlacement)assert.deepEqual(a,b);
            return [b.x-a.x,b.y-a.y];
        });
        assert.ok(diffs.every(d=>Math.abs(d[0]-diffs[0][0])<.002&&Math.abs(d[1]-diffs[0][1])<.002));
    }
});

test('packing discovers distant structural peers before placement, without a distance escape from penalties',()=>{
    const {input,roots,placements}=telemetry();
    const policy=boardAlignmentPolicy(input,roots);
    assert.ok(policy.pairs.some(p=>[p.anchorA,p.anchorB].includes('U1')&&[p.anchorA,p.anchorB].includes('U2')));
    assert.ok(!findAlignmentPairs(input,roots,placements).some(p=>p.anchorA==='U2'&&p.anchorB==='U1'));
    const f=simple(), p=boardAlignmentPolicy(f.input,f.roots);
    assert.equal(boardAlignmentScore([f.roots[0]],p),0);
    const aligned=[f.roots[0],translatePrimitive(f.roots[1],0,-1)];
    assert.ok(boardAlignmentScore(aligned,p)<boardAlignmentScore(f.roots,p));
    assert.equal(boardAlignmentScore([aligned[0],translatePrimitive(aligned[1],40,0)],p),0);
    assert.equal(boardAlignmentScore([f.roots[0],translatePrimitive(f.roots[1],0,-.9)],p),boardAlignmentScore(aligned,p));
});

test('native and TS early alignment scores agree, anchors validated, rotated candidates remain legal',()=>{
    const {input,roots}=simple(), policy=boardAlignmentPolicy(input,roots);
    const problem=encodeNativeBoardPackProblem({node:buildPlacementGraph(input).root,primitives:roots.map(p=>({...p,locked:true})),relations:[],options:{
        grid:.5,clearance:.2,bounds:{left:-49,right:49,top:-49,bottom:49},board:input.board,
        componentByDesignator:new Map(input.components.map(c=>[c.designator,c])),softAlignment:policy}});
    const addon=loadNativeBoardPacker();
    const aligned=addon.solveBoardPacked(problem), plain=addon.solveBoardPacked({...problem,softAlignment:undefined});
    assert.ok(Math.abs(aligned.rank.score-plain.rank.score-boardAlignmentScore(roots,policy))<1e-6);
    assert.throws(()=>addon.solveBoardPacked({...problem,softAlignment:{...policy,weight:-1}}),/softAlignment/);
    assert.throws(()=>addon.solveBoardPacked({...problem,softAlignment:{...policy,pairs:[{...policy.pairs[0],anchorA:'absent'}]}}),/anchor/);
    problem.primitives[1].locked=false;problem.primitives[1].allowedOrientations=[0,90,180,270];
    const found=addon.solveBoardPacked(problem);
    assert.equal(found.rank.hardCount,0);
    const ps=found.states.flatMap(s=>s.placements??[]);
    assert.equal(ps.length,2);
    assert.ok(Math.min(Math.abs(ps[0].x-ps[1].x),Math.abs(ps[0].y-ps[1].y))<=.15);
});

test('axis penalty stays positive beyond 3.15mm and cannot be escaped by moving along the other axis',()=>{
    const {input,roots}=simple(),policy={...boardAlignmentPolicy(input,roots),orientationWeight:0};
    const score=(dx:number,dy:number)=>boardAlignmentScore([roots[0],translatePrimitive(roots[1],dx,dy)],policy);
    assert.equal(score(0,-1),0);
    assert.ok(score(0,7)>score(0,3));
    assert.equal(score(40,7),score(0,7));
    assert.ok(score(0,3)>0);
});

test('electrical acceptance bounds total wire growth and large individual stretches',()=>{
    const baseline={score:10000,lengths:[10,10,10]};
    assert.equal(boardElectricalRegression(baseline,{score:9000,lengths:[14,2,2]}),'individual net length');
    assert.equal(boardElectricalRegression(baseline,{score:9000,lengths:[11,10,10]}),'total net length');
    assert.equal(boardElectricalRegression(baseline,{score:9000,lengths:[11,9,10]}),undefined);
    assert.equal(boardElectricalRegression(baseline,{score:10020,lengths:[10,10,10]}),'wiring score');
    assert.equal(boardElectricalRegression(baseline,{score:9990,lengths:[10.1,10,9]}),undefined);
});

test('Telemetry geometric blocks align on an intermediate axis without changing C9 or mandatory constraints',()=>{
    const {input}=telemetry();
    const saved=JSON.parse(gunzipSync(readFileSync('tests/fixtures/block-placement/Telemetry/board-alignment-geometric.json.gz')).toString()) as {roots:PlacementPrimitive[];placements:Placement[]};
    const result=refineBoardAlignment(input,saved.roots,saved.placements);
    const pair=result.after.find(p=>[p.anchorA,p.anchorB].includes('U1')&&[p.anchorA,p.anchorB].includes('U2'))!;
    assert.ok(pair.error<=.15);
    const before=new Map(saved.placements.map(p=>[p.designator,p]));
    const after=new Map(result.placements.map(p=>[p.designator,p]));
    for(const axis of ['x','y'] as const)
        assert.ok(Math.abs((before.get('C9')![axis]-before.get('U2')![axis])-(after.get('C9')![axis]-after.get('U2')![axis]))<.001);
    assert.equal(createPlacementReport(input,result.placements).ok,true);
    assert.ok(alignmentHardHintsNoWorse(createPlacementReport(input,saved.placements),createPlacementReport(input,result.placements)));
    assert.equal(boardElectricalRegression(boardElectricalQuality(input,saved.placements),boardElectricalQuality(input,result.placements)),undefined);
    for(const c of input.components.filter(c=>c.pcb.fixedPlacement)) assert.deepEqual(after.get(c.designator),before.get(c.designator));
});

test('orientation uses main ICs, independently of larger alignment anchors, and normalizes library angle',()=>{
    const {input,roots}=telemetry(),policy=boardAlignmentPolicy(input,roots);
    const charge=policy.pairs.find(p=>p.anchorA==='R38')!;
    assert.equal(charge.orientation?.a,'U16');assert.equal(charge.orientation?.b,'U20');
    const hv=policy.pairs.find(p=>p.anchorA==='L4')!;
    assert.equal(hv.orientation?.a,'U17');assert.equal(hv.orientation?.b,'U13');
    const u1=input.components.find(c=>c.designator==='U1')!,u2=input.components.find(c=>c.designator==='U2')!;
    assert.equal(footprintOrientationOffset(u1,u2),270);
    const bottom=roots.map(r=>({...r,placements:r.placements.map(p=>({...p,layer:'bottom' as const}))}));
    const bottomPair=boardAlignmentPolicy(input,bottom).pairs.find(p=>p.orientation?.a==='U2')!;
    assert.equal(bottomPair.orientation!.offset,-90);
    const copy=structuredClone(u1);
    copy.footprint.pads=copy.footprint.pads.map(p=>({...p,x:-p.y*1.1+20,y:p.x*.95-3}));
    assert.equal(footprintOrientationOffset(u1,copy),90);
    // A reflected pad numbering is not equivalent to a rotation.
    copy.footprint.pads=copy.footprint.pads.map(p=>({...p,x:-p.x}));
    assert.equal(footprintOrientationOffset(u1,copy),undefined);
});

test('orientation penalty is soft, bounded, and matches Rust at 0, 90, 180 and normalized angles',()=>{
    const {input,roots}=simple(),policy={...boardAlignmentPolicy(input,roots),weight:0,orientationWeight:120};
    const addon=loadNativeBoardPacker();
    let parallelScore=0;
    for(const angle of [0,90,180,270]){
        const rs=structuredClone(roots);rs[1].placements[0].rotate=angle;
        const problem=encodeNativeBoardPackProblem({node:buildPlacementGraph(input).root,primitives:rs.map(p=>({...p,locked:true})),relations:[],options:{
            grid:.5,clearance:.2,bounds:{left:-49,right:49,top:-49,bottom:49},board:input.board,
            componentByDesignator:new Map(input.components.map(c=>[c.designator,c])),softAlignment:policy}});
        const delta=addon.solveBoardPacked(problem).rank.score-addon.solveBoardPacked({...problem,softAlignment:undefined}).rank.score;
        assert.ok(Math.abs(delta-boardAlignmentScore(rs,policy))<1e-6);
        if(angle===0)parallelScore=delta;
        if(angle===90)assert.ok(Math.abs(delta-60)<1e-6);
        if(angle===180)assert.ok(Math.abs(delta-120)<1e-6);
        assert.deepEqual(addon.solveBoardPacked(problem).states.map(s=>s.rotation),[0,0]);
    }
    policy.pairs[0].orientation!.offset=90;
    const rs=structuredClone(roots);rs[0].placements[0].rotate=90;
    assert.ok(Math.abs(boardAlignmentScore(rs,policy)-parallelScore)<1e-6);
});
