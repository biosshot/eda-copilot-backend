import assert from 'node:assert/strict';
import test from 'node:test';
import {createRequire} from 'node:module';
import {readFileSync} from 'node:fs';
import {runAnalysisProblem, terminateAnalysisPool} from '../src/analyze/run-analysis.ts';
import type {AnalysisConductionReport, AnalysisConductionOptions, AnalysisStackEntry} from '../native/analyze/index.js';
import type {AnalysisProblem} from '../src/analyze/types.ts';
const native=createRequire(import.meta.url)('../native/node-addon/index.cjs') as typeof import('../native/node-addon/index.cjs');
assert.equal(typeof native.prepareAnalysisConduction,'function','Rebuild shared addon: npm run native:build');
function coupon(size=4) {
    const b=JSON.parse(readFileSync('tests/pcb-analyze/NanoScope/NanoScope.json','utf8'));
    b.outline=[{x:-100,y:-100},{x:100,y:-100},{x:100,y:100},{x:-100,y:100}];b.cutouts=[];b.holes=[];delete b.stackup;
    b.pads=[{id:'strip',number:'1',at:{x:0,y:0},rotationDeg:0,layers:[b.layers[0].name],shape:{kind:'rect',widthMm:size,heightMm:2}}];
    b.copper={fixed:{tracks:[],vias:[],zones:[]},editable:{tracks:[],vias:[],zones:[]}};
    return {json:JSON.stringify(b),layer:b.layers[0].name as string};
}
const semantic=(r:AnalysisConductionReport)=>{const {timings_ms,serialized_bytes,...rest}=r;return rest;};
test('conduction addon/worker parity, real elements, explicit sheet physics and strict options',{timeout:30_000},async()=>{
    try{
        const {json,layer}=coupon();const options:AnalysisConductionOptions={mesh:{min_angle_deg:0,max_area_mm2:0.1},profile:{stackup:[{kind:'copper',layer,thickness_mm:0.035}],materials:{[layer]:{conductivity_s_m:58e6,state:'20 C coupon assumption'}}}};
        const a=native.prepareAnalysisConduction(json,options);
        const b=await runAnalysisProblem({kind:'prepare-conduction',boardJson:json,...options},{onProgress:e=>{
            if(e.phase==='queued'){const m=options.profile?.materials?.[layer];if(m)m.conductivity_s_m=1;}
        }});
        assert.deepEqual(semantic(a),semantic(b));assert.equal(a.status,'complete');assert.equal(a.domains.length,1);assert.equal(a.domains[0]!.readiness.dc_ready,true);
        assert.ok(Math.abs(a.profile.layers[0]!.sheet_conductance_s!-2030)<1e-9);
        let area=0;for(const d of a.domains)for(const t of d.triangles){const [p,q,r]=t.map(i=>d.nodes[i]!);const s=((q![0]-p![0])*(r![1]-p![1])-(q![1]-p![1])*(r![0]-p![0]))/2;assert.ok(s>0&&s<=0.1+1e-10);area+=s;}
        assert.ok(Math.abs(area-8)<1e-8);assert.ok(a.domains[0]!.terminals[0]!.boundary_edges.length>4);
        assert.ok(a.serialized_bytes>0); // Compact Rust encoding; Node transport normalizes number formatting.
        const noProfile=await runAnalysisProblem({kind:'prepare-conduction',boardJson:json});assert.equal(noProfile.status,'partial');assert.equal(noProfile.domains[0]!.readiness.physics_complete,false);
        assert.equal((await runAnalysisProblem({kind:'prepare-conduction',boardJson:'{'})).status,'invalid');
        assert.equal(native.prepareAnalysisConduction(json,{profile:{materials:{BAD:{conductivity_s_m:1,state:'state'}}}}).status,'invalid');
        for(const extra of [{mesh:null},{mesh:{extra:1}},{mesh:{max_nodes:2}},{mesh:{max_area_mm2:NaN}},{profile:{materials:{[layer]:{conductivity_s_m:Infinity,state:'x'}}}},{profile:{barrel:{diameter_convention:'guess'}}}]){
            await assert.rejects(runAnalysisProblem({kind:'prepare-conduction',boardJson:json,...extra} as AnalysisProblem),{code:'INVALID_REQUEST'});
        }
        assert.throws(()=>native.prepareAnalysisConduction(json,{mesh:{extra:1}} as never));
        const limited=native.prepareAnalysisConduction(json,{mesh:{max_nodes:3}});assert.equal(limited.status,'partial');assert.equal(limited.counts.nodes,0);assert.ok(limited.diagnostics.some(d=>d.code==='mesh_unbuilt'));
    }finally{await terminateAnalysisPool();}
});
test('meshing abort/deadline kills the active process and the next task succeeds',{timeout:30_000},async()=>{
    try{
        const slow={kind:'prepare-conduction' as const,boardJson:coupon(100).json,mesh:{min_angle_deg:0,max_area_mm2:1e-6,max_nodes:500_000,max_refinement_steps:64}};
        const controller=new AbortController();let timer:ReturnType<typeof setTimeout>|undefined;
        try{await assert.rejects(runAnalysisProblem(slow,{signal:controller.signal,onProgress:e=>{if(e.phase==='started')timer=setTimeout(()=>controller.abort(),80);}}),{code:'ABORTED'});}finally{clearTimeout(timer);}
        const fast={kind:'prepare-conduction' as const,boardJson:coupon().json};
        assert.equal((await runAnalysisProblem(fast)).counts.built_domains,1);
        let stopping:Promise<void>|undefined;
        await assert.rejects(runAnalysisProblem(slow,{onProgress:e=>{if(e.phase==='started')setTimeout(()=>{stopping=terminateAnalysisPool();},80);}}),{code:'DISPOSED'});
        await stopping;
        assert.equal((await runAnalysisProblem(fast)).counts.built_domains,1);
        await assert.rejects(runAnalysisProblem(slow,{timeoutMs:100}),{code:'TIMEOUT'});
        assert.equal((await runAnalysisProblem(fast)).counts.built_domains,1);
        assert.equal(native.numericContract(),'f32-rte-ftz-v1');assert.equal(native.placementScoreContractVersion(),13);
    }finally{await terminateAnalysisPool();}
});
test('conduction preserves known R1 uncertainty per domain rather than only global status',async()=>{
    const b=JSON.parse(coupon().json);b.pads[0].shape={kind:'rect',widthMm:6,heightMm:1.999};b.pads[0].rotationDeg=-87.5;
    b.holes=[{id:'cut',at:{x:0,y:0},hole:{shape:'round',diameterMm:2,plated:false}}];
    const coarse=native.prepareAnalysisConduction(JSON.stringify(b),{mesh:{min_angle_deg:0}});
    assert.ok(coarse.domains.length>=2);assert.ok(coarse.domains.every(d=>!d.readiness.topology_confirmed&&!d.readiness.dc_ready));
    const fine=native.prepareAnalysisConduction(JSON.stringify(b),{geometry:{max_deviation_mm:0.0001},mesh:{min_angle_deg:0}});
    assert.ok(fine.domains.every(d=>d.readiness.topology_confirmed));assert.equal(fine.domains.length,2);
});
test('unconfirmed drill span blocks other layer readiness through addon and worker',async()=>{
    try {
        const board=JSON.parse(coupon().json),layers=board.layers.map((l:{name:string})=>l.name) as string[];
        board.pads[0].hole={shape:'round',diameterMm:1,plated:true};
        board.pads.push({...board.pads[0],id:'inner-copper',layers:[layers[1]],hole:undefined});
        const stackup:AnalysisStackEntry[]=[];
        for(const [i,layer] of layers.entries()){if(i)stackup.push({kind:'dielectric',name:`gap${i}`,thickness_mm:0.5});stackup.push({kind:'copper',layer,thickness_mm:0.035});}
        const options:AnalysisConductionOptions={mesh:{min_angle_deg:0},profile:{stackup,materials:Object.fromEntries(layers.map(layer=>[layer,{conductivity_s_m:58e6,state:'20 C coupon assumption'}]))}};
        const json=JSON.stringify(board),direct=native.prepareAnalysisConduction(json,options);
        const worker=await runAnalysisProblem({kind:'prepare-conduction',boardJson:json,...options});
        assert.deepEqual(semantic(direct),semantic(worker));assert.ok(worker.diagnostics.some(d=>d.code==='drill_span_unconfirmed'));
        assert.ok(worker.domains.some(d=>d.layer===layers[1]));assert.ok(worker.domains.every(d=>!d.readiness.copper_complete&&!d.readiness.dc_ready));
    } finally {await terminateAnalysisPool();}
});

