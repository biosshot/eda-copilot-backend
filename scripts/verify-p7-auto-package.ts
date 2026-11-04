import assert from 'node:assert/strict';
import {createHash} from 'node:crypto';
import {mkdirSync,writeFileSync} from 'node:fs';
import {createRequire} from 'node:module';
import {join} from 'node:path';
import type {AnalysisBoardFoilCheckedRequest} from '../native/analyze/index.js';
import {validateBoardFoilReport} from '../src/analyze/board-foil-validation.ts';
import {channelPassiveElement} from '../src/analyze/channel-components.ts';
import {capturedBoardGeometry} from '../src/analyze/geometry-snapshot.ts';
import {localCoreIdentity} from '../src/analyze/local-model-cache.ts';
import {isAnalysisProblem,type BoardFoilProblem} from '../src/analyze/types.ts';
import {segmentedCoupon} from '../tests/helpers/pcb-analysis-segmented-coupon.ts';

const folder=process.env.P7_OUTPUT_DIR;
if(!folder)throw Error('Run this smoke under the P7 process-tree supervisor');
mkdirSync(folder,{recursive:true});
const write=(name:string,value:unknown)=>writeFileSync(join(folder,name),JSON.stringify(value,null,2)+'\n');
const hash=(text:string)=>createHash('sha256').update(text).digest('hex');
const budget={maxSeconds:1800,maxMemoryMiB:3072};
process.env.P7_VERIFY_SECONDS=String(budget.maxSeconds);
process.env.P7_VERIFY_MEMORY_MIB=String(budget.maxMemoryMiB);

// Exact copper/stack geometry of the passing 42-tet response control in
// native/analyze/tests/foil_reduced.rs::mixed_thickness_board_checks_shared_matched_response_in_original_coordinates.
// Expose the input pair as the sole external port. The opposite original pair
// is reserved for the explicit package, with both physical pads retained.
const board=segmentedCoupon();
const points=(xy:number[][])=>xy.map(([x,y])=>({x,y}));
board.outline=points([[-1.25,-1.25],[1.25,-1.25],[1.25,1.25],[-1.25,1.25]]);
board.copper.fixed.tracks=[];
const strip=points([[-1,-.25],[1,-.25],[1,.25],[-1,.25]]);
const island=points([[.25,.375],[.5,.375],[.5,.625],[.25,.625]]);
function zone(id:string,layer:string,net:string,outer:{x:number;y:number}[],reason:string){
 const polygon={outer,holes:[]};
 return {id,kind:'pour',net,layers:[layer],outline:polygon,priority:1,preserveIslands:true,
  refillRules:{status:'unknown',unresolved:[reason]},
  filledPolygons:[{layer,source:'cad',status:'available',freshness:'unknown',polygons:[polygon]}]};
}
board.copper.fixed.zones=[
 zone('signal-foil','TOP','SIGNAL',strip,'Authored finite foil control'),
 zone('return-foil','BOTTOM','RETURN',strip,'Authored finite foil control'),
 zone('floating','TOP','SIGNAL',island,'Authored floating foil control'),
];
for(const [i,pad] of board.pads.entries()){
 pad.at={x:i%2===0?-.875:.875,y:0};
 pad.shape={kind:'rect',widthMm:.25,heightMm:.5};
}
const boardJson=JSON.stringify(board);
const ports:[string,string][]=[['p0','p2']];
const geometry={
 roiMm:[[-1.25,-1.25],[1.25,1.25]],
 stack:[
  {name:'TOP',thicknessM:5e-6,copper:true,epsilonR:null,conductivitySM:58e6},
  {name:'dielectric',thicknessM:1e-4,copper:false,epsilonR:4,conductivitySM:null},
  {name:'BOTTOM',thicknessM:1e-4,copper:true,epsilonR:null,conductivitySM:58e6},
 ],
 airMarginM:.0005,maxTriangleAreaM2:5e-6,maxVerticalStepM:.00025,airEdgeDivisions:1,
 minTriangleAngleDeg:0,curveDeviationMm:.001,acceptUnknownFillFreshness:true,allowInsulatedCopperCuts:false,
 electrodes:['TOP','TOP','BOTTOM','BOTTOM'].map((layer,i)=>({name:'p'+i,sourcePath:'pads['+i+']',layer,upper:layer==='BOTTOM'})),
 ports,
};
const resistanceOhm=47;
const attachment={
 id:'package:R1',electrodes:['p1','p3'] as [string,string],
 model:channelPassiveElement('package:R1',{kind:'resistor',resistanceOhm},[1000,10000]),
};
const request:AnalysisBoardFoilCheckedRequest={
 frequenciesHz:[1000],referenceOhm:[50],budget,maxIterations:16,attachments:[attachment],
};
const config={fixture:'native compact mixed-thickness 42-copper-tetrahedra control',
 scope:'Existing copper and physical pad geometry; one external pair and one private finite package pair',
 budget,treePrivateCapMiB:4096,node:process.version,boardSha256:hash(boardJson),
 geometrySha256:hash(JSON.stringify(geometry)),attachmentsSha256:hash(JSON.stringify(request.attachments))};
