import test from'node:test';import assert from'node:assert/strict';import{readFileSync}from'node:fs';
import{runAnalysisDsl}from'../src/analyze/run-analysis-dsl.ts';import{auditCrossSection,sectionResults,auditSectionOrigin}from'./helpers/pcb-analysis-cross-section-audit.ts';
const board=JSON.parse(readFileSync('tests/pcb-analyze/BandAmpV5/BandAmpV5.json','utf8')),expected=JSON.parse(readFileSync('tests/pcb-analyze/BandAmpV5/impedance.expectations.json','utf8'));
function rejectDamagedSeries(base:any,board:any,expected:any,labels:string[]){
 const mutations:[string,(p:any,plots:any[])=>void][]=[
  ['empty',p=>p.data.values=[]],['truncated',p=>p.data.values.pop()],['extra',p=>p.data.values.push(0)],
  ['duplicate',(p,plots)=>plots.push(structuredClone(p))],['sparse',p=>delete p.data.values[1]],
  ['undefined',p=>p.data.values[1]=undefined],['null',p=>p.data.values[1]=null],['string',p=>p.data.values[1]='0'],
  ['NaN',p=>p.data.values[1]=NaN],['infinity',p=>p.data.values[1]=Infinity],['not array',p=>p.data.values={}],
  ['plot kind',p=>p.kind='table'],['data kind',p=>p.data.kind='scalar'],['unit',p=>p.data.unit='V'],
  ['validity',p=>p.data.validity.status='valid'],['axis name',p=>p.data.axis.name='timeS'],
  ['axis unit',p=>p.data.axis.unit='s'],['axis values',p=>p.data.axis.values.pop()],
 ];
 for(const label of labels)for(const suffix of ['magnitude','phase'])for(const[name,mutate]of mutations){
  // Only plots are mutated; reuse the read-only FEM/network result for every negative case.
  const r={...base,reports:base.reports.map((report:any)=>({...report,plots:structuredClone(report.plots)}))},title=label+' '+suffix;
  const report=r.reports.find((report:any)=>report.plots.some((p:any)=>p.title===title));assert.ok(report,title);
  mutate(report.plots.find((p:any)=>p.title===title),report.plots);
  assert.throws(()=>auditCrossSection(r,board,expected),undefined,`${title}: ${name}`);
 }
 assert.equal(auditCrossSection(base,board,expected).status,'pass');
}
test('versioned audit accepts production report and rejects geometry/field/provenance/phase/convergence corruption',async()=>{
 const base=await runAnalysisDsl({board,models:JSON.parse(readFileSync('tests/fixtures/analysis-cross-section/attenuator-pad.json','utf8')),code:readFileSync('tests/pcb-analyze/BandAmpV5/impedance.js','utf8')},{maxOutputBytes:128*1024*1024});assert.equal(auditCrossSection(base,board,expected).status,'pass');
 const mutations:[string,(r:any,s:any)=>void][]=[
  ['width',(_,s)=>s.input.widthMm*=2],['gap',(_,s)=>s.input.gapMm=.001],
  ['conductor id',(_,s)=>s.field.geometry.conductors[0].id='foreign'],['conductor order',(_,s)=>s.parameters.conductorOrder=['negative']],
  ['epsilon',(_,s)=>s.field.geometry.dielectrics[0].relativePermittivity*=2],
  ['phase',(_,s)=>s.network.parameters[1][1][0].imaginary*=-1],['mutual sign',(_,s)=>s.parameters.capacitanceFPerM[0][0]*=-1],
  ['deleted field',(_,s)=>delete s.field],['deleted E',(_,s)=>delete s.field.electricFieldsVPerM],['phi',(_,s)=>s.field.potentialsV[0][0]=.5],
  ['deleted plot',r=>r.reports[0].plots=r.reports[0].plots.filter((p:any)=>p.title!=='S21 phase')],
  ['plot array',r=>r.reports[0].plots.find((p:any)=>p.title==='S21 phase').data.values[1]*=-1],
  ['deleted convergence',(_,s)=>s.convergence=[]],['conditional promoted',(_,s)=>s.validity.status='valid'],
  ['stale hash',(_,s)=>s.geometrySha256='0'.repeat(64)],['port order',(_,s)=>s.network.ports.reverse()],
 ];
 for(const[name,mutate]of mutations){const r=structuredClone(base),s=sectionResults(r)[0].solution;mutate(r,s);assert.throws(()=>auditCrossSection(r,board,expected),undefined,name);}
 rejectDamagedSeries(base,board,expected,['S21','Line + Attenuator Pad S21']);
});
test('versioned audit accepts complete coupled production plots and rejects incomplete/invalid crosstalk series',async()=>{
 const board=JSON.parse(readFileSync('tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json','utf8')),expected=JSON.parse(readFileSync('tests/pcb-analyze/STM32G0_CAN_RS485/impedance.expectations.json','utf8'));
 const base=await runAnalysisDsl({board,code:readFileSync('tests/pcb-analyze/STM32G0_CAN_RS485/impedance.js','utf8')},{maxOutputBytes:128*1024*1024});assert.equal(auditCrossSection(base,board,expected).status,'pass');
 rejectDamagedSeries(base,board,expected,['Near crosstalk','Far crosstalk']);
});
test('board origin rejects wrong width, net/layer/ID/length/parallel gap and conductor reordering',()=>{
 const b=JSON.parse(readFileSync('tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json','utf8')),e=JSON.parse(readFileSync('tests/pcb-analyze/STM32G0_CAN_RS485/impedance.expectations.json','utf8'));auditSectionOrigin(b,e);
 for(const mutate of [(v:any)=>v.widthMm=.9,(v:any)=>v.gapMm*=2,(v:any)=>v.lengthMm*=2,(v:any)=>v.net.reverse(),(v:any)=>v.layer='BOTTOM',(v:any)=>v.tracks.reverse(),(v:any)=>v.segment[1][1][0]+=.01]){const x=structuredClone(e);mutate(x);assert.throws(()=>auditSectionOrigin(b,x));}
});
