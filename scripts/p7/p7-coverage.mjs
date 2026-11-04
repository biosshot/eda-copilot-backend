import { fileURLToPath } from 'node:url';
import { readFileSync, statSync } from 'node:fs';
import { resolve, isAbsolute, relative, sep } from 'node:path';

const matrix = [
  'continuous-ground-line','90-degree-bend','45-degree-trace','explicit-coupled-pair','ground-gap-under-trace','explicit-stripline','automatic-real-board-dual-ground','dual-ground-with-gaps','via-fence','nearby-return-via','signal-via-layer-change','branches','remote-ground-connection','no-ground-under-trace','multiple-grounds','wide-return','surrounding-copper','bandamp-rf-section','simultaneous-top-bottom-ground-gaps','blind-buried-via','connector-pad-transition','differential-pair','reference-swap','return-search-without-local-reference','physical-ground-net-bridge','multiple-reference-transitions',
  'capacitive-mixed-return-gap','component-bridge-rc-ferrite','pdn-multiport','small-coupled-branch-meander-group','stackups-2-4-6-8-asymmetry-finite-edges','dsl-adaptive-frequency-coverage','universality-invariants',
  'same-net-gap-with-bypass','agnd-to-gnd-physical-bridge','multiple-agnd-pgnd-transitions','signal-and-return-via-reference-change','galvanically-open-capacitive-gap','reference-change-without-found-connection',
  'short-wide-reference-bridge','long-narrow-reference-bridge','board-edge-reference-connection'
];
export const manifest = JSON.parse(readFileSync(new URL('./coverage-manifest.json', import.meta.url), 'utf8'));
export function validateManifest(data, root = new URL('../../', import.meta.url)) {
  root = root instanceof URL ? fileURLToPath(root) : resolve(root);
  if (data?.schemaVersion !== 1 || !Array.isArray(data?.cases)) return ['manifest must have schemaVersion 1 and cases array'];
  const errors=[], seen=new Set();
  for(const c of data.cases){
    if(!c || typeof c !== 'object'){errors.push('invalid case');continue;}
    if(typeof c.id!=='string'||!matrix.includes(c.id)) errors.push('unknown case id: '+c.id);
    if(seen.has(c.id)) errors.push('duplicate id: '+c.id); seen.add(c.id);
    if(!['todo','ready'].includes(c.status)) errors.push(c.id+': invalid preparation status');
    if(c.acceptance!==null) errors.push(c.id+': acceptance requires a separate reviewed evidence workflow');
    for(const k of ['scenario','test','evidence']) {
      const v=c[k]; if(v===null)continue;
      if(typeof v!=='string'||!v){errors.push(c.id+': invalid '+k);continue;}
      const rel=relative(root,resolve(root,v));
      if(isAbsolute(v)||rel==='..'||rel.startsWith('..'+sep))errors.push(c.id+': nonlocal path');
      else {try{if(!statSync(resolve(root,v)).isFile())errors.push(c.id+': not a file');}catch{errors.push(c.id+': missing '+k);}}
    }
    if(c.status==='ready'&&(!c.scenario||!c.test)) errors.push(c.id+': ready requires scenario and test');
  }
  for(const id of matrix)if(!seen.has(id))errors.push('missing case: '+id);
  return errors;
}
/** A gate result only; pass does not establish model acceptance. Inputs are percent units. */
export function classifyReview({errorPercent,upperThresholdPercent,kind='sensitivity',failure=null}) {
  if(failure!==null || !Number.isFinite(errorPercent)||errorPercent<0||!Number.isFinite(upperThresholdPercent)||upperThresholdPercent<0) return 'fail';
  if(!['sensitivity','physical-error','invariant'].includes(kind))return 'fail';
  if(errorPercent<=upperThresholdPercent)return 'pass';
  return kind!=='invariant'&&errorPercent<=upperThresholdPercent+1.5?'requires-review':'fail';
}
if(process.argv[1] && resolve(process.argv[1])===fileURLToPath(import.meta.url)) {
 const errors=validateManifest(manifest);
 if(errors.length){console.error(errors.join('\n'));process.exitCode=1;}
 else console.log(`ready=${manifest.cases.filter(c=>c.status==='ready').length} pending=${manifest.cases.filter(c=>c.status==='todo').length} (preparation only; not acceptance)`);
}
