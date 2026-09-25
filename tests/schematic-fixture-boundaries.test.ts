import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import type { Circuit } from '../src/types/circuit.ts';
import { pageBoundarySignals } from '../scripts/testing/schematic-boundaries.ts';

const bank = join('tests', 'schematic-layout', 'portable-scope');
const read = (file: string): Circuit => JSON.parse(readFileSync(join(bank, file), 'utf8'));

test('PortableScope isolated blocks inherit their full-page boundary signals', () => {
    let checked = 0;
    for (const fullFile of readdirSync(bank).filter(file => file.endsWith('-full.json'))) {
        const page = read(fullFile), prefix = fullFile.slice(0, -'-full.json'.length);
        for (const file of readdirSync(bank).filter(file => file.startsWith(`${prefix}-`) && file !== fullFile && file.endsWith('.json'))) {
            const signals = pageBoundarySignals(read(file), page);
            assert.notEqual(signals, null, `${file} must match ${fullFile}`);
            assert(!signals!.includes('GND'));
            checked++;
        }
    }
    assert.equal(checked, 19);
    assert.deepEqual(pageBoundarySignals(read('portable-scope-adcclock-clock.json'), read('portable-scope-adcclock-full.json')),
        ['ADC_CLK_M', 'ADC_CLK_P']);
    assert.deepEqual(pageBoundarySignals(read('portable-scope-batterypower-ddr_vtt.json'), read('portable-scope-batterypower-full.json')),
        ['DDR_1V5', 'DDR_1V5_PG', 'SYS_3V3']);
});
