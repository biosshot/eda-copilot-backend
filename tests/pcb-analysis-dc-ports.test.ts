import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdir, writeFile } from 'node:fs/promises';
import { runAnalysisDsl } from '../src/analyze/run-analysis-dsl.ts';
import { dcCoupon, couponSetup } from './helpers/pcb-analysis-dc-coupon.ts';
import { completedDc, resolveDcMesh } from './helpers/pcb-analysis-dc-audit.ts';
import { renderDcReport } from './helpers/pcb-analysis-dc-render.ts';
import { dcPadPotentials } from '../src/analyze/analysis-dsl/dc-observation.ts';
import type { AnalysisDcReport } from '../native/analyze/index.js';

test('D1 unused ordinary, aggregated and unready-layer ports leave board assembly unchanged', async () => {
  const board = dcCoupon();
  board.layers.push({ name: 'BOTTOM', index: 1, side: 'bottom' });
  board.pads.push({ ...board.pads[0], id: 'unready', layers: ['BOTTOM'] });
  const setup = couponSetup.replace("{kind:'copper',name:'TOP',thicknessMm:.035,material:'cu'}", "{kind:'copper',name:'TOP',thicknessMm:.035,material:'cu'},{kind:'dielectric',name:'core',thicknessMm:1},{kind:'copper',name:'BOTTOM',thicknessMm:.035}");
  const r = await runAnalysisDsl({ board, code: `${setup}
    const before = await base.loads([{port:load,currentA:1}]).run();
    const unrelated = a.port({positive:padId('rail'),negative:padId('return')});
    const unready = a.port({positive:padId('unready'),negative:padId('return')});
    const aggregated = a.port({positive:[pad('J1','1'),pad('U1','1')],negative:pad('J1','2')});
    const after = await base.loads([{port:load,currentA:1}]).run();
    a.log({level:'info',message:'observations',data:{before:before.voltage(load),after:after.voltage(load),unready:after.voltage(unready),aggregated:after.voltage(aggregated)}});
  ` });
  assert.equal(r.status, 'complete', JSON.stringify(r.error));
  const [a, b] = completedDc(r).map(r => r.solution);
  assert.equal(a.status, 'complete'); assert.equal(b.status, 'complete');
  assert.deepEqual(a.domains, b.domains); assert.deepEqual(a.terminals, b.terminals);
  assert.deepEqual(a.included_domains, b.included_domains); assert.equal(a.copper_loss_w, b.copper_loss_w);
  assert.equal(b.reused_factorization, true);
  const data = r.reports[0].logs.find(l => l.message === 'observations')!.data as Record<string, {value?:number;validity:{status:string}}>;
  assert.equal(data.before.value, data.after.value);
  assert.equal(data.unready.validity.status, 'unavailable'); assert.equal(data.aggregated.validity.status, 'unavailable');
  const active = await runAnalysisDsl({ board, code: `${setup}
    const aggregated=a.port({positive:[pad('J1','1'),pad('U1','1')],negative:pad('J1','2')});
    await base.sources([{port:aggregated,voltageV:1,model:'ideal'}]).loads([{port:load,currentA:1}]).run();` });
  assert.equal(active.error?.code, 'UNSUPPORTED_DC_MODEL');
});

test('D1 passive wide tap integrates the unchanged linear field without equipotential shunt', async () => {
  const board = dcCoupon();
  board.pads.push({ ...board.pads[0], id: 'tap', at: {x: 5,y: 0}, shape: {kind:'rect',widthMm:4,heightMm:2} });
  const r = await runAnalysisDsl({ board, code: `${couponSetup}
    const before=await base.loads([{port:load,currentA:1}]).run();
    const observation=a.port({positive:padId('tap'),negative:pad('J1','2')});
    const after=await base.loads([{port:load,currentA:1}]).run();
    a.log({level:'info',message:'tap',data:{before:before.voltage(observation),after:after.voltage(observation),load:after.voltage(load)}});` });
  assert.equal(r.status, 'complete', JSON.stringify(r.error));
  const [a, b] = completedDc(r).map(r => r.solution);
  assert.deepEqual(a.domains, b.domains); assert.equal(a.copper_loss_w, b.copper_loss_w);
  assert.equal(b.reused_factorization, true);
  const data = r.reports[0].logs[0].data as Record<string,{value:number}>;
  const expected = 1 - .004 / (58e6 * .002 * .000035);
  assert.ok(Math.abs(data.before.value - expected) < 1e-12);
  assert.equal(data.before.value, data.after.value);
  const mesh = b.mesh ?? completedDc(r)[0].solution.mesh!;
  const domain = b.domains!.find(d => mesh.domains.find(m => m.id === d.domain_id)?.terminals.some(t => t.source.source_id === 'tap'))!;
  const terminal = mesh.domains.find(m => m.id === domain.domain_id)!.terminals.find(t => t.source.source_id === 'tap')!;
  const nodes = new Set(terminal.triangles.flatMap(i => mesh.domains.find(m => m.id === domain.domain_id)!.triangles[i]));
  const values = [...nodes].map(i => domain.voltage_v[i]);
  assert.ok(Math.max(...values) - Math.min(...values) > .0009, 'passive pad retains voltage gradient');
});

