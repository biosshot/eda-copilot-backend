import {z} from 'zod';
import * as S from './schema.ts';
import {freeze,json} from './data.ts';
import {object} from './descriptions.ts';
import type {NetworkResource} from './spec-doc.js';
const positive=z.number().finite().positive();
const reference=z.union([positive,z.record(S.text,positive)]);
const temporal=z.discriminatedUnion('method',[
 z.strictObject({method:z.literal('passive-relaxation'),modeTransform:z.array(z.array(z.number().finite()).max(4)).max(4).optional(),normalizationOhm:reference.optional(),highFrequency:z.strictObject({conductanceS:z.array(z.number().finite().nonnegative()).min(1).max(4),capacitanceF:z.array(z.number().finite().nonnegative()).min(1).max(4)})}),
 z.strictObject({method:z.literal('passive-resonant-y'),modeTransform:z.array(z.array(z.number().finite()).min(1).max(2)).min(1).max(2),normalizationOhm:positive,dcSlopeF:z.array(positive).min(1).max(2),highFrequency:z.strictObject({inverseInductancePerH:z.array(positive).min(1).max(2)})}),
]);
const base={referenceTopology:z.literal('common'),nature:z.enum(['passive','active','unspecified']),temporal:temporal.optional()};
const entry=z.tuple([z.number().int().nonnegative(),z.number().int().nonnegative(),z.number().finite()]);
const matrix=z.array(entry).max(4_000_000);
const terminal=z.union([z.strictObject({node:S.text}),z.strictObject({reference:z.literal('ground')})]);
const contact=z.strictObject({sourcePath:S.text,layer:S.text});
const physicalPort=z.strictObject({positive:contact,negative:contact}).refine(p=>
 p.positive.sourcePath!==p.negative.sourcePath||p.positive.layer!==p.negative.layer);
const sampleAxis=z.array(positive).min(1);
const samples=z.array(z.array(z.array(z.strictObject({real:z.number().finite(),imaginary:z.number().finite()}))));
const data=z.discriminatedUnion('format',[
 z.strictObject({format:z.literal('physical-port-samples'),samplePolicy:z.enum(['exact','rational-hybrid']),directFrequenciesHz:sampleAxis.optional(),representation:z.enum(['S','Y','Z']),
  referenceTopology:z.literal('physical-pairs'),nature:z.literal('passive'),frequenciesHz:sampleAxis,parameters:samples,
  referenceOhm:z.array(positive).min(1).max(16),physicalPorts:z.array(physicalPort).min(1).max(16),producerParameters:samples.optional()}),
 z.strictObject({format:z.literal('delayed-scattering'),referenceTopology:z.literal('common'),nature:z.literal('passive'),referenceOhm:reference,scattering:z.array(z.array(z.number().finite()).max(16)).min(1).max(16),delayS:z.number().finite().nonnegative()}),
 z.strictObject({format:z.literal('coupled-rlc'),referenceTopology:z.enum(['grounded','infinity']),potentialNodes:z.array(S.text).min(1).max(1024),currentDofs:z.array(S.text).max(1536),incidence:matrix,portIncidence:matrix,resistanceOhm:matrix,inductanceH:matrix,capacitanceF:matrix,structuralGaugeModes:z.array(z.array(z.number().finite()).max(1024)).max(1024).optional(),conservedPotentialModes:z.array(z.array(z.number().finite()).max(1024)).max(1024).optional(),portTerminals:z.array(z.strictObject({positive:terminal,negative:terminal})).min(1).max(16).optional()}),
 z.strictObject({...base,format:z.literal('touchstone'),content:z.string().min(1)}),
 z.strictObject({...base,format:z.literal('network-parameters'),representation:z.enum(['S','Z','Y']),frequenciesHz:z.array(z.number().finite().nonnegative()).min(1),parameters:z.array(z.array(z.array(z.strictObject({real:z.number().finite(),imaginary:z.number().finite()})))),referenceOhm:reference.optional()}),
]);
/** Producer applicability is an additional bound, independent of table samples. */
export function declaredNetworkBand(resource:{validity?:unknown}):[number,number]|undefined{
 const domain=object(resource.validity)?resource.validity.domain:undefined;
 if(!object(domain)||domain.bandHz===undefined)return;
 const band=S.parse(z.tuple([z.number().finite().nonnegative(),z.number().finite().nonnegative()]),domain.bandHz);
 if(band[1]<band[0])S.invalid('Network applicability band must increase');
 return band;
}
/** Shared by factory, direct Resource and catalog: admission BEFORE cloning or
 * walking coefficients. Rust alone parses text and checks numerical physics. */
