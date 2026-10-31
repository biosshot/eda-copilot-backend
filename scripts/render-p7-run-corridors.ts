import { readFileSync, writeFileSync } from 'node:fs';
import { createCanvas } from 'canvas';
import { createProblemExecutor } from '../src/analyze/execute-problem.ts';
const evidence = JSON.parse(readFileSync('debugging/p7-channel-plan/corridor-refusal.json', 'utf8')), boardJson = readFileSync('tests/pcb-analyze/STM32G0_CAN_RS485/STM32G0_CAN_RS485.json', 'utf8'), g = createProblemExecutor()({ kind: 'prepare-geometry', boardJson }), canvas = createCanvas(1400, 1000), ctx = canvas.getContext('2d');
ctx.fillStyle = '#f8fafc';
ctx.fillRect(0, 0, 1400, 1000);
ctx.fillStyle = '#0f172a';
ctx.font = 'bold 25px Arial';
ctx.fillText('STM32: finite-return corridor admission is blocked', 30, 40);
ctx.font = '17px Arial';
ctx.fillText('All15 physical runs (16 authored segments) retain lateral GND connections; no plane current is silently removed. Geometry evidence only.', 30, 75);
const polygons = evidence.corridors.flatMap((c: any) => c.polygonMm), xs = polygons.map((p: number[]) => p[0]), ys = polygons.map((p: number[]) => p[1]), xmin = Math.min(...xs) - .5, xmax = Math.max(...xs) + .5, ymin = Math.min(...ys) - .5, ymax = Math.max(...ys) + .5, scale = Math.min(1300 / (xmax - xmin), 750 / (ymax - ymin)), map = (p: number[]) => [50 + (p[0] - xmin) * scale, 110 + (p[1] - ymin) * scale];
function ring(points: number[][]) { points.forEach((p, i) => { const a = map(p); if (i)
    ctx.lineTo(a[0], a[1]);
else
    ctx.moveTo(a[0], a[1]); }); ctx.closePath(); }
ctx.save();
ctx.beginPath();
ctx.rect(30, 100, 1340, 780);
ctx.clip();
for (const r of g.regions) {
    ctx.beginPath();
    for (const p of [r.polygon.outer, ...r.polygon.holes])
        ring(p);
    ctx.fillStyle = r.source.path.includes('zones[0]') ? 'rgba(50,130,95,.14)' : r.layer === 'TOP' ? 'rgba(220,65,30,.20)' : 'rgba(30,90,220,.20)';
    ctx.fill('evenodd');
}
for (const [i, c] of evidence.corridors.entries()) {
    ctx.beginPath();
    ring(c.polygonMm);
    ctx.strokeStyle = i === 1 ? '#dc2626' : 'rgba(71,85,105,.25)';
    ctx.lineWidth = i === 1 ? 3 : 1;
    ctx.stroke();
    const d = c.direction, a = c.longitudinalMm[0], b = c.longitudinalMm[1], cross = (c.transverseMm[0] + c.transverseMm[1]) / 2, t = [-d[1], d[0]], p = map([d[0] * a + t[0] * cross, d[1] * a + t[1] * cross]), q = map([d[0] * b + t[0] * cross, d[1] * b + t[1] * cross]);
    ctx.beginPath();
    ctx.moveTo(p[0], p[1]);
    ctx.lineTo(q[0], q[1]);
    ctx.lineWidth = 3;
    ctx.strokeStyle = '#1d4ed8';
    ctx.stroke();
}
ctx.restore();
ctx.fillStyle = '#0f172a';
ctx.font = '18px Arial';
ctx.fillText('Blue: actual signal runs. Red: track48 proposed finite run corridor (3.2201mm long, half-width3.327mm).', 30, 920);
ctx.fillText('Green: actual GND source. Gray: other corridor proposals. Shared plane is continuous across corridor sides.', 30, 953);
writeFileSync('debugging/p7-channel-plan/corridor-refusal.png', canvas.toBuffer('image/png'));