test('D1 standalone conductor ignores board ports, including aggregated and unready contacts', async () => {
  const board=dcCoupon(); board.layers.push({name:'BOTTOM',index:1,side:'bottom'});
  board.pads.push({...board.pads[0],id:'unready',layers:['BOTTOM']});
  const r = await runAnalysisDsl({ board, code: `const a=analysis({board});
    const conductor=a.conductor({lengthMm:10,widthMm:1,thicknessMm:.035,material:{conductivitySM:58e6,state:'test'}});
    const base=a.dc().conductor(conductor).currentA(1);
    await base.run();
    a.port({positive:[pad('J1','1'),pad('U1','1')],negative:pad('J1','2')});
    a.port({positive:padId('unready'),negative:padId('return')});
    await base.run();` });
  assert.equal(r.status, 'complete', JSON.stringify(r.error));
  const [a, b] = completedDc(r).map(r => r.solution); assert.deepEqual(a.standalone, b.standalone);
});

test('D1 passive pad with a bore measures remaining copper area; missing coverage stays unavailable', async () => {
  const board = dcCoupon();
  board.pads.push({ ...board.pads[0], id: 'tap', at: {x:5,y:0}, shape: {kind:'rect',widthMm:4,heightMm:2} });
  board.holes!.push({id:'tap-bore',at:{x:4,y:0},hole:{shape:'round',diameterMm:1,plated:false}});
  const result = await runAnalysisDsl({board,code:`${couponSetup}
    const d=await base.loads([{port:load,currentA:1}]).run();
    const p=a.port({positive:padId('tap'),negative:pad('J1','2')});
    a.check({name:'passive tap with bore',actual:d.voltage(p),min:.99,max:1,unit:'V'});`});
  assert.equal(result.status,'complete',JSON.stringify(result.error)); assert.equal(result.reports[0].checks[0].status,'pass');
  const s=completedDc(result)[0].solution, metrics=dcPadPotentials(board,s,{status:'valid'},[]);
  const area=(metrics['padVoltage:tap'].data as {areaMm2:number}).areaMm2;
  assert.ok(Math.abs(area-(8-Math.PI*.25))<.02, 'observation excludes the finite bore');
  const broken=structuredClone(s), d=broken.mesh!.domains.find(d=>d.terminals.some(t=>t.source.source_id==='tap'))!;
  d.terminals.find(t=>t.source.source_id==='tap')!.area_mm2 *= 1.1;
  assert.equal(dcPadPotentials(board,broken,{status:'valid'},[])['padVoltage:tap'].validity.status,'unavailable');
  // No fallback from an uncomputed first surface to a later computed one.
  const bottom=structuredClone(d);bottom.id='later';bottom.layer='BOTTOM';
  broken.mesh!.domains.push(bottom);broken.mesh!.profile.layers.push({...broken.mesh!.profile.layers[0],layer:'BOTTOM'});
  broken.domains!.push({...broken.domains!.find(f=>f.domain_id===d.id)!,domain_id:'later',layer:'BOTTOM'});
  broken.domains=broken.domains!.filter(f=>f.domain_id!==d.id);
  assert.equal(dcPadPotentials(board,broken as AnalysisDcReport,{status:'valid'},[])['padVoltage:tap'].validity.status,'unavailable');
});

