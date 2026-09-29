import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { parseEasyEdaFootprintDataStr } from '../src/devices/footprints/easyeda-footprint.ts';
import { componentCollisionBoxes, componentPairCollisionBoxPairs } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import type { PcbComponent, Placement } from '../src/types/pcb/layout-model.ts';

describe('easyeda footprint parser', () => {
    it('keeps the PortableScope U6 display outline and mounting holes at their real size', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT","1.7"]',
            '["PAD","e2",0,"",12,"1",0,470,-90,["ROUND",36,36],["ELLIPSE",60,60],[],0,0,0,1,0,null,null,null,null,0]',
            '["PAD","e15",0,"",12,"14",0,-553.6226,-90,["ROUND",36,36],["ELLIPSE",60,60],[],0,0,0,1,0,null,null,null,null,0]',
            '["POLY","e1",0,"",3,10,["CIRCLE",39.3701,1078.7402,62.9921],0]',
            '["POLY","e16",0,"",3,10,["CIRCLE",4055.1182,1078.7402,62.9921],0]',
            '["POLY","e17",0,"",3,10,["CIRCLE",4055.1182,-1082.6772,62.9921],0]',
            '["POLY","e18",0,"",3,10,["CIRCLE",39.3701,-1082.6772,62.9921],0]',
            '["POLY","e38",0,"",3,10,["R",-78.7402,1198.4252,4251.9685,2396.8504,0,0],0]',
        ].join('\n');
        const footprint = parseEasyEdaFootprintDataStr(dataStr);
        assert.equal(footprint.width, 108);
        assert.equal(footprint.height, 60.88);
        assert.equal(footprint.graphics?.filter((graphic) => graphic.kind === 'circle' && graphic.side === 'top').length, 4);
        assert.equal(footprint.graphics?.some((graphic) => graphic.kind === 'path' && graphic.closed && graphic.side === 'top'), true);
        assert.equal(footprint.pads.every((pad) => pad.mount === 'through_hole' && pad.layer === 'multi'), true);
    });

    it('preserves bottom silk and bottom SMD pads as opposite-side geometry', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT"]',
            '["PAD","top",0,"",1,"1",0,0,0,null,["RECT",20,20,0],[],0,0,0,1,0,null,null,null,null,0]',
            '["PAD","bottom",0,"",2,"2",100,0,0,null,["RECT",20,20,0],[],0,0,0,1,0,null,null,null,null,0]',
            '["POLY","silk",0,"",4,10,["R",70,30,60,60,0,0],0]',
        ].join('\n');
        const footprint = parseEasyEdaFootprintDataStr(dataStr);
        assert.equal(footprint.pads.find((pad) => pad.pin_number === '2')?.layer, 'bottom');
        assert.equal(footprint.graphics?.find((graphic) => graphic.layer === 'silk')?.side, 'bottom');
    });
    it('rotates R rectangles and never treats circle radius as a coordinate', () => {
        const footprint = parseEasyEdaFootprintDataStr([
            '["DOCTYPE","FOOTPRINT"]',
            '["PAD","p",0,"",1,"1",0,0,0,null,["RECT",20,20,0],[],0,0,0,1,0,null,null,null,null,0]',
            '["POLY","rotated",0,"",3,10,["R",-100,50,200,100,90,0],0]',
            '["FILL","circle",0,"",13,0.2,0,[["CIRCLE",1000,1000,10]],0]',
        ].join('\n'));
        const rectangle = footprint.graphics?.find((graphic) => graphic.kind === 'path' && graphic.layer === 'silk');
        assert.ok(rectangle && rectangle.kind === 'path');
        const xs = rectangle.points.map((point) => point.x);
        const ys = rectangle.points.map((point) => point.y);
        assert.ok(Math.abs(Math.max(...xs) - Math.min(...xs) - 2.54) < 0.001);
        assert.ok(Math.abs(Math.max(...ys) - Math.min(...ys) - 5.08) < 0.001);
        assert.equal(footprint.graphics?.some((graphic) => graphic.kind === 'path' && graphic.layer === 'document'), false);
    });

    it('leaves a top-mounted display center available on bottom but blocks its drilled pads', () => {
        const display = parseEasyEdaFootprintDataStr([
            '["DOCTYPE","FOOTPRINT"]',
            '["PAD","hole",0,"",12,"1",0,0,0,["ROUND",36,36],["ELLIPSE",60,60],[],0,0,0,1,0,null,null,null,null,0]',
            '["POLY","outline",0,"",3,10,["R",-200,200,400,400,0,0],0]',
        ].join('\n'));
        const passive = parseEasyEdaFootprintDataStr([
            '["DOCTYPE","FOOTPRINT"]',
            '["PAD","smd",0,"",1,"1",0,0,0,null,["RECT",20,20,0],[],0,0,0,1,0,null,null,null,null,0]',
        ].join('\n'));
        const u6 = { footprint: display, pcb: {} } as PcbComponent;
        const r1 = { footprint: passive, pcb: {} } as PcbComponent;
        const top = { x: 0, y: 0, rotate: 0, layer: 'top', score: 0 } as Placement;
        const bottom = { x: 4, y: 0, rotate: 0, layer: 'bottom', score: 0 } as Placement;
        const bottomBoxes = componentCollisionBoxes(u6, top, 'bottom');
        assert.equal(bottomBoxes.length, 1);
        assert.equal(componentPairCollisionBoxPairs(u6, top, r1, bottom).some(({ a, b }) =>
            a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top), false);
        const onHole = { ...bottom, x: 0 };
        assert.equal(componentPairCollisionBoxPairs(u6, top, r1, onHole).some(({ a, b }) =>
            a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top), true);
    });
    it('flips explicit source-side body areas when the component mounts on bottom', () => {
        const footprint = parseEasyEdaFootprintDataStr([
            '["DOCTYPE","FOOTPRINT"]',
            '["PAD","hole",0,"",12,"1",0,0,0,["ROUND",36,36],["ELLIPSE",60,60],[],0,0,0,1,0,null,null,null,null,0]',
            '["POLY","outline",0,"",3,10,["R",-200,200,400,400,0,0],0]',
        ].join('\n'));
        const component = { footprint, pcb: { occupiedAreas: { top: [], bottom: [{ left: 3, right: 5, top: -1, bottom: 1 }] } } } as PcbComponent;
        const top = { x: 0, y: 0, rotate: 0, layer: 'top', score: 0 } as Placement;
        const bottom = { ...top, layer: 'bottom' as const };
        assert.equal(componentCollisionBoxes(component, top, 'top').some((box) => box.left <= -4), false);
        assert.equal(componentCollisionBoxes(component, top, 'bottom').some((box) => box.left === 3 && box.right === 5), true);
        assert.equal(componentCollisionBoxes(component, bottom, 'top').some((box) => box.left === -5 && box.right === -3), true);
    });
    it('parses polygon pads such as exposed thermal pads', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT"]',
            '["ATTR",0,0,"Name","WSON-10_TEST"]',
            '["PAD","e1",0,"",1,"11",0,0,0,null,["POLY",[-50,-25,"L",50,-25,50,25,-50,25,-50,-25],[]],[],0,0,0,1,0,null,null,null,null,0]',
        ].join('\n');

        const footprint = parseEasyEdaFootprintDataStr(dataStr);
        const pad = footprint.pads.find((item) => String(item.pin_number) === '11');

        assert.equal(footprint.pads.length, 1);
        assert.ok(pad);
        assert.equal(pad.width, 2.54);
        assert.equal(pad.height, 1.27);
    });

    it('marks EasyEDA layer-12 drilled pads as through-hole', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT"]',
            '["ATTR",0,0,"Name","CONN-TH_TEST"]',
            '["PAD","e16",0,"",12,"1",50,0,180,["ROUND",47.244,47.244],["RECT",70.866,70.866,0],[],0,0,0,1,0,null,null,null,null,0]',
        ].join('\n');

        const footprint = parseEasyEdaFootprintDataStr(dataStr);
        const pad = footprint.pads[0];

        assert.equal(pad.mount, 'through_hole');
        assert.equal(pad.drillDiameter, 1.2);
        assert.equal(pad.width, 1.8);
        assert.equal(pad.height, 1.8);
    });

    it('converts EasyEDA layer-12 circular mechanical holes to through-hole pads', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT","1.8"]',
            '["ATTR",0,0,"Name","USB-TYPE-C-SMD_TEST"]',
            '["PAD","e12",0,"",1,"A1",0,0,0,null,["RECT",20,40,0],[],0,0,0,1,0,null,null,null,null,0]',
            '["POLY","e2",0,"",13,9.843,["CIRCLE",-113.785,46.36,4.92],0]',
            '["FILL","e44",0,"",12,0.2,0,["CIRCLE",-113.785,46.36,12.795],0]',
            '["FILL","e45",0,"",12,0.2,0,["CIRCLE",113.775,46.36,12.795],0]',
        ].join('\n');

        const footprint = parseEasyEdaFootprintDataStr(dataStr);
        const holes = footprint.pads.filter((pad) => String(pad.pin_number).startsWith('MH'));

        assert.equal(holes.length, 2);
        assert.deepEqual(holes.map((pad) => pad.mount), ['through_hole', 'through_hole']);
        assert.deepEqual(holes.map((pad) => pad.drillDiameter), [0.65, 0.65]);
        assert.equal(footprint.graphics?.some((graphic) => graphic.layer === 'document'), true);
    });

    it('converts EasyEDA footprint vias to through-hole physical pads', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT","1.8"]',
            '["ATTR",0,0,"Name","QFN_WITH_EP_VIAS"]',
            '["PAD","e1",0,"",1,"1",0,0,0,null,["RECT",20,20,0],[],0,0,0,1,0,null,null,null,null,0]',
            '["VIA","v1",0,"GND",50,-50,23.622,11.811,0]',
        ].join('\n');

        const footprint = parseEasyEdaFootprintDataStr(dataStr);
        const via = footprint.pads.find((pad) => pad.pin_number === 'FV1');

        assert.ok(via);
        assert.equal(via.mount, 'through_hole');
        assert.equal(via.width, 0.6);
        assert.equal(via.height, 0.6);
        assert.equal(via.drillDiameter, 0.3);
    });

    it('converts EasyEDA footprint vias when drill and diameter are swapped', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT","1.8"]',
            '["ATTR",0,0,"Name","RP2040_QFN_WITH_EP_VIAS"]',
            '["PAD","e126",0,"",1,"57",0.005,0.005,0,null,["RECT",157.48,157.48,0],[],-0.002,-0.002,0,1,0,2,2,-3937,-3937,0]',
            '["VIA","e184",0,"","",39.375,-0.005,12,24,0,null,null,0]',
            '["VIA","e185",0,"","",39.375,39.365,12,24,0,null,null,0]',
            '["VIA","e186",0,"","",-39.355,0.005,12,24,0,null,null,0]',
            '["VIA","e187",0,"","",-39.355,39.365,12,24,0,null,null,0]',
            '["VIA","e188",0,"","",39.375,-39.375,12,24,0,null,null,0]',
            '["VIA","e189",0,"","",-39.365,-39.375,12,24,0,null,null,0]',
            '["VIA","e190",0,"","",0.005,-0.005,12,24,0,null,null,0]',
            '["VIA","e191",0,"","",0.025,-39.355,12,24,0,null,null,0]',
            '["VIA","e192",0,"","",0.005,39.365,12,24,0,null,null,0]',
        ].join('\n');

        const footprint = parseEasyEdaFootprintDataStr(dataStr);
        const vias = footprint.pads.filter((pad) => String(pad.pin_number).startsWith('FV'));

        assert.equal(vias.length, 9);
        assert.ok(vias.every((via) => via.mount === 'through_hole'));
        assert.ok(vias.every((via) => via.width === 0.6096 && via.height === 0.6096));
        assert.ok(vias.every((via) => via.drillDiameter === 0.3048));
    });

    it('adds a small placement margin around pad-only footprints', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT"]',
            '["ATTR",0,0,"Name","PAD_ONLY_TEST"]',
            '["PAD","e1",0,"",1,"1",0,0,0,null,["RECT",20,10,0],[],0,0,0,1,0,null,null,null,null,0]',
        ].join('\n');

        const footprint = parseEasyEdaFootprintDataStr(dataStr);
        const pad = footprint.pads[0];

        assert.equal(pad.width, 0.508);
        assert.equal(pad.height, 0.254);
        assert.equal(footprint.width, 1.016);
        assert.equal(footprint.height, 0.762);
    });

    it('keeps pad margin on axes not covered by partial silk graphics', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT"]',
            '["ATTR",0,0,"Name","PARTIAL_SILK_TEST"]',
            '["PAD","e1",0,"",1,"1",0,0,0,null,["RECT",100,100,0],[],0,0,0,1,0,null,null,null,null,0]',
            '["FILL","silk",0,"",3,0.2,0,[-80,-50,"L",80,-50,80,50,-80,50,-80,-50],0]',
        ].join('\n');

        const footprint = parseEasyEdaFootprintDataStr(dataStr);

        assert.equal(footprint.width, 4.064);
        assert.equal(footprint.height, 3.048);
    });

    it('mirrors EasyEDA pad X and ignores remote silk markers in footprint bbox', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT"]',
            '["ATTR",0,0,"Name","QFN_TEST"]',
            '["PAD","e1",0,"",1,"1",-100,0,0,null,["RECT",20,10,0],[],0,0,0,1,0,null,null,null,null,0]',
            '["PAD","e2",0,"",1,"2",100,0,0,null,["RECT",20,10,0],[],0,0,0,1,0,null,null,null,null,0]',
            '["FILL","body",0,"",48,0.2,0,[-120,-120,"L",120,-120,120,120,-120,120,-120,-120],0]',
            '["FILL","pin1_marker",0,"",3,0.2,0,[-220,-20,"L",-180,-20,-180,20,-220,20,-220,-20],0]',
        ].join('\n');

        const footprint = parseEasyEdaFootprintDataStr(dataStr);
        const pad1 = footprint.pads.find((pad) => pad.pin_number === '1');
        const pad2 = footprint.pads.find((pad) => pad.pin_number === '2');

        assert.ok(pad1);
        assert.ok(pad2);
        assert.equal(pad1.x, 2.54);
        assert.equal(pad2.x, -2.54);
        assert.equal(footprint.width, 6.096);
        assert.equal(footprint.height, 6.096);
        assert.deepEqual(footprint.sourceOriginOffset, { x: 0, y: 0 });
    });

    it('ignores large micro-stroke closed helper polygons in footprint graphics and bbox', () => {
        const dataStr = [
            '["DOCTYPE","FOOTPRINT"]',
            '["ATTR",0,0,"Name","CONNECTOR_WITH_HELPER_TRIANGLE"]',
            '["PAD","p1",0,"",1,"1",0,0,0,null,["RECT",20,20,0],[],0,0,0,1,0,null,null,null,null,0]',
            '["FILL","body",0,"",48,0.2,0,[-50,-40,"L",50,-40,50,40,-50,40,-50,-40],0]',
            '["FILL","silk",0,"",3,10,0,[-60,-50,"L",60,-50],0]',
            '["FILL","bad_silk_triangle",0,"",3,0.2,0,[-500,-500,"L",500,-500,0,500,-500,-500],0]',
            '["POLY","bad_doc_triangle",0,"",13,0.2,[-500,-500,"L",500,-500,0,500,-500,-500],0]',
        ].join('\n');

        const footprint = parseEasyEdaFootprintDataStr(dataStr);

        assert.equal(footprint.width < 4, true);
        assert.equal(footprint.height < 4, true);
        assert.equal(footprint.graphics?.some((graphic) => (
            graphic.kind === 'path'
            && graphic.closed
            && graphic.strokeWidth <= 0.01
            && (graphic.layer === 'silk' || graphic.layer === 'document')
        )), false);
    });
});
