import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { parseEasyEdaFootprintDataStr } from '../src/devices/footprints/easyeda-footprint.ts';
import { componentCollisionBoxes, componentPairCollisionBoxPairs } from '../src/pcb-layout/pcb-auto-place/geometry.ts';
import type { PcbComponent, Placement } from '../src/types/pcb/layout-model.ts';

describe('easyeda footprint parser', () => {
    const testPad = '["PAD","p",0,"",1,"1",0,0,0,null,["RECT",10,10,0],[],0,0,0,1,0,null,null,null,null,0]';
    const close = (actual: number, expected: number) => assert.ok(Math.abs(actual - expected) < 0.0002, `${actual} != ${expected}`);

    it('keeps the real 10mm capacitor arc body and its collision area at every quarter turn', () => {
        const footprint = parseEasyEdaFootprintDataStr([
            '["DOCTYPE","FOOTPRINT"]',
            '["POLY","body",0,"",48,2,[-196.86,0,"ARC",-180,196.84,0,"ARC",-180,-196.86,0],0]',
            '["POLY","silk",0,"",3,10,["CIRCLE",-0.005,0,196.85],0]',
            '["FILL","polarity",0,"",3,0.2,0,[[139.99,140,"L",139.99,-140,169.99,-100,189.99,-40,199.99,20,189.99,60,139.99,140]],0]',
            '["PAD","p1",0,"",12,"1",-98.425,0,0,["ROUND",39.37,39.37],["ELLIPSE",62.992,62.992],[],0,0,0,1,0,null,null,null,null,0]',
            '["PAD","p2",0,"",12,"2",98.425,0,0,["ROUND",39.37,39.37],["ELLIPSE",62.992,62.992],[],0,0,0,1,0,null,null,null,null,0]',
        ].join('\n'));
        close(footprint.width, 10.0799);
        close(footprint.height, 10);
        assert.ok(footprint.bodyBox);
        const component = { footprint, pcb: {} } as PcbComponent;
        const small = { footprint: parseEasyEdaFootprintDataStr(testPad), pcb: {} } as PcbComponent;
        for (const rotate of [0, 90, 180, 270]) for (const layer of ['top', 'bottom'] as const) {
            const pose = { x: 0, y: 0, rotate, layer, score: 0 } as Placement;
            const neighbor = { ...pose, x: 0, y: 4 };
            assert.equal(componentPairCollisionBoxPairs(component, pose, small, neighbor).some(({ a, b }) =>
                a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top), true);
            assert.equal(componentCollisionBoxes(component, pose, layer === 'top' ? 'bottom' : 'top').length, 2);
        }
    });

    it('bounds every documented contour form, curve extrema and nested contours', () => {
        const cases: Array<{ name: string; path: unknown[]; width: number; height: number }> = [
            { name: 'numeric polygon', path: [-100,-50,100,-50,100,50,-100,50,-100,-50], width: 200, height: 100 },
            { name: 'L', path: [-100,-50,'L',100,-50,100,50,-100,50,-100,-50], width: 200, height: 100 },
            { name: 'R 0', path: ['R',-100,50,200,100,0,0], width: 200, height: 100 },
            { name: 'rounded R 90', path: ['R',-100,50,200,100,90,0,20], width: 100, height: 200 },
            { name: 'R 45', path: ['R',-100,50,200,100,45,0], width: 300 / Math.sqrt(2), height: 300 / Math.sqrt(2) },
            { name: 'CIRCLE', path: ['CIRCLE',500,300,100,1], width: 200, height: 200 },
            { name: 'nested circle', path: [['CIRCLE',500,300,100,1]], width: 200, height: 200 },
            { name: 'multiple circles', path: [['CIRCLE',0,0,100], ['CIRCLE',300,0,50]], width: 450, height: 200 },
            { name: 'positive semicircle', path: [-100,0,'ARC',180,100,0], width: 200, height: 100 },
            { name: 'negative semicircle', path: [-100,0,'ARC',-180,100,0], width: 200, height: 100 },
            { name: 'CARC', path: [-100,0,'CARC',-180,100,0], width: 200, height: 100 },
            { name: 'major arc', path: [100,0,'ARC',270,0,-100], width: 200, height: 200 },
            { name: 'negative major arc', path: [100,0,'CARC',-270,0,100], width: 200, height: 200 },
            { name: 'quarter arc', path: [100,0,'ARC',90,0,100], width: 100, height: 100 },
            { name: 'zero arc', path: [-100,0,'ARC',0,100,0], width: 200, height: 0 },
            { name: 'cubic only', path: [0,0,'C',0,100,100,100,100,0], width: 100, height: 75 },
            { name: 'cubic repeated', path: [0,0,'C',0,100,100,100,100,0,100,-100,200,-100,200,0], width: 200, height: 150 },
            { name: 'quadratic', path: [0,0,'Q',50,100,100,0], width: 100, height: 50 },
            { name: 'nested mixed', path: [[-100,0,'CARC',-180,100,0], ['CIRCLE',300,0,50]], width: 450, height: 150 },
        ];
        for (const { name, path, width, height } of cases) {
            const footprint = parseEasyEdaFootprintDataStr([testPad, JSON.stringify(['FILL',name,0,'',13,1,0,path,0])].join('\n'));
            const graphic = footprint.graphics?.find(g => g.layer === 'document');
            assert.ok(graphic, name);
            if (graphic.kind === 'circle') {
                close(graphic.radius * 2, width * 0.0254);
                close(graphic.radius * 2, height * 0.0254);
            } else {
                close(Math.max(...graphic.points.map(p => p.x)) - Math.min(...graphic.points.map(p => p.x)), width * 0.0254);
                close(Math.max(...graphic.points.map(p => p.y)) - Math.min(...graphic.points.map(p => p.y)), height * 0.0254);
            }
        }
    });

    it('preserves direction and includes extrema of off-axis arcs and Beziers', () => {
        for (const angle of [-300, -180, -60, 60, 180, 300]) {
            const start = 35 * Math.PI / 180, sweep = angle * Math.PI / 180;
            const p = (t: number) => ({ x: 300 + 100 * Math.cos(start + sweep * t), y: 200 + 100 * Math.sin(start + sweep * t) });
            const first = p(0), last = p(1);
            const path = [first.x, first.y, 'ARC', angle, last.x, last.y];
            const fp = parseEasyEdaFootprintDataStr([testPad, JSON.stringify(['POLY','arc',0,'',48,1,path,0])].join('\n'));
            assert.ok(fp.bodyBox);
            for (let step = 0; step <= 1000; step++) {
                const point = p(step / 1000);
                const x = fp.sourceOriginOffset!.x - point.x * 0.0254;
                const y = point.y * 0.0254 + fp.sourceOriginOffset!.y;
                assert.ok(x >= fp.bodyBox.left - 0.0002 && x <= fp.bodyBox.right + 0.0002, `${angle}: x`);
                assert.ok(y >= fp.bodyBox.top - 0.0002 && y <= fp.bodyBox.bottom + 0.0002, `${angle}: y`);
            }
        }
        // Extremum at t=1/3, not one of the rendering sample positions.
        const fp = parseEasyEdaFootprintDataStr([testPad, '["POLY","curve",0,"",13,1,[0,0,"C",0,300,100,0,100,0],0]'].join('\n'));
        const curve = fp.graphics?.find(g => g.layer === 'document');
        assert.ok(curve && curve.kind === 'path');
        close(Math.max(...curve.points.map(p => p.y)) - Math.min(...curve.points.map(p => p.y)), 400 / 3 * 0.0254);
    });

    it('retains rectangular envelopes for all pad forms and rotations', () => {
        for (const shape of [['RECT',100,40,0], ['ROUND',100,40], ['OVAL',100,40], ['ELLIPSE',100,40], ['CIRCLE',100], ['NGON',100,6], ['POLY',[-50,-20,'L',50,-20,50,20,-50,20,-50,-20],[]]]) {
            for (const rotate of [0,45,90,180,270]) {
                const fp = parseEasyEdaFootprintDataStr(JSON.stringify(['PAD','p',0,'',1,'1',0,0,rotate,null,shape,[],0,0,0,1,0,null,null,null,null,0]));
                assert.equal(fp.pads.length, 1, String(shape[0]));
                const radians = rotate * Math.PI / 180;
                const width = 100, height = ['CIRCLE','NGON'].includes(String(shape[0])) ? 100 : 40;
                close(fp.pads[0].width, (width * Math.abs(Math.cos(radians)) + height * Math.abs(Math.sin(radians))) * 0.0254);
                close(fp.pads[0].height, (width * Math.abs(Math.sin(radians)) + height * Math.abs(Math.cos(radians))) * 0.0254);
            }
        }
    });

    it('centers asymmetric polygon pad envelopes on their actual geometry', () => {
        const fp = parseEasyEdaFootprintDataStr('["PAD","p",0,"",1,"1",0,0,0,null,["POLY",[0,-20,"L",100,-20,100,20,0,20,0,-20],[]],[],0,0,0,1,0,null,null,null,null,0]');
        close(fp.pads[0].x, 0);
        close(fp.pads[0].y, 0);
        close(fp.sourceOriginOffset!.x, 1.27);
        const component = { footprint: fp, pcb: {} } as PcbComponent;
        const pose = { x: 0, y: 0, rotate: 0, layer: 'top', score: 0 } as Placement;
        const boxes = componentCollisionBoxes(component, pose, 'top');
        assert.ok(boxes.every(box => box.left >= -fp.width / 2 - 0.0002 && box.right <= fp.width / 2 + 0.0002));
    });

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