test('lost geometry-confirmed barrel contact blocks dependent domains through addon and worker',async()=>{
    try {
        const {board,options}=JSON.parse(readFileSync('tests/fixtures/conduction-contact-coupon.json','utf8')) as {board:unknown;options:AnalysisConductionOptions};
        const json=JSON.stringify(board),direct=native.prepareAnalysisConduction(json,options);
        const worker=await runAnalysisProblem({kind:'prepare-conduction',boardJson:json,...options});
        assert.deepEqual(semantic(direct),semantic(worker));
        const barrel=worker.barrels[0]!;
        assert.equal(barrel.ready,false);assert.equal(barrel.expected_contacts.length,4);assert.equal(barrel.contacts.length,3);
        const missing=barrel.expected_contacts.find(c=>c.layer==='TOP')!;
        assert.equal(missing.ready,false);assert.equal(missing.domain_ids.length,1);
        assert.ok(missing.reasons.includes(`mesh_unbuilt:${missing.domain_ids[0]}`));
        assert.ok(worker.diagnostics.some(d=>d.code==='barrel_contact_unready'&&d.path==='pads[0]'&&d.message.includes(missing.domain_ids[0]!)));
        assert.ok(worker.domains.filter(d=>d.source_paths.includes('pads[0]')).every(d=>!d.readiness.dc_ready));
        assert.equal(worker.domains.find(d=>d.source_paths.includes('pads[1]'))!.readiness.dc_ready,true);
    } finally {await terminateAnalysisPool();}
});
