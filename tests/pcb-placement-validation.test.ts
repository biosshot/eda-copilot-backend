import assert from 'node:assert/strict';
import test from 'node:test';
import { validatePlacementRulesForCircuit } from '../src/pcb-layout/placement-input.ts';
import { runPcbLayoutDsl } from '../src/pcb-layout/pcb-layout-dsl/spec.ts';
import type { ExplainCircuit } from '../src/types/circuit.ts';

const circuit: ExplainCircuit = {
    components: [
        {
            designator: 'U1',
            value: 'IC',
            pins: [{ pin_number: '1', name: '1', signal_name: 'NET' }],
            part_uuid: null,
            footprint_name: null,
        },
        {
            designator: 'C1',
            value: '100nF',
            pins: [{ pin_number: '1', name: '1', signal_name: 'NET' }],
            part_uuid: null,
            footprint_name: null,
        },
    ],
};

for (const assignment of ['list', 'component']) {
    test(`rejects mixed edgePlace blocks using ${assignment} ownership with actionable guidance`, () => {
        const rules = runPcbLayoutDsl(`
            board.rect(30, 20);
            block("mixed", ${assignment === 'list' ? '["U1", "C1"]' : '["U1"]'}, "generic");
            component("U1").edgePlace("top", { inset: 1 });
            ${assignment === 'component' ? 'component("C1").block("mixed");' : ''}
        `);
        assert.throws(() => validatePlacementRulesForCircuit(circuit, rules), (error: Error) => {
            assert.match(error.message, /block\("mixed"\) mixes edgePlace\(\) components \(U1\).*\(C1\)/);
            assert.match(error.message, /near\(\), veryNear\(\), or criticalPair\(\)/);
            return true;
        });
    });
}

test('allows multiple edgePlace components in a board-level block', () => {
    const rules = runPcbLayoutDsl(`
        board.rect(30, 20);
        block("controls", ["U1", "C1"], "connector");
        component("U1").edgePlace("top", { inset: 1 });
        component("C1").edgePlace("bottom", { inset: 1 });
    `);
    assert.doesNotThrow(() => validatePlacementRulesForCircuit(circuit, rules));
});

test('allows edgePlace and ordinary components in separate blocks linked by near', () => {
    const rules = runPcbLayoutDsl(`
        board.rect(30, 20);
        block("control", ["U1"], "connector");
        block("support", ["C1"], "generic");
        component("U1").edgePlace("top", { inset: 1 });
        near(comp("C1"), comp("U1"), "high");
    `);
    assert.doesNotThrow(() => validatePlacementRulesForCircuit(circuit, rules));
});

test('rejects blocks with components declared on different layers', () => {
    const rules = runPcbLayoutDsl(`
        board.rect(10, 10, { layers: ["top", "bottom"] });
        block("mixed", ["U1", "C1"], "generic");
        component("U1").block("mixed").top();
        component("C1").block("mixed").bottom();
    `);

    assert.throws(
        () => validatePlacementRulesForCircuit(circuit, rules),
        /block\("mixed"\) mixes layers \(U1:top, C1:bottom\)/,
    );
});

test('rejects disconnected physical blocks before placement', () => {
    const disconnectedCircuit: ExplainCircuit = {
        components: [
            {
                designator: 'R1',
                value: '10k',
                pins: [{ pin_number: '1', name: '1', signal_name: 'A' }],
                part_uuid: null,
                footprint_name: null,
            },
            {
                designator: 'R2',
                value: '10k',
                pins: [{ pin_number: '1', name: '1', signal_name: 'B' }],
                part_uuid: null,
                footprint_name: null,
            },
        ],
    };
    const rules = runPcbLayoutDsl(`
        board.rect(10, 10);
        block("bad", ["R1", "R2"], "generic");
        component("R1").block("bad").top();
        component("R2").block("bad").top();
    `);

    assert.throws(
        () => validatePlacementRulesForCircuit(disconnectedCircuit, rules),
        /block\("bad"\) is disconnected/,
    );
});

