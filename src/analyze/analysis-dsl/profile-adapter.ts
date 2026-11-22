import type { AnalysisPhysicalProfile } from '../../../native/analyze/index.js';
import type { Material, Materials, StackupDescription } from './spec-doc.js';
import { invalid } from './schema.ts';

export function dcMaterial(m: Material, temperatureC?: number) {
  if (m.conductivitySM === undefined || !m.state?.trim()) invalid('DC material requires conductivitySM and explicit state');
  const law = m.referenceC !== undefined || m.temperatureCoefficientPerK !== undefined;
  if (temperatureC !== undefined && temperatureC < -273.15) invalid('DC temperature must be >= -273.15 °C');
  if (law && (m.referenceC === undefined || m.temperatureCoefficientPerK === undefined || temperatureC === undefined)) invalid('DC temperature law needs referenceC, temperatureCoefficientPerK and temperatureC');
  const factor = law ? 1 + m.temperatureCoefficientPerK! * (temperatureC! - m.referenceC!) : 1;
  if (!Number.isFinite(factor) || factor <= 0) invalid('DC resistivity temperature law is outside its positive range');
  if (m.validity?.status === 'unavailable') invalid('DC material is unavailable');
  return { conductivity_s_m: m.conductivitySM / factor, state: law ? `${m.state}; linear resistivity at ${temperatureC} C, reference ${m.referenceC} C, alpha ${m.temperatureCoefficientPerK} /K; positive-law domain only` : m.state };
}

/** Only translate explicit, compatible fields into the existing conduction
 * profile. Do not select copper by a catalog name, infer a temperature law,
 * invent bore conventions or fill absent material/plating data. Other analyses
 * retain their full typed DSL materials in config for a future executor. */
export function conductionProfile(stackup?: StackupDescription, materials?: Materials, temperatureC?: number): AnalysisPhysicalProfile | undefined {
  if (!stackup) return undefined;
  const profile: AnalysisPhysicalProfile = {
    stackup: stackup.routing.layers.map(layer => layer.kind === 'copper'
      ? { kind: 'copper', layer: layer.layer, thickness_mm: layer.thicknessMm }
      : { kind: 'dielectric', name: layer.name ?? '', thickness_mm: layer.thicknessMm }),
  };
  for (const layer of stackup.input.layers) {
    if (layer.kind !== 'copper' || !layer.material) continue;
    const m = materials?.[layer.material];
    if (m?.conductivitySM !== undefined && m.state) {
      profile.materials ??= {};
      profile.materials[layer.name] = dcMaterial(m, temperatureC);
    }
  }
  if (stackup.routing.viaPlatingThicknessUm !== undefined) profile.barrel = { plating_thickness_mm: stackup.routing.viaPlatingThicknessUm / 1000 };
  if (stackup.input.platingMaterial !== undefined || stackup.input.diameterConvention !== undefined) {
    const material = materials?.[stackup.input.platingMaterial ?? ''];
    if (stackup.input.platingMaterial && !material) invalid(`Missing plating material ${stackup.input.platingMaterial}`);
    profile.barrel = { ...profile.barrel, ...(material ? { material: dcMaterial(material, temperatureC) } : {}), ...(stackup.input.diameterConvention ? { diameter_convention: stackup.input.diameterConvention } : {}) };
  }
  return profile;
}
