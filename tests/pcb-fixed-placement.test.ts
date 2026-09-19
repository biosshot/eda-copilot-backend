import assert from 'node:assert/strict';
import test from 'node:test';
import { autoPlacePcbWithReport } from '../src/pcb-layout/pcb-auto-place/auto-place.ts';
import { buildPlacementInput } from '../src/pcb-layout/placement-input.ts';
import { validatePlacementRulesForCircuit } from '../src/pcb-layout/placement-validation.ts';
import { runPcbLayoutDsl } from '../src/pcb-layout/pcb-layout-dsl/spec.ts';
import { createPcbToolReport } from '../src/pcb-layout/report.ts';
import type { ExplainCircuit } from '../src/types/circuit.ts';
import type { FootprintSpec } from '../src/types/pcb/layout-model.ts';

const PART_UUID = '11111111111111111111111111111111';
const FOOTPRINT: FootprintSpec = {
    name: 'TEST_2PAD',
    width: 2,
    height: 1,
    pads: [
        { pin_number: '1', name: '1', x: -0.5, y: 0, width: 0.4, height: 0.4 },
        { pin_number: '2', name: '2', x: 0.5, y: 0, width: 0.4, height: 0.4 },
    ],
};

function singleComponentCircuit(designator: string, value: string): ExplainCircuit {
    return {
        components: [{
            designator,
            value,
            pins: [
                { pin_number: '1', name: '1', signal_name: 'NET' },
                { pin_number: '2', name: '2', signal_name: 'GND' },
            ],
            part_uuid: PART_UUID,
            footprint_name: FOOTPRINT.name,
        }],
    };
}

function fixedRules(designator: string, role: 'passive' | 'main_ic' | 'connector') {
    return runPcbLayoutDsl(`
        board.rect(20, 12);
        block("main", ["${designator}"], "generic");
        component("${designator}").role("${role}").fixed({ x: 2.5, y: -1.5, rotate: 90, layer: "top" });
    `);
}

test('allows passive components to use fixed()', () => {
    const circuit = singleComponentCircuit('R1', '10k');
    assert.doesNotThrow(() => validatePlacementRulesForCircuit(circuit, fixedRules('R1', 'passive')));
});

test('allows main_ic components to use fixed()', () => {
    const circuit = singleComponentCircuit('U1', 'MCU');
    assert.doesNotThrow(() => validatePlacementRulesForCircuit(circuit, fixedRules('U1', 'main_ic')));
});

test('keeps connector fixed() behavior accepted', () => {
    const circuit = singleComponentCircuit('J1', 'USB-C');
    assert.doesNotThrow(() => validatePlacementRulesForCircuit(circuit, fixedRules('J1', 'connector')));
});

test('reports one aggregate warning when fixed() is used for passive components', async () => {
    const circuit: ExplainCircuit = {
        components: [
            ...singleComponentCircuit('R1', '10k').components,
            ...singleComponentCircuit('C1', '100nF').components,
        ],
    };
    const rules = runPcbLayoutDsl(`
        board.rect(20, 12);
        block("passives", ["R1", "C1"], "generic");
        component("R1").role("passive").fixed({ x: -2, y: 0, rotate: 0, layer: "top" });
        component("C1").role("decoupling_cap").fixed({ x: 2, y: 0, rotate: 0, layer: "top" });
    `);
    const input = await buildPlacementInput(circuit, rules, { [PART_UUID]: FOOTPRINT });
    const placed = autoPlacePcbWithReport(input);
    const report = createPcbToolReport({
        placementInput: input,
        placementReport: placed.report,
        layout: placed.layout,
    });
    const warnings = report.quality.warnings.filter((warning) => warning.includes('not recommended for passive R/C/L'));

    assert.equal(warnings.length, 1);
    assert.doesNotMatch(warnings[0], /R1|C1/);
});

test('keeps the exact fixed pose in normalized placement input', async () => {
    const circuit = singleComponentCircuit('R1', '10k');
    const rules = fixedRules('R1', 'passive');
    validatePlacementRulesForCircuit(circuit, rules);
    const input = await buildPlacementInput(circuit, rules, { [PART_UUID]: FOOTPRINT });

    assert.deepEqual(input.components[0].pcb.fixedPlacement, {
        x: 2.5,
        y: -1.5,
        rotate: 90,
        layer: 'top',
    });
});

test('still rejects fixed placement inside a satellite block', () => {
    const circuit: ExplainCircuit = {
        components: [
            {
                designator: 'U1',
                value: 'MCU',
                pins: [{ pin_number: '1', name: '1', signal_name: 'NET' }],
                part_uuid: PART_UUID,
                footprint_name: FOOTPRINT.name,
            },
            {
                designator: 'C1',
                value: '100nF',
                pins: [{ pin_number: '1', name: '1', signal_name: 'NET' }],
                part_uuid: PART_UUID,
                footprint_name: FOOTPRINT.name,
            },
        ],
    };
    const rules = runPcbLayoutDsl(`
        board.rect(20, 12);
        block("core", ["U1"], "mcu");
        block("support", ["C1"], "generic", { placement: "satellite", attachTo: "core" });
        component("C1").role("passive").fixed({ x: 1, y: 1, rotate: 0, layer: "top" });
    `);

    assert.throws(
        () => validatePlacementRulesForCircuit(circuit, rules),
        /Mechanical blocks with fixed\(\), edgeMount\(\), or edgePlace\(\) components cannot be satellites/,
    );
});
