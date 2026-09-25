import { test } from 'node:test';
import assert from 'node:assert/strict';
import type { CircuitComponent } from '../src/types/circuit.ts';
import type { Placed } from '../src/circuit-layout/refinement/geometry.ts';
import { turnNode } from '../src/circuit-layout/refinement/groups.ts';
import { connectorOrientationSeverity, connectorOverrideWorthwhile, inferConnectorRoles } from '../src/circuit-layout/refinement/connector-policy.ts';

const part = (designator: string, signal: string, block = 'front'): CircuitComponent => ({
    designator, block_name: block, value: '', search_query: '', part_uuid: null,
    pins: [{ pin_number: 1, name: 'SIG', signal_name: signal }, { pin_number: 2, name: 'GND', signal_name: 'GND' }]
});
const placed = (designator: string, x: number): Placed => ({ id: designator, x, y: 0, width: 60, height: 70,
    ports: [{ id: `${designator}_pin_1`, x: 60, y: 20 }, { id: `${designator}_pin_2`, x: 60, y: 50 }] });
const edge = (a: string, b: string) => ({ id: `${a}-${b}`, sources: [`${a}_pin_1`], targets: [`${b}_pin_1`] });

test('the first placement gives unnamed boundary connectors a per-connector input/output role', () => {
    const nodes = [placed('J1', 0), placed('R1', 120), placed('R2', 240), placed('J2', 360)];
    const components = [part('J1', 'SCOPE'), part('R1', 'SCOPE'), part('R2', 'MONITOR'), part('J2', 'MONITOR')];
    const roles = inferConnectorRoles(nodes, [edge('J1', 'R1'), edge('R2', 'J2')], components);
    assert.equal(roles.get('J1'), 'input');
    assert.equal(roles.get('J2'), 'output');
});

test('explicit signal direction wins over position; conflicting styles fall back to position', () => {
    const named = part('J1', 'SCOPE_IN');
    const nodes = [placed('R1', 0), placed('J1', 120)];
    assert.equal(inferConnectorRoles(nodes, [edge('R1', 'J1')], [part('R1', 'SCOPE_IN'), named]).get('J1'), 'input');
    named.pins[0].signal_name = 'SCOPE';
    named.pins[0].port_style = 'out';
    assert.equal(inferConnectorRoles(nodes, [edge('R1', 'J1')], [part('R1', 'SCOPE'), named]).get('J1'), 'output');
    named.pins.push({ pin_number: 3, name: 'CTRL', signal_name: 'CTRL_IN', port_style: 'in' });
    assert.equal(inferConnectorRoles(nodes, [edge('R1', 'J1')], [part('R1', 'SCOPE'), named]).get('J1'), 'output');
});

test('input BNC faces its circuit with GND below; output BNC faces back toward it', () => {
    const component = part('J6', 'SCOPE_IN'), native = placed('J6', 0);
    assert.equal(connectorOrientationSeverity(native, component, 'input'), 0);
    assert.equal(connectorOrientationSeverity(turnNode(native, 90), component, 'input'), 1.7);
    assert.equal(connectorOrientationSeverity(turnNode(native, 180), component, 'output'), 0.7);
});

test('a two-pin supply connector uses its power pin as the lead and keeps ground lower', () => {
    const component = part('J7', 'VCC'), native = placed('J7', 0);
    assert.equal(connectorOrientationSeverity(native, component, 'input'), 0);
    assert.equal(connectorOrientationSeverity(turnNode(native, 180), component, 'output'), 0.7);
});

test('a less readable connector pose needs a 30 percent reduction in its own leads', () => {
    assert.equal(connectorOverrideWorthwhile(70, 100), true);
    assert.equal(connectorOverrideWorthwhile(71, 100), false);
});
