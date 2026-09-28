import test from 'node:test';
import assert from 'node:assert/strict';
import { nearBlockLayout, selectPairSeeds } from '../src/pcb-layout/pcb-auto-place-v2/block-search-stages.ts';
import type { BlockCandidate } from '../src/pcb-layout/pcb-auto-place-v2/block-quality.ts';

function candidate(): BlockCandidate {
    return { stage: 'singles', hypothesis: 'original', quality: { score: 100, electrical: 90, wire: 2, maxLocal: 1,
        localStretch: 0, area: 5, width: 5, height: 1, exposure: 0, links: {'R1.1->U1': 1}, ports: {} },
        primitives: ['U1', 'R1'].map((designator, i) => ({ id: designator, label: designator, kind: 'component', sourceNodeId: designator,
            bbox: {left:i*2,right:i*2+1,top:0,bottom:1}, width:1,height:1,children:[],connectionPoints:[],allowedOrientations:[0,90,180,270],
            placements:[{designator,x:i*2,y:0,rotate:0,layer:'top',score:0}] })) };
}

test('jitter dedup ignores global translation but protects orientation, groups, locked frame and electrical changes', () => {
    const a=candidate(), b=structuredClone(a);
    for(const p of b.primitives){p.placements[0].x+=10;p.placements[0].y-=3;}
    b.primitives[1].placements[0].x+=.1;
    assert.equal(nearBlockLayout(a,b),true);
    for(const edit of [(c:BlockCandidate)=>{c.primitives[1].placements[0].rotate=180;},
        (c:BlockCandidate)=>{c.quality.electrical+=180;},
        (c:BlockCandidate)=>{c.quality.links['R1.1->U1']+=.2;},
        (c:BlockCandidate)=>{c.primitives[1].placements[0].x+=.1;},
        (c:BlockCandidate)=>{c.primitives[0].locked=true;}]){
        const changed=structuredClone(b);edit(changed);assert.equal(nearBlockLayout(a,changed),false);
    }
    a.primitives[0].locked=b.primitives[0].locked=true;
    assert.equal(nearBlockLayout(a,b),false);
});

test('pair budget selects competitive distinct states without mutating the original pool', () => {
    const best=candidate(), duplicate=structuredClone(best), different=structuredClone(best), bad=structuredClone(best);
    duplicate.primitives[1].placements[0].x+=.05;
    different.primitives[1].placements[0].rotate=180;different.quality.score+=2;
    different.hypothesis='different';bad.hypothesis='bad';
    bad.quality.score=110;bad.quality.electrical=1000;
    const pool=[bad,duplicate,different,best], original=structuredClone(pool);
    const chosen=selectPairSeeds(pool,[best]);
    assert.equal(chosen.length,2);assert.ok(chosen.includes(different));assert.ok(!chosen.includes(bad));
    assert.deepEqual(pool,original);
});

test('successful postrefine does not exclude the same hypothesis from pair refinement', () => {
    const singles=candidate(), refined=structuredClone(singles);
    refined.stage='singles+postrefine';refined.quality.score=10;refined.quality.electrical=1;
    assert.deepEqual(selectPairSeeds([singles],[singles,refined]),[singles]);
});
