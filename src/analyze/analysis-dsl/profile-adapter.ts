import type { AnalysisPhysicalProfile } from '../../../native/analyze/index.js';
import type { Materials, StackupDescription } from './spec-doc.js';

/** Only translate explicit, compatible fields into the existing conduction
 * profile. Do not select copper by a catalog name, infer a temperature law,
 * invent bore conventions or fill absent material/plating data. Other analyses
 * retain their full typed DSL materials in config for a future executor. */
export function conductionProfile(stackup?: StackupDescription, materials?: Materials): AnalysisPhysicalProfile | undefined {
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
      profile.materials[layer.name] = { conductivity_s_m: m.conductivitySM, state: m.state };
    }
  }
  if (stackup.routing.viaPlatingThicknessUm !== undefined) profile.barrel = { plating_thickness_mm: stackup.routing.viaPlatingThicknessUm / 1000 };
  return profile;
}
