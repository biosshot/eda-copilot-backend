import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import {runAnalysisDsl} from '../src/analyze/run-analysis-dsl.ts';
import {auditValidation,snapshotValidation,validationResults,independentPolygonDistance} from './helpers/pcb-analysis-validation-audit.ts';
const polygon=(outer:number[][],holes:number[][][]=[])=>({outer,holes});
test('independent saved-report oracle handles holes, containment, diagonals and cutouts',()=>{
 const a=polygon([[0,0],[1,0],[1,1],[0,1]]),b=polygon([[1.3,1.4],[2.3,1.4],[2.3,2.4],[1.3,2.4]]);assert.ok(Math.abs(independentPolygonDistance(a,b)-.5)<1e-12);const ring=polygon([[-2,-2],[3,-2],[3,3],[-2,3]],[[[-1,-1],[2,-1],[2,2],[-1,2]]]);assert.equal(independentPolygonDistance(a,ring),1);const solid=polygon([[-2,-2],[3,-2],[3,3],[-2,3]]);assert.equal(independentPolygonDistance(a,solid),0);assert.equal(independentPolygonDistance(a,solid,false),2);
});
test('versioned report audit rejects lost/duplicate findings, IDs/layers/distances/thresholds, diagnostics and false coverage',async()=>{
 const b=JSON.parse(readFileSync('tests/fixtures/analysis-dc-coupon.json','utf8'));b.components=[];b.pads=[{id:'a',number:'1',net:'VCC',at:{x:0,y:0},rotationDeg:0,layers:['TOP'],shape:{kind:'rect',widthMm:1,heightMm:1}},{id:'b',number:'1',net:'GND',at:{x:1.1,y:0},rotationDeg:0,layers:['TOP'],shape:{kind:'rect',widthMm:1,heightMm:1}}];b.copper.fixed.zones=[{id:'captured-zone',kind:'fill',net:'VCC',layers:['TOP'],outline:{outer:[{x:3,y:0},{x:4,y:0},{x:4,y:1},{x:3,y:1}]},filledPolygons:[{layer:'TOP',source:'cad',status:'available',freshness:'unknown',polygons:[]}]}];
 const rules=[{kind:'connectivity'},{kind:'clearance',minMm:.2},{kind:'edge-clearance',minMm:.3}];const output=await runAnalysisDsl({board:b,code:`const a=analysis({board});const g=await a.geometry().rules(${JSON.stringify(rules)}).run();a.plot({title:'geometry',data:g.measurements});a.check({name:'No confirmed violations',actual:g.violations.count(),equals:0});a.check({name:'Expected fail: control',actual:g.violations.items.length,equals:-1});a.check({name:'Continued',actual:g.measurements.unit,equals:'mm'});a.log({level:'info',message:'validation.case'});`},{timeoutMs:30000,maxOutputBytes:16*1024*1024});assert.equal(output.status,'complete',JSON.stringify(output.error));const solution=validationResults(output)[0].solution;const e={version:1,plotTitle:'geometry',expectedChecks:['unknown','fail','pass'],rules,snapshot:snapshotValidation(solution)};assert.equal(auditValidation(output,b,e).status,'pass');
 const corruptions:((r:any)=>void)[]=[
 r=>{const s=validationResults(r)[0].solution;s.measurements.pop();},
 r=>{const s=validationResults(r)[0].solution;s.violations=[];},
 r=>{validationResults(r)[0].solution.measurements[0].id='substituted';},
 r=>{validationResults(r)[0].solution.measurements[0].region_ids[0]='missing';},
 r=>{validationResults(r)[0].solution.measurements[0].layers=['BOTTOM'];},
 r=>{validationResults(r)[0].solution.measurements[0].actual_mm+=.01;},
 r=>{validationResults(r)[0].solution.measurements[0].required_mm+=.01;},
 r=>{validationResults(r)[0].solution.diagnostics.pop();},
 r=>{validationResults(r)[0].solution.coverage.forEach((c:any)=>c.status='complete');},
 r=>{validationResults(r)[0].solution.regions=[];},
 r=>{const s=validationResults(r)[0].solution;s.measurements.push(s.measurements[0]);},
 r=>{const s=validationResults(r)[0].solution;s.regions.push(s.regions[0]);},
 r=>{r.reports[0].checks[0].status='pass';},
 r=>{const s=validationResults(r)[0].solution;s.regions[0].polygon.outer[0][0]+=.01;},
 r=>{const result=validationResults(r)[0],s=result.solution;s.measurements=[];s.violations=[];s.uncertain_count=0;result.violations.items=[];result.measurements.data.rows=[];result.measurements.data.confirmedReportCount=0;},
 ];
 for(const mutate of corruptions){const copy=structuredClone(output);mutate(copy);const result=validationResults(copy)[0],s=result.solution;result.measurements.data.rows=s.measurements;result.measurements.data.coverage=s.coverage;result.measurements.data.confirmedReportCount=s.violations.length;result.violations.items=s.violations;copy.reports[0].plots[0].data=result.measurements;assert.throws(()=>auditValidation(copy,b,e));}
});
