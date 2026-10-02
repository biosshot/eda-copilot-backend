import assert from 'node:assert/strict';
import test from 'node:test';
import { componentSearch, componentSymbol } from '../src/components.ts';
import { easyEdaSearch } from '../src/devices/easy-eda.ts';
import { getSymbol } from '../src/devices/symbols/symbol-parser.ts';
import { requireResolvedComponentFootprint } from '../src/pcb-layout/footprints.ts';
import { getPcbComponentSizes } from '../src/pcb.ts';

test('edition arguments select all library routes and isolate symbol/footprint caches', async t => {
    const part = 'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee';
    const symbol = 'dddddddddddddddddddddddddddddddd';
    const footprint = 'cccccccccccccccccccccccccccccccc';
    const requests: URL[] = [];
    t.mock.method(globalThis, 'fetch', async (input: string | URL | Request) => {
        const url = new URL(String(input));
        requests.push(url);
        assert.ok(['pro.easyeda.com', 'pro.lceda.cn'].includes(url.hostname));
        const name = url.hostname === 'pro.lceda.cn' ? 'CHINESE' : 'INTERNATIONAL';
        const symbolData = [
            ['DOCTYPE', 'SYMBOL', '1.1'], ['PART', 'RESISTOR.1', { BBOX: [-10, -5, 10, 5] }],
            ['PIN', 'p1', 1, null, -20, 0, 10, 0],
            ['ATTR', 'n1', 'p1', 'NAME', name], ['ATTR', 'num1', 'p1', 'NUMBER', '1'],
            ['PIN', 'p2', 1, null, 20, 0, 10, 180],
            ['ATTR', 'n2', 'p2', 'NAME', 'GND'], ['ATTR', 'num2', 'p2', 'NUMBER', '2'],
        ].map(row => JSON.stringify(row)).join('\n');
        const device = { uuid: part, display_title: name, symbol: { uuid: symbol }, footprint: { uuid: footprint },
            attributes: { 'Manufacturer Part': name, Designator: 'R?', Datasheet: 'https://example.invalid/test.pdf' } };
        if (url.pathname === '/api/devices/' + part) return Response.json({ success: true, result: device });
        if (url.pathname === '/api/v2/components/' + symbol) return Response.json({ success: true, result: { dataStr: symbolData } });
        if (url.pathname === '/api/v2/components/' + footprint) return Response.json({ success: true, result: { title: name, dataStr: [
            ['DOCTYPE', 'FOOTPRINT'],
            ['PAD', 'p1', 0, '', 1, '1', 0, 0, 0, null, ['RECT', 10, 10, 0], [], 0, 0, 0, 1, 0, null, null, null, null, 0],
        ].map(row => JSON.stringify(row)).join('\n') } });
        if (url.pathname === '/api/devices/search') return Response.json({ success: true,
            result: { lists: { user: [device] }, page: 1, pageSize: 10, totalPage: 1, count: 1 } });
        if (['/api/v2/eda/product/search', '/api/v2/eda/product/list'].includes(url.pathname)) return Response.json({ code: 200,
            result: { pageInfo: { totalPage: 1 }, productList: [{ manufacturer: name, device_info: {
                ...device, footprint_info: { title: name }, symbol_info: { dataStr: symbolData },
            } }] } });
        throw new Error('Unexpected route: ' + url);
    });

    await t.test('system and public searches, including catalog filters, use the requested host', async () => {
        assert.equal((await componentSearch({ MPN: 'test' })).components![0].name, 'INTERNATIONAL');
        assert.equal((await componentSearch({ MPN: 'test' }, { edaEdition: 'jlceda' })).components![0].name, 'CHINESE');
        assert.equal((await componentSearch({ MPN: 'test', library_uuid: 'user' }, { edaEdition: 'jlceda' })).components![0].name, 'CHINESE');
        assert.equal((await easyEdaSearch({ catalogId: 308, params: null, currPage: 1, pageSize: 1 }, undefined, true, 'jlceda'))[0].name, 'CHINESE');
    });
    await t.test('same UUID resolves independently in concurrent editions and subsequent cache hits', async () => {
        const [en, cn] = await Promise.all([
            componentSearch({ part_uuid: part }), componentSearch({ part_uuid: part }, { edaEdition: 'jlceda' }),
        ]);
        assert.equal(en.bestComponent!.name, 'INTERNATIONAL');
        assert.equal(cn.bestComponent!.name, 'CHINESE');
        assert.match((await componentSymbol(part)).dataStr, /INTERNATIONAL/);
        assert.match((await componentSymbol(part, { edaEdition: 'jlceda' })).dataStr, /CHINESE/);
        const [enSymbol, cnSymbol] = await Promise.all([getSymbol(part), getSymbol(part, undefined, 'jlceda')]);
        assert.equal(enSymbol!.pins[0].name, 'INTERNATIONAL');
        assert.equal(cnSymbol!.pins[0].name, 'CHINESE');
        assert.equal((await getSymbol(part))!.pins[0].name, 'INTERNATIONAL');
    });
    await t.test('footprints and component sizes keep edition through UUID and part fallback caches', async () => {
        const component = { designator: 'R1', value: 'test', part_uuid: part, pins: [] };
        const cache = new Map();
        assert.equal((await requireResolvedComponentFootprint(component, cache)).name, 'INTERNATIONAL');
        assert.equal((await requireResolvedComponentFootprint(component, cache, undefined, 'jlceda')).name, 'CHINESE');
        assert.equal((await requireResolvedComponentFootprint({ ...component, footprint_uuid: footprint }, cache, undefined, 'jlceda')).name, 'CHINESE');
        const result = await getPcbComponentSizes({ circuit: { components: [component] }, includeAll: true }, { edaEdition: 'jlceda' });
        assert.equal(result.report!.components[0].footprint, 'CHINESE');
    });
    assert.equal(new Set(requests.map(url => url.pathname)).size, 6);
});
