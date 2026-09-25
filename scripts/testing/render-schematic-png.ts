import { writeFile } from 'node:fs/promises';
import { createCanvas, loadImage } from 'canvas';

/** Rasterize the same SVG shown in the schematic layout gallery. */
export async function renderSvgToPng(svg: string, path: string) {
    const width = Number(svg.match(/<svg[^>]*\bwidth="([\d.]+)"/)?.[1]);
    const height = Number(svg.match(/<svg[^>]*\bheight="([\d.]+)"/)?.[1]);
    if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
        throw new Error('Invalid schematic SVG dimensions');
    }
    const canvas = createCanvas(Math.ceil(width), Math.ceil(height));
    const context = canvas.getContext('2d');
    context.fillStyle = '#fff';
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.drawImage(await loadImage(Buffer.from(svg)), 0, 0);
    await writeFile(path, canvas.toBuffer('image/png'));
}
