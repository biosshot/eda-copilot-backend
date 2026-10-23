import {z} from 'zod';
import * as S from './schema.ts';
import {freeze,json} from './data.ts';
import {object} from './descriptions.ts';
import type {NetworkResource} from './spec-doc.js';
const positive=z.number().finite().positive();
const reference=z.union([positive,z.record(S.text,positive)]);
const base={referenceTopology:z.literal('common'),nature:z.enum(['passive','active','unspecified'])};
const data=z.discriminatedUnion('format',[
 z.strictObject({...base,format:z.literal('touchstone'),content:z.string().min(1)}),
 z.strictObject({...base,format:z.literal('network-parameters'),representation:z.enum(['S','Z','Y']),frequenciesHz:z.array(z.number().finite().nonnegative()).min(1),parameters:z.array(z.array(z.array(z.strictObject({real:z.number().finite(),imaginary:z.number().finite()})))),referenceOhm:reference.optional()}),
]);
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
 for(const axis of [d.frequenciesHz,d.parameters])if(Array.isArray(axis)&&(axis.length>100000||axis.length>262144/n/n))throw new S.AnalysisDslError('NETWORK_BUDGET','Resource frequency/coefficient cap exceeded');
 if(Array.isArray(d.parameters)&&d.parameters.some(m=>!Array.isArray(m)||m.length!==n||m.some(r=>!Array.isArray(r)||r.length!==n)))S.invalid('Matrix dimensions must match Resource.ports');
 const r=S.parse(S.resource,json(input));const v=S.parse(data,r.data);
 if(v.format==='network-parameters'){
  if(v.frequenciesHz.length!==v.parameters.length||v.frequenciesHz.some((f,i)=>i>0&&f<=v.frequenciesHz[i-1]))S.invalid('Frequency axis must strictly increase and match parameters');
  if(v.representation==='S'&&v.referenceOhm===undefined||v.representation!=='S'&&v.referenceOhm!==undefined)S.invalid('S requires referenceOhm; Z/Y forbid it');
  const refs=v.referenceOhm;if(object(refs)&&(Object.keys(refs).length!==n||ports.some(p=>!(p in refs))))S.invalid('Reference keys must exactly match Resource.ports');
 }
 return freeze({...r,data:v}) as NetworkResource;
}
