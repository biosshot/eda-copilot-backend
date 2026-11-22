import { object } from '../analysis-dsl/descriptions.ts';

export const record = (value: unknown): Record<string, unknown> => object(value) ? value : {};
export const list = (value: unknown): readonly unknown[] => Array.isArray(value) ? value : [];
export const rows = (value: unknown) => list(value).filter(object);
export const finite = (value: unknown): value is number => typeof value === 'number' && Number.isFinite(value);
export const format = (value: number) => Number(value.toPrecision(4)).toString();
export const escape = (value: unknown) => String(value ?? '').replace(/[&<>"']/g, c =>
  ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]!);

export const width = 960;
export const ink = '#e1edf8';
export const muted = '#aabed0';
export const unknown = '#526174';
export const accent = '#ffbd73';
export const colors = ['#64d7ff', '#ffbd73', '#b9a5fa', '#7ce3ae', '#ff91b4', '#f2e98a'];
const scaleColors = ['#453382', '#246d9c', '#179485', '#82bd64', '#ffe17d'];

export function color(value: number, min: number, max: number) {
  const position = min === max ? 2 : Math.max(0, Math.min(4, 4 * (value - min) / (max - min)));
  const lower = Math.floor(position);
  const from = parseInt(scaleColors[lower]!.slice(1), 16);
  const to = parseInt(scaleColors[Math.min(4, lower + 1)]!.slice(1), 16);
  const fraction = position - lower;
  return 'rgb(' + [16, 8, 0].map(shift => {
    const a = (from >> shift) & 255;
    const b = (to >> shift) & 255;
    return Math.round(a + fraction * (b - a));
  }).join(',') + ')';
}

export function label(x: number, y: number, value: unknown, options: {
  size?: number; color?: string; anchor?: 'start' | 'middle' | 'end';
} = {}) {
  return `<text x="${x}" y="${y}" font-size="${options.size ?? 15}" fill="${options.color ?? ink}" text-anchor="${options.anchor ?? 'start'}">${escape(value)}</text>`;
}

export function frame(title: string, status: string, body: string, height: number) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img"><title>${escape(title)}</title>
<rect width="${width}" height="${height}" fill="#101b2a"/>
<g font-family="Arial, sans-serif">
${label(32, 43, title, { size: 26 })}
${label(32, 72, status, { color: accent })}
${body}
<path d="M32 ${height - 40}H928" stroke="#33465a"/>
${label(32, height - 17, 'Physical accuracy unverified', { color: muted, size: 13 })}
</g></svg>`;
}

export function textCard(title: string, status: string, lines: readonly string[]) {
  const wrapped = lines.flatMap(line => line.match(/.{1,108}(?:\s|$)|.{1,108}/gu) ?? ['']);
  return frame(title, status, wrapped.map((line, index) => label(32, 110 + index * 23, line)).join(''),
    155 + wrapped.length * 23);
}

export function colorKey(min: number, max: number, unit: string, y: number, unknownLabel?: string) {
  return Array.from({ length: 80 }, (_, index) =>
    `<rect x="${32 + index * 4}" y="${y}" width="4" height="13" fill="${color(min + (max - min) * index / 79, min, max)}"/>`).join('') +
    label(32, y + 34, format(min)) + label(352, y + 34, `${format(max)} ${unit}`, { anchor: 'end' }) +
    (unknownLabel ? `<rect x="385" y="${y}" width="13" height="13" fill="${unknown}"/>` + label(406, y + 12, unknownLabel) : '');
}

