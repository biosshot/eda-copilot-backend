import {createCanvas} from 'canvas';
import {validationResults} from './pcb-analysis-validation-audit.ts';

/** Static QA artifact: reads numeric arrays, never runs saved HTML or scripts. */
export function validationPng(name: string, metadata: any, output: any, focus = false): Buffer {
  const s = validationResults(output)[0]?.solution;
  if (!s) throw Error('No geometry solution for static QA');
  const canvas = createCanvas(1600, 1050), c = canvas.getContext('2d');
  c.fillStyle = '#0d1829'; c.fillRect(0, 0, 1600, 1050);
  c.fillStyle = '#edf5ff'; c.font = 'bold 30px sans-serif';
  c.fillText(name + ' · Geometry validation', 30, 45);
  c.font = '17px sans-serif'; c.fillStyle = '#b8cce2';
  c.fillText('CPU/F64 · mm · source copper and shortest-distance witnesses · coverage remains explicit', 30, 76);
  const finding = s.measurements.find((m: any) => m.status === 'confirmed')
    ?? s.measurements.find((m: any) => m.status === 'uncertain') ?? s.measurements[0];
  const regions = new Map<string, any>(s.regions.map((r: any) => [r.id, r]));
  const selected = finding?.region_ids.map((id: string) => regions.get(id)).filter(Boolean) ?? [];
  const points = focus && finding
    ? [...finding.witness, ...(finding.outside_witness ? [finding.outside_witness] : [])]
    : [...s.outline.outer, ...s.regions.flatMap((r: any) => r.polygon.outer)];
  const bounds = [Infinity, Infinity, -Infinity, -Infinity];
  for (const p of points) {
    bounds[0] = Math.min(bounds[0], p[0]); bounds[1] = Math.min(bounds[1], p[1]);
    bounds[2] = Math.max(bounds[2], p[0]); bounds[3] = Math.max(bounds[3], p[1]);
  }
  if (focus && finding) {
    const margin = Math.max(.6, finding.required_mm * 4, finding.actual_mm * .5);
    bounds[0] -= margin; bounds[1] -= margin; bounds[2] += margin; bounds[3] += margin;
  }
  const pad = Math.max(.15, Math.max(bounds[2] - bounds[0], bounds[3] - bounds[1]) * .05);
  const scale = Math.min(960 / (bounds[2] - bounds[0] + 2 * pad), 720 / (bounds[3] - bounds[1] + 2 * pad));
  const ox = 30 + (960 - scale * (bounds[2] - bounds[0])) / 2 - bounds[0] * scale;
  const oy = 125 + (720 - scale * (bounds[3] - bounds[1])) / 2 - bounds[1] * scale;
  c.fillStyle = '#071321'; c.fillRect(20, 110, 990, 760);
  c.save(); c.beginPath(); c.rect(20, 110, 990, 760); c.clip(); c.translate(ox, oy); c.scale(scale, scale);
  const path = (p: any) => {
    c.beginPath();
    for (const ring of [p.outer, ...p.holes]) {
      c.moveTo(ring[0][0], ring[0][1]);
      for (const q of ring.slice(1)) c.lineTo(q[0], q[1]);
      c.closePath();
    }
  };
  path(s.outline); c.fillStyle = '#193c34'; c.fill('evenodd'); c.globalAlpha = .55;
  for (const r of s.regions) {
    path(r.polygon); c.fillStyle = r.layer === 'TOP' ? '#dc6073' : r.layer === 'BOTTOM' ? '#428ce2' : '#c99f54';
    c.fill('evenodd');
  }
  c.globalAlpha = 1; path(s.outline); c.strokeStyle = '#d8e7ed'; c.lineWidth = 1.5 / scale; c.stroke();
  if (finding) {
    for (const r of selected) {
      path(r.polygon); c.fillStyle = '#ffe27a44'; c.fill('evenodd');
      c.strokeStyle = '#ffe27a'; c.lineWidth = 2 / scale; c.stroke();
    }
    const [a, b] = finding.witness;
    c.beginPath(); c.moveTo(a[0], a[1]); c.lineTo(b[0], b[1]); c.strokeStyle = '#48f2ff'; c.lineWidth = 3 / scale; c.stroke();
    for (const p of [a, b]) {
      c.beginPath(); c.arc(p[0], p[1], 4 / scale, 0, Math.PI * 2); c.fillStyle = '#48f2ff'; c.fill();
    }
    if (finding.outside_witness) {
      const p = finding.outside_witness;
      c.beginPath(); c.arc(p[0], p[1], 5 / scale, 0, Math.PI * 2); c.fillStyle = '#fff'; c.fill();
    }
  }
  c.restore();
  const barMm = 10 ** Math.floor(Math.log10(100 / scale));
  c.beginPath(); c.moveTo(45, 835); c.lineTo(45 + barMm * scale, 835); c.strokeStyle = '#fff'; c.lineWidth = 2; c.stroke();
  c.fillStyle = '#fff'; c.font = '14px sans-serif'; c.fillText(barMm + ' mm', 45, 825);
  const text = (v: string, x: number, y: number, width: number, font = '16px sans-serif', color = '#d7e6f6') => {
    c.font = font; c.fillStyle = color; let line = '';
    for (const word of v.split(' ')) {
      const next = line ? line + ' ' + word : word;
      if (c.measureText(next).width > width && line) { c.fillText(line, x, y); y += 23; line = word; }
      else line = next;
    }
    if (line) c.fillText(line, x, y);
    return y + 25;
  };
  let y = text(s.violations.length + ' confirmed reports · ' + s.uncertain_count + ' uncertain', 1040, 130, 525, 'bold 22px sans-serif', '#ffd68c');
  y = text('Aggregate no-violation check: UNKNOWN. Coverage: ' + s.status + '.', 1040, y, 525);
  y = text('Confirmed findings remain visible even when a full-board defect count is unavailable.', 1040, y, 525);
  y += 8;
  for (const cov of s.coverage) {
    const requirement = cov.rule === 'connectivity' ? 'max 1 known net per island' : cov.required_mm + ' mm';
    y = text(cov.rule + ' · ' + requirement + ' · ' + cov.status, 1040, y, 525, '17px sans-serif');
  }
  y += 10; y = text(focus ? 'Selected finding detail' : 'Board overview and selected finding', 1040, y, 525, 'bold 20px sans-serif', '#fff');
  if (finding) {
    y = text(finding.rule + ' / ' + finding.status + ' / ' + finding.code, 1040, y, 525);
    const requirement = finding.rule === 'connectivity'
      ? (finding.code === 'physical_short' ? finding.nets.length + ' known nets; maximum 1 per reliable island. ' : 'Contact unverified. ') + 'Witness ' + finding.actual_mm.toPrecision(8) + ' mm'
      : 'Actual ' + finding.actual_mm.toPrecision(8) + ' mm; required ' + finding.required_mm + ' mm; error ±' + finding.error_mm.toExponential(3) + ' mm';
    y = text(requirement, 1040, y, 525);
    y = text('Sources: ' + finding.source_ids.slice(0, 6).join(' / ') + (finding.source_ids.length > 6 ? ' … (' + finding.source_ids.length + ' total)' : ''), 1040, y, 525, '14px sans-serif');
    y = text('Nets: ' + finding.nets.join(' / ') + ' · Layers: ' + finding.layers.join(', '), 1040, y, 525);
    const witness = finding.witness.map((p: number[]) => '(' + p.map(v => v.toFixed(6)).join(', ') + ')').join(' → ');
    y = text('Witness mm: ' + witness, 1040, y, 525, '14px sans-serif');
    y = text(finding.message + (finding.code === 'physical_short' ? ' Marker: representative reliable contact; the retained graph proves the whole shorted island.' : ''), 1040, y, 525, '14px sans-serif');
  }
  y += 10;
  y = text('Numerical audit: ' + metadata.audit?.status + ' · ' + (metadata.wallMs / 1000).toFixed(2) + ' s · ' + (s.resources.peakRssBytes / 1048576).toFixed(1) + ' MiB process peak RSS', 1040, y, 525);
  text('Candidates: ' + s.statistics.candidate_pairs + ' · exact segment checks: ' + s.statistics.exact_segment_checks, 1040, y, 525, '14px sans-serif');
  c.font = '15px sans-serif'; c.fillStyle = '#b8cce2';
  c.fillText('Top red · bottom blue · inner amber · outline/cutouts white · selected copper yellow · distance witness cyan', 30, 900);
  c.fillText('Connectivity: conservative shorts/contacts; open intent unavailable. Missing/stale fill and uncertainty remain in diagnostics.', 30, 930);
  c.fillText('Static QA drawn from saved numerical JSON. Interactive HTML controls have not been executed in a browser.', 30, 960);
  c.font = '12px monospace';
  c.fillText('Board SHA-256 ' + metadata.boardSha256, 30, 1002);
  c.fillText('Native SHA-256 ' + metadata.nativeSha256, 30, 1025);
  return canvas.toBuffer('image/png');
}