const start=performance.now();
writeFileSync(join(folder,'board.json'),boardJson);
write('geometry.json',geometry);
write('request.json',request);
write('config.json',config);
try{
 write('config.json',{...config,coreIdentity:localCoreIdentity()});
 const captured=capturedBoardGeometry(boardJson);
 assert.notEqual(captured.status,'invalid');
 assert.equal(captured.excluded.length,0);
 assert.ok(captured.fills.every(fill=>fill.status==='available'));
 assert.equal(captured.holes.length,0);
 assert.equal(captured.regions.length,7);
 const sourceCopper=captured.regions.map(({id,source,layer})=>({regionId:id,sourcePath:source.path,layer}));
 const problem:BoardFoilProblem={kind:'board-foil-checked',boardJson,geometry,
  expected:{electrodePairs:ports,sourceCopper,sourceBarrels:[]},request};
 assert.ok(isAnalysisProblem(problem));
 write('expected.json',problem.expected);
 const addon=createRequire(import.meta.url)('../native/node-addon/index.cjs') as typeof import('../native/node-addon/index.cjs');
 const session=new addon.AnalysisElectricalSession();
 const nativeStart=performance.now();
 const report=session.solveBoardFoilChecked(boardJson,geometry,request);
 const nativeSeconds=(performance.now()-nativeStart)/1000;
 write('report.json',report);
 validateBoardFoilReport(report,problem,budget);
 if(report.provenance.modelKind!=='mixed')throw Error('Expected mixed native model');
 assert.equal(report.provenance.nativeCopperTetrahedra,42);
 assert.ok(report.provenance.prisms>0&&report.provenance.volumeTetrahedra>0);
 assert.ok(report.provenance.sourceMapping.some(([,path])=>path.includes('zones[2]')));
 assert.deepEqual(report.ports,ports);
 assert.deepEqual(Object.keys(report.provenance.electrodeAreasM2).sort(),['p0','p1','p2','p3']);
 assert.deepEqual(report.provenance.attachments?.map(a=>[a.id,...a.electrodes]),[['package:R1','p1','p3']]);
 const column=report.samples[0].columns[0];
 assert.equal(column.packages?.length,1);
 const pkg=column.packages![0];
 assert.ok(column.copperLossW>0&&pkg.lossW>0&&Math.hypot(pkg.currentA.real,pkg.currentA.imaginary)>0);
 const expectedLoss=resistanceOhm*(pkg.currentA.real**2+pkg.currentA.imaginary**2);
 assert.ok(Math.abs(expectedLoss-pkg.lossW)<=1e-12+1e-8*(expectedLoss+pkg.lossW));
 const {real,imaginary}=report.parameters[0][0][0];
 const wavePowerW=12.5*(1-real*real-imaginary*imaginary);
 const lossW=column.copperLossW+pkg.lossW;
 assert.ok(Math.abs(wavePowerW-lossW)<=1e-12+1e-8*(Math.abs(wavePowerW)+lossW));
 write('error.json',null);
 write('comparison.json',{status:'pass',nativeSeconds,elapsedSeconds:(performance.now()-start)/1000,externalPorts:report.ports,
  privatePackage:pkg,copperLossW:column.copperLossW,wavePowerW,quality:report.quality,
  nativeCopperTetrahedra:report.provenance.nativeCopperTetrahedra,prisms:report.provenance.prisms,
  volumeTetrahedra:report.provenance.volumeTetrahedra,plannedBytes:report.provenance.plannedBytes,physicalCoverage:'unverified'});
}catch(error){
 const detail=error instanceof Error?{name:error.name,message:error.message,stack:error.stack}:String(error);
 write('error.json',detail);
 write('comparison.json',{status:'validation-failure',elapsedSeconds:(performance.now()-start)/1000,error:detail});
 console.error(error);
 process.exitCode=1;
}
