import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const fixturePath = 'tests/fixtures/analysis-shared-return-prototype/manifest.json';
export function evaluate(manifest: any, output?: any) {
  if (manifest.schema !== 'p7-shared-return-benchmark-v1' || manifest.units.length !== 'm' || manifest.units.impedance !== 'ohm') throw Error('Manifest units/schema mismatch');
  if (output && output.schema !== 'p7-shared-return-output-v1') throw Error('Output schema mismatch');
  const ids = new Set<string>();
  for (const c of output?.cases ?? []) { if (ids.has(c.id)) throw Error('Duplicate output case'); ids.add(c.id); }
  const rows = manifest.cases.map((c: any) => {
    const actual = output?.cases?.find((v: any) => v.id === c.id);
    if (!actual || actual.status === 'unsupported') return { id: c.id, family: c.family, metric: 'DC port Z', status: 'unavailable', reason: actual?.reason ?? 'Prototype output pending' };
    if (actual.status !== 'ok' || actual.units !== 'ohm' || (!Array.isArray(actual.ports) || actual.ports.length !== c.model.ports.length || actual.ports.some((p: any, i: number) => p.positive !== c.model.ports[i].positive || p.negative !== c.model.ports[i].negative))) return { id: c.id, status: 'fail', reason: 'Status, units or ordered physical ports mismatch' };
    const ref = c.reference.impedance_ohm, z = actual.impedance_ohm;
    if (!Array.isArray(z) || z.length !== ref.length || z.some((r: any, i: number) => !Array.isArray(r) || r.length !== ref[i].length)) return { id: c.id, status: 'fail', reason: 'Matrix shape mismatch' };
    let maxNormalizedError = 0, maxAbsoluteErrorOhm = 0;
    for (let i = 0; i < ref.length; i++) for (let j = 0; j < ref.length; j++) {
      if (!Number.isFinite(z[i][j])) return { id: c.id, status: 'fail', reason: 'Non-finite matrix entry' };
      const error = Math.abs(z[i][j] - ref[i][j]);
      maxAbsoluteErrorOhm = Math.max(maxAbsoluteErrorOhm, error);
      maxNormalizedError = Math.max(maxNormalizedError, error / Math.max(manifest.tolerances.absolute_ohm, manifest.tolerances.relative * Math.abs(ref[i][j])));
    }
    return { id: c.id, family: c.family, metric: 'DC port Z', frequency_hz: 0, status: maxNormalizedError <= 1 ? 'pass' : 'fail', maxNormalizedError, maxAbsoluteErrorOhm };
  });
  rows.push(...manifest.unavailable.map((v: any) => ({ ...v, status: 'unavailable' })));
  return { schema: 'p7-shared-return-verification-v1', rows, counts: Object.fromEntries(['pass','fail','unavailable'].map(s => [s, rows.filter((r: any) => r.status === s).length])), resources: output?.resources ?? 'unavailable: process tree measurements not supplied', scope: 'DC ideal graph controls only; EM and P7 remain unaccepted' };
}

export function figures(manifest: any, output: any, dir: string) {
  const escape = (s: string) => s.replaceAll('&','&amp;').replaceAll('<','&lt;');
  const svg = (body: string, height: number) => `<svg xmlns="http://www.w3.org/2000/svg" width="1050" height="${height}" viewBox="0 0 1050 ${height}"><rect width="100%" height="100%" fill="white"/><g font-family="sans-serif" font-size="13">${body}</g></svg>`;
  let geometry = '<text x="20" y="22">Authored DC resistor topology; node coordinates schematic, not PCB geometry</text>', comparison = '<text x="20" y="22">DC Z11: reference (black) and prototype (orange), mOhm; pending output has no orange bar</text>';
  manifest.cases.forEach((c: any, i: number) => {
    const y = 65 + i*90, nodes = c.model.nodes;
    geometry += `<text x="15" y="${y}">${escape(c.id)}</text>`;
    c.model.edges.forEach((e: any, k: number) => {const a=nodes.findIndex((n: any)=>n.id===e.a),b=nodes.findIndex((n: any)=>n.id===e.b); geometry += `<path d="M ${240+a*135} ${y} Q ${240+(a+b)*67.5} ${y-30-k*5} ${240+b*135} ${y}" stroke="#557" fill="none"/><text x="${240+(a+b)*67.5}" y="${y-16-k*5}">${(e.resistance_ohm*1000).toFixed(3)}</text>`; });
    nodes.forEach((n: any,j: number)=>geometry+=`<circle cx="${240+j*135}" cy="${y}" r="4"/><text x="${240+j*135}" y="${y+22}">${escape(n.id)}</text>`);
    const ref=c.reference.impedance_ohm[0][0]*1000, actual=output?.cases?.find((a: any)=>a.id===c.id)?.impedance_ohm?.[0]?.[0];
    comparison+=`<text x="15" y="${y}">${escape(c.id)}</text><rect x="200" y="${y-15}" width="${ref*170}" height="8" fill="#222"/><text x="850" y="${y}">${ref.toFixed(6)}</text>`;
    if(Number.isFinite(actual)) comparison+=`<rect x="200" y="${y}" width="${actual*1000*170}" height="8" fill="#d80"/>`;
  });
  writeFileSync(resolve(dir,'geometry.svg'),svg(geometry,810)); writeFileSync(resolve(dir,'comparison.svg'),svg(comparison,810));
}
if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const manifest=JSON.parse(readFileSync(fixturePath,'utf8'));
  const output=process.argv[2] ? JSON.parse(readFileSync(process.argv[2],'utf8')) : undefined;
  const dir=resolve(process.argv[3] ?? 'debugging/p7-shared-return-benchmark'); mkdirSync(dir,{recursive:true});
  const report=evaluate(manifest,output); writeFileSync(resolve(dir,'verification.json'),JSON.stringify(report,null,2)+'\n'); figures(manifest,output,dir);
  console.log(JSON.stringify(report.counts)); if(report.counts.fail) process.exitCode=1;
}