test('does not use GND-only nets as physical block connectivity', () => {
    const gndOnlyCircuit: ExplainCircuit = {
        components: [
            {
                designator: 'C1',
                value: '100nF',
                pins: [
                    { pin_number: '1', name: '1', signal_name: '+3V3' },
                    { pin_number: '2', name: '2', signal_name: 'GND' },
                ],
                part_uuid: null,
                footprint_name: null,
            },
            {
                designator: 'C2',
                value: '100nF',
                pins: [
                    { pin_number: '1', name: '1', signal_name: '+5V' },
                    { pin_number: '2', name: '2', signal_name: 'GND' },
                ],
                part_uuid: null,
                footprint_name: null,
            },
        ],
    };
    const rules = runPcbLayoutDsl(`
        board.rect(10, 10);
        block("caps", ["C1", "C2"], "power");
        component("C1").block("caps").top();
        component("C2").block("caps").top();
    `);

    assert.throws(
        () => validatePlacementRulesForCircuit(gndOnlyCircuit, rules),
        /block\("caps"\) is disconnected/,
    );
});

test('allows explicitly disconnected small placement groups', () => {
    const disconnectedCircuit: ExplainCircuit = {
        components: [
            {
                designator: 'R1',
                value: '22R',
                pins: [{ pin_number: '1', name: '1', signal_name: 'USB_DP_IN' }],
                part_uuid: null,
                footprint_name: null,
            },
            {
                designator: 'R2',
                value: '22R',
                pins: [{ pin_number: '1', name: '1', signal_name: 'USB_DM_IN' }],
                part_uuid: null,
                footprint_name: null,
            },
        ],
    };
    const rules = runPcbLayoutDsl(`
        board.rect(10, 10);
        block("usb_series", ["R1", "R2"], "generic", { allowDisconnected: true });
        component("R1").block("usb_series").top();
        component("R2").block("usb_series").top();
    `);

    assert.doesNotThrow(() => validatePlacementRulesForCircuit(disconnectedCircuit, rules));
});

test('rejects capCluster without a concrete target pin', () => {
    const clusterCircuit = capClusterCircuit();
    const rules = runPcbLayoutDsl(`
        board.rect(10, 10);
        block("power", ["U1"], "power");
        block("caps", ["C1", "C2"], "power");
        component("U1").block("power").top();
        component("C1").block("caps").top();
        component("C2").block("caps").top();
        capCluster(["C1", "C2"], { powerNet: "+3V3", returnNet: "GND" });
    `);

    assert.throws(
        () => validatePlacementRulesForCircuit(clusterCircuit, rules),
        /requires target: pin/,
    );
});

test('rejects capCluster when any capacitor lacks the requested power net', () => {
    const rules = runPcbLayoutDsl(`
        board.rect(10, 10);
        block("power", ["U1"], "power");
        block("caps", ["C1", "C2"], "power");
        component("U1").block("power").top();
        component("C1").block("caps").top();
        component("C2").block("caps").top();
        capCluster(["C1", "C2"], { powerNet: "+3V3", returnNet: "GND", target: pin("U1", "1") });
    `);

    assert.throws(
        () => validatePlacementRulesForCircuit(capClusterCircuit({ c2PowerNet: "+5V" }), rules),
        /component "C2" has no pad on powerNet "\+3V3"/,
    );
});

function capClusterCircuit(options: { c2PowerNet?: string } = {}): ExplainCircuit {
    return {
        components: [
            {
                designator: 'U1',
                value: 'IC',
                pins: [{ pin_number: '1', name: 'VDD', signal_name: '+3V3' }],
                part_uuid: null,
                footprint_name: null,
            },
            {
                designator: 'C1',
                value: '100nF',
                pins: [
                    { pin_number: '1', name: '1', signal_name: '+3V3' },
                    { pin_number: '2', name: '2', signal_name: 'GND' },
                ],
                part_uuid: null,
                footprint_name: null,
            },
            {
                designator: 'C2',
                value: '100nF',
                pins: [
                    { pin_number: '1', name: '1', signal_name: options.c2PowerNet ?? '+3V3' },
                    { pin_number: '2', name: '2', signal_name: 'GND' },
                ],
                part_uuid: null,
                footprint_name: null,
            },
        ],
    };
}