export function networkResource(input:unknown):NetworkResource{
 if(!object(input)||input.kind!=='network'||!Array.isArray(input.ports)||!object(input.data))S.invalid('Network Resource requires kind, ordered ports and data');
 if(typeof input.id!=='string'||!input.id.trim())S.invalid('Network resource id must not be empty');
 if(input.parameters!==undefined)S.invalid('Network resource behavior belongs to data; scalar parameters are unsupported');
 const ports=input.ports,d=input.data,n=ports.length;
 if(n<1||n>16)throw new S.AnalysisDslError('NETWORK_BUDGET','Resource requires 1..16 ports');
 if(ports.some(p=>typeof p!=='string'||!p.trim())||new Set(ports).size!==n)S.invalid('Duplicate/empty resource port');
 if(typeof d.content==='string'&&Buffer.byteLength(d.content)>8*1024*1024)throw new S.AnalysisDslError('NETWORK_BUDGET','Touchstone text exceeds 8 MiB');
 for(const axis of [d.frequenciesHz,d.parameters,d.producerParameters])if(Array.isArray(axis)&&(axis.length>100000||axis.length>262144/n/n))throw new S.AnalysisDslError('NETWORK_BUDGET','Resource frequency/coefficient cap exceeded');
 const tables=[d.parameters,d.producerParameters].filter(Array.isArray);
 if(tables.reduce((count,table)=>count+table.length,0)>262144/n/n)throw new S.AnalysisDslError('NETWORK_BUDGET','Resource coefficient and producer provenance cap exceeded');
 for(const table of tables)if(table.some(m=>!Array.isArray(m)||m.length!==n||m.some(r=>!Array.isArray(r)||r.length!==n)))S.invalid('Matrix dimensions must match Resource.ports');
 if(d.format==='coupled-rlc'){
  if(Array.isArray(d.portTerminals)&&d.portTerminals.length!==n)S.invalid('One physical contact pair per resource port required');
  const arrays=['incidence','portIncidence','resistanceOhm','inductanceH','capacitanceF'];
  if(arrays.reduce((sum,k)=>sum+(Array.isArray(d[k])?d[k].length:0),0)>4_000_000)throw new S.AnalysisDslError('NETWORK_BUDGET','Coupled coefficient cap exceeded before cloning');
 }
 const r=S.parse(S.resource,json(input));declaredNetworkBand(r);const v=S.parse(data,r.data);
 if(v.format==='delayed-scattering'){
  if(v.scattering.length!==n||v.scattering.some(row=>row.length!==n))S.invalid('Delayed scattering dimensions must match Resource.ports');
  const refs=v.referenceOhm;if(object(refs)&&(Object.keys(refs).length!==n||ports.some(p=>!(p in refs))))S.invalid('Delayed power-wave references must match Resource.ports');
 }
 if('temporal'in v&&v.temporal){
  const t=v.temporal;
  if(t.method==='passive-resonant-y'){
   if(n>2||v.format!=='network-parameters'||v.representation!=='Y'||v.nature!=='passive')S.invalid('Resonant temporal class requires authored passive 1..2 port Y tables');
   if(t.dcSlopeF.length!==n||t.highFrequency.inverseInductancePerH.length!==n)S.invalid('Resonant closure dimensions must match Resource.ports');
  }else if(t.highFrequency.conductanceS.length!==n||t.highFrequency.capacitanceF.length!==n)S.invalid('Temporal closure dimensions must match Resource.ports');
  if(t.modeTransform?.some(row=>row.length!==n)||t.modeTransform&&t.modeTransform.length!==n)S.invalid('Temporal mode dimensions must match Resource.ports');
  const refs=v.temporal.normalizationOhm;if(object(refs)&&(Object.keys(refs).length!==n||ports.some(p=>!(p in refs))))S.invalid('Temporal normalization keys must match Resource.ports');
 }
 if(v.format==='physical-port-samples'){
  if(v.frequenciesHz.length!==v.parameters.length||v.frequenciesHz.some((f,i)=>i>0&&f<=v.frequenciesHz[i-1])||
     v.referenceOhm.length!==n||v.physicalPorts.length!==n||
     (v.representation==='S'?v.producerParameters!==undefined:!v.producerParameters||v.producerParameters.length!==v.frequenciesHz.length||v.producerParameters.some(m=>m.length!==n||m.some(row=>row.length!==n))))
   S.invalid('Exact physical samples require a strictly ordered axis, matching matrices, pairs and references');
  if(r.analyses?.some(analysis=>analysis!=='networkParameters'))
   S.invalid('Physical exact-grid samples authorize standalone networkParameters only');
 }
 if(v.format==='network-parameters'){
  if(v.frequenciesHz.length!==v.parameters.length||v.frequenciesHz.some((f,i)=>i>0&&f<=v.frequenciesHz[i-1]))S.invalid('Frequency axis must strictly increase and match parameters');
  if(v.representation==='S'&&v.referenceOhm===undefined||v.representation!=='S'&&v.referenceOhm!==undefined)S.invalid('S requires referenceOhm; Z/Y forbid it');
  const refs=v.referenceOhm;if(object(refs)&&(Object.keys(refs).length!==n||ports.some(p=>!(p in refs))))S.invalid('Reference keys must exactly match Resource.ports');
 }
 return freeze({...r,data:v}) as NetworkResource;
}
