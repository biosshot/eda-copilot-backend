import { z } from 'zod';
import type { ComponentDescription, ComponentInput, Environment, Json, ProfilesInput, RoutingBoardV2, StackupDescription, StackupInput, Waveform } from './spec-doc.js';
import { componentInput, environment, invalid, parse, stackupInput } from './schema.ts';
import { freeze, json } from './data.ts';
import type { RoutingStackup } from '../../types/pcb/routing-board-v2.ts';

export function componentDescription(kind: ComponentDescription['kind'], input: ComponentInput): ComponentDescription {
  const v = parse(componentInput, json(input));
  if (kind === 'device' && !v.model) invalid('device requires an explicit model');
  const fields: Record<ComponentDescription['kind'], string[]> = {
    ic: [], device: [], resistor: ['resistanceOhm', 'referenceC', 'temperatureCoefficientPerK'], capacitor: ['capacitanceF', 'esrOhm', 'eslH', 'ideal'], inductor: ['inductanceH', 'seriesResistanceOhm', 'ideal'],
  };
  const common = ['designator', 'model', 'pins', 'ground', 'supplies', 'thermalNodes'];
  for (const k of Object.keys(v)) if (![...common, ...fields[kind]].includes(k)) invalid(`${kind} does not accept ${k}`);
  return freeze({ kind, ...v }) as ComponentDescription;
}

export function stackup(input: StackupInput): StackupDescription {
  const v = parse(stackupInput, json(input));
  const names = v.layers.map(l => l.name);
  if (new Set(names).size !== names.length) invalid('Stackup layer names must be unique');
  const layers: RoutingStackup['layers'][number][] = v.layers.map(l => {
    if (l.kind === 'dielectric') return { ...l };
    if (l.thicknessMm !== undefined && l.thicknessOz !== undefined) invalid('Choose thicknessMm or thicknessOz, not both');
    const oz = l.thicknessOz ?? v.fallbackCopperThicknessOz;
    const thicknessMm = l.thicknessMm ?? (oz === undefined ? undefined : oz * 0.03479);
    if (thicknessMm === undefined) invalid(`Missing copper thickness: ${l.name}`);
    return { kind: 'copper', layer: l.name, thicknessMm };
  });
  const total = layers.reduce((sum, l) => sum + l.thicknessMm, 0);
  if (v.boardThicknessMm !== undefined && Math.abs(total - v.boardThicknessMm) > 1e-6) invalid('boardThicknessMm must equal layer sum, excluding mask (tolerance 1e-6 mm)');
  const routing: RoutingStackup = { ...v, layers };
  return freeze({ kind: 'stackup', input: v, routing });
}

/** Reuse the V2 transport. This is only indexing/reference validation, not a
 * second importer or certification of copper topology/physical readiness. */
export function boardSnapshot(input: RoutingBoardV2): RoutingBoardV2 {
  const safe = json(input);
  if (!object(safe) || !Array.isArray(safe.layers) || !Array.isArray(safe.nets) || !Array.isArray(safe.pads) || !Array.isArray(safe.components) || !Array.isArray(safe.outline) || !safe.copper || !safe.rules) invalid('Expected current RoutingBoardV2 snapshot');
  const result = safe as unknown as RoutingBoardV2;
  if (!result.layers.every(l => l && typeof l.name === 'string') || !result.nets.every(n => n && typeof n.name === 'string') || !result.components.every(c => c && typeof c.designator === 'string') || !result.pads.every(p => p && typeof p.id === 'string' && typeof p.number === 'string' && Array.isArray(p.layers))) invalid('Malformed V2 reference tables');
  return freeze(result);
}

export function sampleWaveform(v: number | Waveform, atS: number): number {
  if (typeof v === 'number') return v;
  if (v.kind === 'step') return atS < v.atS ? v.from : v.riseS === 0 ? v.to : v.from + (v.to - v.from) * Math.min(1, (atS - v.atS) / v.riseS);
  if (v.kind === 'sine') return v.offset + v.amplitude * Math.sin(2 * Math.PI * atS / v.periodS + v.phaseDeg * Math.PI / 180);
  if (v.kind === 'pulse') {
    if (atS < v.delayS || atS >= v.delayS + v.count * v.periodS) return v.low;
    const t = (atS - v.delayS) % v.periodS;
    if (t < v.riseS) return v.low + (v.high - v.low) * t / v.riseS;
    if (t < v.riseS + v.highS) return v.high;
    if (t < v.riseS + v.highS + v.fallS) return v.high + (v.low - v.high) * (t - v.riseS - v.highS) / v.fallS;
    return v.low;
  }
  const t = v.repeat && atS < v.repeat.periodS * v.repeat.count ? atS % v.repeat.periodS : atS;
  for (let i = 1; i < v.points.length; i++) {
    const [time, value] = v.points[i], [previousTime, previous] = v.points[i - 1];
    if (t < time) return v.interpolation === 'hold' ? previous : previous + (value - previous) * (t - previousTime) / (time - previousTime);
  }
  return v.points.at(-1)![1];
}

export function makeProfiles(input: ProfilesInput = {}) {
  const schema = z.strictObject({
    environments: z.record(z.string(), environment).optional(),
    // Detailed stackup/model/rule validation occurs at its use in a builder;
    // catalog categories themselves must still be explicit and finite JSON.
    stackups: z.record(z.string(), z.json()).optional(), cooling: z.record(z.string(), z.json()).optional(),
    checks: z.record(z.string(), z.json()).optional(), numerics: z.record(z.string(), z.json()).optional(),
  });
  const safe = parse(schema, json(input));
  const out = { environments: safe.environments ?? {}, stackups: safe.stackups ?? {}, cooling: safe.cooling ?? {}, checks: safe.checks ?? {}, numerics: safe.numerics ?? {} };
  Object.defineProperties(out, {
    environment: { value: (v: Environment) => freeze(parse(environment, json(v))) },
    sample: { value: (v: { environment: Environment; atS: number }) => {
      const p = parse(z.strictObject({ environment, atS: z.number().finite().nonnegative() }), json(v));
      return sampleWaveform(p.environment.ambientC, p.atS);
    } },
  });
  return freeze(out);
}

export function object(v: Json | unknown): v is Record<string, unknown> { return !!v && typeof v === 'object' && !Array.isArray(v); }