export function twoCircuitCoupon() {
  const b = dcCoupon(); b.outline[2].y = 24; b.outline[3].y = 24;
  b.nets.push({name:'VCCB'},{name:'GNDB'});
  b.components.push(...b.components.filter(c => c.designator !== 'U2').map(c => ({...c,designator:c.designator==='J1'?'J2':'U3',at:{...c.at,y:c.at.y+12}})));
  b.pads.push(...b.pads.map(p => ({...p,id:p.id+'B',net:p.net==='VCC'?'VCCB':'GNDB',at:{...p.at,y:p.at.y+12},...(p.component?{component:p.component==='J1'?'J2':'U3'}:{})})));
  return b;
}
test('D2 new B and A+B domains retain immutable geometry and resolvable maps after JSON round-trip', async () => {
  const board = twoCircuitCoupon();
  const code = `${couponSetup}
    function plot(d,caseId,port,referenceV,loadScale){for(const field of [d.voltageMap,d.currentDensityMap,d.lossMap]){const {values,...reference}=field;a.plot({title:caseId,data:reference})}const voltage=d.voltage(port);const drop=typeof voltage.value==='number'?{...voltage,value:Math.abs(referenceV-voltage.value)}:voltage;a.log({level:'info',message:'dc.case',data:{caseId,resultId:d.id,maxAreaMm2:.125,loadScale,measurements:[{label:caseId,portId:port.id,referenceV,voltage,drop}]}})}
    const first=await base.loads([{port:load,currentA:1}]).run(); plot(first,'A only',load,1,1);
    const original=JSON.stringify(first);
    const sourceB=a.port({positive:pad('J2','1'),negative:pad('J2','2')});
    const loadB=a.port({positive:pad('U3','1'),negative:pad('U3','2')});
    const configured=base.sources([{port:sourceB,voltageV:2,model:'ideal'}]);
    const second=await configured.loads([{port:loadB,currentA:1}]).run();plot(second,'B only',loadB,2,1);
    const third=await configured.loads([{port:loadB,currentA:2}]).run();plot(third,'B raised',loadB,2,2);
    const combined=base.sources([{port:source,voltageV:1,model:'ideal'},{port:sourceB,voltageV:2,model:'ideal'}]);
    const fourth=await combined.loads([{port:load,currentA:1},{port:loadB,currentA:1}]).run();plot(fourth,'A+B',loadB,2,1);
    const fifth=await combined.loads([{port:load,currentA:2},{port:loadB,currentA:2}]).run();plot(fifth,'A+B raised',loadB,2,2);
    a.check({name:'first immutable',actual:JSON.stringify(first)===original,equals:true});
    const cross=a.port({positive:pad('J1','1'),negative:pad('J2','2')});
    a.log({level:'info',message:'uncomputed/incompatible',data:{outside:first.voltage(loadB),cross:fourth.voltage(cross)}});
  `;
  const result = JSON.parse(JSON.stringify(await runAnalysisDsl({board,code}))) as Awaited<ReturnType<typeof runAnalysisDsl>>;
  assert.equal(result.status, 'complete', JSON.stringify(result.error));
  assert.equal(result.reports[0].checks[0].status, 'pass');
  const observations=result.reports[0].logs.find(l=>l.message==='uncomputed/incompatible')!.data as Record<string,{validity:{status:string}}> ;
  assert.equal(observations.outside.validity.status,'unavailable'); assert.equal(observations.cross.validity.status,'unavailable');
  const all = completedDc(result); assert.equal(all.length, 5);
  const [a,b,c,d,e] = all;
  assert.ok(b.solution.reused_preparation); assert.equal(b.solution.reused_factorization,false);
  assert.ok(!b.solution.meshReference, 'new B must not reference A-only compact mesh');
  assert.equal(c.solution.meshReference,b.id); assert.equal(c.solution.reused_factorization,true);
  assert.ok(!d.solution.meshReference); assert.equal(e.solution.meshReference,d.id); assert.equal(e.solution.reused_factorization,true);
  assert.equal(d.solution.domains!.length,a.solution.domains!.length+b.solution.domains!.length);
  for(const r of all) {
    assert.equal(r.solution.status,'complete'); const mesh=resolveDcMesh(all,r);
    for(const f of r.solution.domains!) {
      const m=mesh.domains.find(m=>m.id===f.domain_id); assert.ok(m);
      assert.equal(m.nodes.length,f.voltage_v.length); assert.equal(m.triangles.length,f.loss_w.length);
      assert.equal(m.triangles.length,f.current_density_a_mm2.length);
      assert.ok(m.triangles.every(t=>t.every(i=>i>=0&&i<m.nodes.length)));
    }
  }
  // Optional local visual artifact; ordinary tests have no generated-file dependency.
  if(process.env.DC_REVIEW_ARTIFACTS) {
    const directory=process.env.DC_REVIEW_ARTIFACTS; await mkdir(directory,{recursive:true});
    await writeFile(`${directory}/dc.json`,JSON.stringify({metadata:{fixture:'Synthetic D2 A → B → A+B'},result}));
    await writeFile(`${directory}/scenario.js`,code);
    await writeFile(`${directory}/index.html`,renderDcReport(board,{fixture:'Synthetic D2 A → B → A+B'},result));
  }
});
