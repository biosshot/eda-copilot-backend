import { readFileSync, writeFileSync } from 'node:fs';
import { createCanvas, loadImage } from 'canvas';
const out = 'docs/experimental/pcb/global-placement-2026-09-27';
const data = JSON.parse(readFileSync(`${out}/measurements.json`, 'utf8'));
async function sheet(kind, examples, height) {
    const width = 800, canvas = createCanvas(width * 2, height * examples.length), ctx = canvas.getContext('2d');
    ctx.fillStyle = '#f3f5f7'; ctx.fillRect(0, 0, canvas.width, canvas.height);
    for (let row = 0; row < examples.length; row++) {
        const [fixture, name] = examples[row], board = data.find(b => b.name === fixture);
        const entity = board.entities.find(e => kind === 'board' ? e.id === 'board' : e.name === name);
        for (let col = 0; col < 2; col++) {
            const tag = col ? 'pads' : 'refined', variant = board.variants[tag], m = variant.metrics[entity.id];
            let svg = readFileSync(`${out}/${variant.files[entity.id]}`, 'utf8');
            svg = svg.replace(/^(<svg[^>]*width=")([\d.]+)(" height=")([\d.]+)/, (_, a, w, b, h) => `${a}${Number(w) * 4}${b}${Number(h) * 4}`);
            const img = await loadImage(Buffer.from(svg));
            const scale = Math.min((width - 32) / img.width, (height - 110) / img.height);
            ctx.fillStyle = '#172d40'; ctx.font = 'bold 22px Arial';
            ctx.fillText(`${fixture.replace('bank-', '')}${name ? ' / ' + name : ''}`, col * width + 16, row * height + 28);
            ctx.font = '18px Arial'; ctx.fillText(col ? 'Со штрафами за пады' : 'Кандидаты + postrefine, без штрафов за пады', col * width + 16, row * height + 55);
            ctx.fillText(`MST ${m.wireLength} мм · линия–линия ${m.crossings} · линия–пад ${m.foreignPadHits}`, col * width + 16, row * height + 82);
            ctx.drawImage(img, col * width + (width - img.width * scale) / 2, row * height + 100, img.width * scale, img.height * scale);
        }
    }
    writeFileSync(`${out}/${kind}-comparison.png`, canvas.toBuffer('image/png'));
}
await sheet('block', [['Telemetry', 'usb_charge'], ['ESPower', 'charger'], ['bank-CNTRL_ACESS_MAX', 'rs485_bus'], ['bank-ICM20948_v2', 'power_1v8']], 430);
await sheet('board', [['Telemetry'], ['bank-CNTRL_ACESS_MAX'], ['bank-ICM20948_v2']], 800);
