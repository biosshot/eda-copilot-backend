import {createHash,randomUUID} from 'node:crypto';
import {mkdirSync,readFileSync,writeFileSync,renameSync,unlinkSync,readdirSync,statSync,utimesSync,openSync,readSync,closeSync,fstatSync} from 'node:fs';
import {join} from 'node:path';
import {homedir} from 'node:os';
import {createRequire} from 'node:module';
import {backendResource} from '../runtime/resources.ts';
export function canonical(value:unknown):string {
 if(value===null||typeof value==='boolean'||typeof value==='string')return JSON.stringify(value);
 if(typeof value==='number'&&Number.isFinite(value))return JSON.stringify(value);
 if(Array.isArray(value))return '['+value.map(canonical).join(',')+']';
 if(value&&typeof value==='object')return '{'+Object.entries(value).filter(([,v])=>v!==undefined).sort(([a],[b])=>a.localeCompare(b,'en')).map(([k,v])=>JSON.stringify(k)+':'+canonical(v)).join(',')+'}';
 throw new Error('Cache identity must contain finite JSON');
}
export const digest=(text:string|Buffer)=>createHash('sha256').update(text).digest('hex');
function digestFile(path:string){
 const fd=openSync(path,'r'),hash=createHash('sha256'),chunk=Buffer.allocUnsafe(64*1024);
 try{for(let n=readSync(fd,chunk);n>0;n=readSync(fd,chunk))hash.update(chunk.subarray(0,n));}
 finally{closeSync(fd);}
 return hash.digest('hex');
}
export function magneticPrecision(){const value=process.env.P7_MAGNETIC_PRECISION??'f64';if(!['f64'].includes(value))throw new Error('LOCAL_SPATIAL_INPUT: unsupported P7_MAGNETIC_PRECISION');return value;}
export function magneticAxialTetPolicy(){const value=process.env.P7_MAGNETIC_AXIAL_TETS??'0';if(!['0','1'].includes(value))throw new Error('LOCAL_SPATIAL_INPUT: unsupported P7_MAGNETIC_AXIAL_TETS');return value==='1'?'axial-tetra-estimated-v1' as const:undefined;}
export function localCoreIdentity(){const require=createRequire(import.meta.url);const {nativeFilename}=require(backendResource('native','node-addon','platform.cjs'));const binary=digestFile(backendResource('native','node-addon',nativeFilename()));const profile=process.env.P7_MAGNETIC_PROFILE??'baseline';if(!['baseline','compact-v1'].includes(profile))throw new Error('LOCAL_SPATIAL_INPUT: unsupported P7_MAGNETIC_PROFILE');const axial=magneticAxialTetPolicy();return digest(canonical({binary,magneticProfile:profile,magneticPrecision:magneticPrecision(),...(axial&&{nearQuadraturePolicy:axial})}));}
export interface CacheEvidence {status:'cold'|'memory'|'disk';key:string;schema:1;assembly:number;training:number;restoreSeconds:number;writeFailed:boolean}
/** Reduced models, recovery and checked outputs only. Native FEM factors never cross this boundary.
 * Each producer validates restored coefficients or response evidence before reuse. */
