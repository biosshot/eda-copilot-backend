// Controlled ablation: replace only USB support poses in the baseline board.
// This is explicitly not an output of global board packing.
import {readFileSync,writeFileSync} from 'node:fs';
import {createPlacementReport} from '../src/pcb-layout/pcb-auto-place/placement-report.ts';
import {createPcbLayout} from '../src/pcb-layout/pcb-auto-place/layout.ts';
import {createBoardAssemble} from '../src/pcb-layout/board-assemble.ts';
import {renderPlacementSvg} from '../src/pcb-layout/pcb-auto-place/render.ts';
import {encodeNativePostPlaceRefineProblem} from '../src/pcb-layout/pcb-auto-place-v2/native/encode-post-place-refine.ts';
import {loadNativeBoardPacker} from '../src/pcb-layout/pcb-auto-place-v2/native/load-native-board-packer.ts';
import {placementMetrics} from './experiment-placement-metrics.mjs';
const dir='docs/experimental/pcb/telemetry-anchored-2026-09-27';
const before=JSON.parse(readFileSync(`${dir}/before.json`)),after=JSON.parse(readFileSync(`${dir}/after-final.json`));
const input=JSON.parse(readFileSync('docs/experimental/pcb/global-placement-2026-09-27/Telemetry/input.json'));
const movable=new Set(['R30','R31','F1','C41']);
const placements=before.placements.map(p=>movable.has(p.designator)?after.placements.find(q=>q.designator===p.designator):p);
const report=createPlacementReport(input,placements);
const validChange=loadNativeBoardPacker().validatePlacementChange(encodeNativePostPlaceRefineProblem(input,before.placements,1),placements);
const metrics=placementMetrics(input,placements);
writeFileSync(`${dir}/usb-control.json`,JSON.stringify({description:'Controlled replacement of four USB components; all other poses are baseline. Not a board-packer result.',placements,report,validChange,metrics},null,2));
if(report.ok&&validChange){
    const asm=createBoardAssemble(createPcbLayout(input,placements),{preserveBoard:true,preservedComponents:new Set(input.components.filter(c=>!movable.has(c.designator)).map(c=>c.designator))});
    writeFileSync(`${dir}/usb-control.assemble.json`,JSON.stringify({components:asm.components},null,2));
    writeFileSync(`${dir}/usb-control-board.svg`,renderPlacementSvg(input,placements,{ratsnestTopology:'mst',signalPaths:false}));
}
console.log(JSON.stringify({ok:report.ok,validChange,metrics,overlaps:report.overlaps},null,2));