export class LocalModelCache {
 private entries=new Map<string,{text:string;bytes:number}>();private bytes=0;
 constructor(readonly dir=process.env.PCB_ANALYSIS_CACHE_DIR??join(homedir(),'.cache','eda-copilot','local-v1'),readonly maxMemoryBytes=64*1024*1024,readonly maxDiskBytes=256*1024*1024,readonly maxEntryBytes=24*1024*1024,readonly maxEntries=8){try{for(const name of readdirSync(dir)){if(!/^[a-f0-9]{64}\.json\.[0-9]+\.[a-f0-9-]+\.tmp$/.test(name))continue;const path=join(dir,name);try{if(Date.now()-statSync(path).mtimeMs>60000)unlinkSync(path);}catch{}}}catch{}}
 key(identity:unknown){return digest(canonical({schema:1,method:'direct-sparse-FEM-F64-electric-v1',identity}));}
 clear(){this.entries.clear();this.bytes=0;}
 private path(key:string){if(!/^[a-f0-9]{64}$/.test(key))throw new Error('Invalid extraction key');return join(this.dir,key+'.json');}
 private remember(key:string,text:string){const size=Buffer.byteLength(text);const old=this.entries.get(key);if(old){this.bytes-=old.bytes;this.entries.delete(key);}if(size>this.maxMemoryBytes||size>this.maxEntryBytes||this.maxEntries<1)return;
 while(this.entries.size>=this.maxEntries||this.bytes+size>this.maxMemoryBytes){const first=this.entries.keys().next().value;if(first===undefined)break;this.bytes-=this.entries.get(first)!.bytes;this.entries.delete(first);}this.entries.set(key,{text,bytes:size});this.bytes+=size;}
 private decode(text:string,key:string,validate:(v:unknown)=>void){if(Buffer.byteLength(text)>this.maxEntryBytes)throw new Error('oversized cache');const e=JSON.parse(text);if(e.schema!==1||e.key!==key||typeof e.payload!=='string'||e.checksum!==digest(e.payload))throw new Error('cache envelope rejected');const value=JSON.parse(e.payload);validate(value);return value;}
 obtain<T>(key:string,prepare:()=>T,validate:(v:unknown)=>void,maxEncodedBytes=Infinity):{value:T;evidence:CacheEvidence}{
  const start=performance.now();let value:T|undefined;let status:CacheEvidence['status']='cold';let text=this.entries.get(key)?.text;

  const admit=(size:number)=>{if(size>maxEncodedBytes)throw new Error('LOCAL_CACHE_MEMORY_LIMIT: encoded result exceeds the current output budget');};
  if(text){admit(Buffer.byteLength(text));try{value=this.decode(text,key,validate);status='memory';this.remember(key,text);}catch{const old=this.entries.get(key);if(old)this.bytes-=old.bytes;this.entries.delete(key);}}
  if(value===undefined){
   const path=this.path(key);let fd:number|undefined;
   try{fd=openSync(path,'r');}catch{/* no saved entry */}
   if(fd!==undefined){
    try{
     const stats=fstatSync(fd);
     if(stats.isFile()&&stats.size>=2&&stats.size<=this.maxEntryBytes){
      admit(stats.size);
      try{text=readFileSync(fd,'utf8');value=this.decode(text,key,validate);status='disk';this.remember(key,text);try{const now=new Date();utimesSync(path,now,now);}catch{}}catch{/* corrupt/incompatible entries are safe misses */}
     }
    }finally{closeSync(fd);}
   }
  }
  const restoreSeconds=(performance.now()-start)/1000;let writeFailed=false;
  if(value===undefined){value=prepare();validate(value);const payload=JSON.stringify(value);text=JSON.stringify({schema:1,key,checksum:digest(payload),payload});admit(Buffer.byteLength(text));if(Buffer.byteLength(text)<=this.maxEntryBytes){this.remember(key,text);let temp:string|undefined;try{mkdirSync(this.dir,{recursive:true});let oldSize=0;try{oldSize=statSync(this.path(key)).size;}catch{}if(!this.evict(Math.max(0,Buffer.byteLength(text)-oldSize)))throw new Error('disk cache capacity unavailable');temp=this.path(key)+'.'+process.pid+'.'+randomUUID()+'.tmp';writeFileSync(temp,text,{flag:'wx'});renameSync(temp,this.path(key));temp=undefined;if(!this.evict())writeFailed=true;}catch{writeFailed=true;}finally{if(temp)try{unlinkSync(temp);}catch{}}}}
  return {value,evidence:{status,key,schema:1,assembly:status==='cold'?1:0,training:status==='cold'?1:0,restoreSeconds,writeFailed}};
 }
 private evict(reserveBytes=0):boolean{let files:{path:string;bytes:number;time:number}[]=[];try{for(const name of readdirSync(this.dir)){if(!/^[a-f0-9]{64}\.json$/.test(name))continue;try{const path=join(this.dir,name),s=statSync(path);if(s.isFile())files.push({path,bytes:s.size,time:s.mtimeMs});}catch{}}}catch{return false;}
 files.sort((a,b)=>a.time-b.time);let total=files.reduce((s,f)=>s+f.bytes,0),count=files.length;for(const file of files){if(total<=this.maxDiskBytes-reserveBytes&&count<=(reserveBytes?511:512))break;try{unlinkSync(file.path);total-=file.bytes;count--;}catch{}}
 return total<=this.maxDiskBytes-reserveBytes&&count<=(reserveBytes?511:512);
 }
}

/** Single producer identity boundary; excitation, loads, plots and checks are results. */
export function localExtractionKey(cache:LocalModelCache,core:string,r:{boardJson:string;geometry:unknown;bandHz:readonly number[];temperatureC?:number;magneticPatchLevel?:1|2;frequencyAudit?:{intervalHz:readonly [number,number]}}){return cache.key({core,board:digest(r.boardJson),geometry:canonicalLocalGeometry(r.geometry),bandHz:r.bandHz,temperatureC:r.temperatureC,magneticPatchLevel:r.magneticPatchLevel,copperSkinMesh:r.frequencyAudit&&{version:3,requestedUpperHz:r.frequencyAudit.intervalHz[1],upperPolicy:'max-requested-and-native-training-v1'},producer:'local-recovery-v1'});}

/** Field coefficients depend on physical transverse geometry/materials/mesh.
 * Propagation length and requested samples do not alter that operator. */
export function sectionFieldIdentity(request:{line:unknown;mesh?:unknown}){
 const line:any=structuredClone(request.line);if(line&&typeof line==='object'){delete line.lengthMm;if(line.geometry?.loss)delete line.geometry.loss.frequenciesHz;}
 const mesh=request.mesh&&typeof request.mesh==='object'?Object.fromEntries(Object.entries(request.mesh).filter(([,v])=>v!==null&&v!==undefined)):request.mesh;
 return {producer:'section-field-integrals-v1',line,mesh};
}

/** Match native physical electrode identity without repairing invalid requests. */
export function canonicalLocalGeometry(geometry:unknown){
 if(!geometry||typeof geometry!=='object')return geometry;
 const value=structuredClone(geometry) as any;
 if(Array.isArray(value.electrodes))value.electrodes=value.electrodes.map((electrode:any)=>{
  const e={...electrode};const hasSingle=typeof e.sourcePath==='string',hasUnion=Array.isArray(e.sourcePaths);
  if(hasSingle!==hasUnion){const paths=hasSingle?[e.sourcePath]:e.sourcePaths;if(paths.every((p:unknown)=>typeof p==='string')){e.sourcePaths=[...new Set(paths)].sort();delete e.sourcePath;}}
  return e;
 });
 return value;
}
