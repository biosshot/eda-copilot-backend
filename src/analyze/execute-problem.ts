import {LocalModelCache,localCoreIdentity,magneticAxialTetPolicy,magneticPrecision as selectedMagneticPrecision,localExtractionKey,sectionFieldIdentity} from './local-model-cache.ts';
import { cachedBoardFoilReport } from './board-foil-cache.ts';
import { createRequire } from 'node:module';
import { backendResource } from '../runtime/resources.ts';
import type { AnalysisProblem, AnalysisResult } from './types.ts';

let workerModelCache:LocalModelCache|undefined;

function validateChannelMagneticCache(value:any,physical:any,budget:any){
 const memory=budget?.maxMemoryMiB??1024,seconds=budget?.maxSeconds??300;
 if(!Number.isFinite(seconds)||seconds<=0||!Number.isFinite(memory)||memory<=0||memory>65536)throw new Error('CHANNEL_MAGNETIC_BUDGET: finite positive bounded budgets required');
 if(value?.status!=='complete'||value.quadratureOrder!==(physical.order??12)||!Number.isFinite(value.plannedBytes)||value.plannedBytes<0||value.plannedBytes>memory*1048576||!Array.isArray(value.couplings)||!Array.isArray(value.selfMatrices))throw new Error('CHANNEL_MAGNETIC_CACHE: complete admitted result required');
 const blocks=physical.blocks,couplings=value.couplings,self=value.selfMatrices;
 if(couplings.length!==blocks.length*(blocks.length-1)/2||self.length!==blocks.filter((b:any)=>b.directSelf).length)throw new Error('CHANNEL_MAGNETIC_CACHE: block coverage changed');
 let ci=0,si=0;for(let a=0;a<blocks.length;a++){
  if(blocks[a].directSelf){const m=self[si++];if(m.id!==blocks[a].id||m.matrixH.length!==blocks[a].dofs||m.matrixH.some((r:any)=>!Array.isArray(r)||r.length!==blocks[a].dofs||r.some((v:any)=>!Number.isFinite(v))))throw new Error('CHANNEL_MAGNETIC_CACHE: invalid self matrix');}
  for(let b=0;b<a;b++){const m=couplings[ci++];if(m.a!==blocks[a].id||m.b!==blocks[b].id||m.matrixH.length!==blocks[a].dofs||m.matrixH.some((r:any)=>!Array.isArray(r)||r.length!==blocks[b].dofs||r.some((v:any)=>!Number.isFinite(v))))throw new Error('CHANNEL_MAGNETIC_CACHE: invalid coupling matrix');}
 }
}

/** Used only inside the existing process workers. DSL calls native in its own
 * disposable process, so cancellation cannot orphan a nested native worker. */
export function createProblemExecutor(retainDcForScenario = false) {
  const localCache=workerModelCache??=new LocalModelCache();let coreIdentity:string|undefined;let magneticProfile:string|undefined;let magneticPrecision:string|undefined;
  let axialTetPolicy:ReturnType<typeof magneticAxialTetPolicy>;
  let session: import('../../native/analyze/index.js').AnalysisDcSession | undefined;
  let electrical: import('../../native/analyze/index.js').AnalysisElectricalSession | undefined;
  const checkedCoreIdentity = () => {
    const activeProfile=process.env.P7_MAGNETIC_PROFILE??'baseline';
    if(magneticProfile!==undefined&&magneticProfile!==activeProfile)throw new Error('LOCAL_CACHE: magnetic profile changed during executor lifetime');
    const activePrecision=selectedMagneticPrecision();
    if(magneticPrecision!==undefined&&magneticPrecision!==activePrecision)throw new Error('LOCAL_CACHE: magnetic precision changed during executor lifetime');
    const activeAxial=magneticAxialTetPolicy();
    if(coreIdentity!==undefined&&axialTetPolicy!==activeAxial)throw new Error('LOCAL_CACHE: axial tetrahedron policy changed during executor lifetime');
    coreIdentity ??= localCoreIdentity();magneticProfile=activeProfile;magneticPrecision=activePrecision;axialTetPolicy=activeAxial;
    return coreIdentity;
  };
  return <P extends AnalysisProblem>(problem: P): AnalysisResult<P> => {
    const loadAddon = () => createRequire(import.meta.url)(backendResource('native', 'node-addon', 'index.cjs')) as typeof import('../../native/node-addon/index.cjs');
    if(problem.kind==='board-foil-checked'){
      const started=performance.now();
      try {
        return cachedBoardFoilReport(localCache,checkedCoreIdentity(),problem,request=>{
          electrical ??= new (loadAddon().AnalysisElectricalSession)();
          return electrical.solveBoardFoilChecked(problem.boardJson,problem.geometry,request);
        },started) as AnalysisResult<P>;
      }
      finally{if(!retainDcForScenario&&electrical){electrical.clear();electrical=undefined;}}
    }
    const addon = loadAddon();
    let result: AnalysisResult;
    if(problem.kind==='board-section'){electrical ??=new addon.AnalysisElectricalSession();result=electrical.sliceBoardSection(problem.boardJson,problem.stack,problem.slice);}
    else if(problem.kind==='local-dc-extraction'){
      electrical ??= new addon.AnalysisElectricalSession();
      try { result=electrical.solveLocalDc(problem.boardJson,problem.request,problem.remainingSeconds); }
      finally{if(!retainDcForScenario){electrical.clear();electrical=undefined;}}
    }
    else if(problem.kind==='local-extraction'){
      electrical ??= new addon.AnalysisElectricalSession();try{const r=problem.request;const requestStart=performance.now();const remaining=()=>{const seconds=(r.remainingSeconds??1800)-(performance.now()-requestStart)/1000;if(!Number.isFinite(seconds)||seconds<=0||(process.env.P7_VERIFY_SECONDS!==undefined&&seconds>Number(process.env.P7_VERIFY_SECONDS)))throw new Error('LOCAL_DEADLINE: shared planning/cache/prepare/solve deadline');return seconds;};
      if(r.bandHz[0]!==1000||r.bandHz[1]!==100000||r.frequenciesHz.some(f=>f<r.bandHz[0]||f>r.bandHz[1]))throw new Error('LOCAL_BAND: unsupported band; explicit validation/rebuild required');
      const core=checkedCoreIdentity();
      const key=localExtractionKey(localCache,core,r);
      const prepare=()=>({...electrical!.prepareLocal(r.boardJson,r.geometry,remaining(),r.magneticPatchLevel,r.frequencyAudit),extraction:{schema:1,coreIdentity,magneticProfile,magneticPrecision,magneticPatchLevel:r.magneticPatchLevel??null,bandHz:r.bandHz,units:{position:'m',C:'F',L:'H',R:'Ohm',current:'A',loss:'W'},references:'physical finite differential ports; exterior capacitance retained',validation:{status:'conditional',electric:'accepted-v1',fullReduced:'incomplete'},assumptions:[(r.geometry as {allowInsulatedCopperCuts?:boolean}).allowInsulatedCopperCuts?'insulated ROI cuts':'complete copper islands; no insulated cuts','explicit materials/plating','finite exterior domain','no dielectric loss or arbitrary RF applicability']}});
      const validate=(prepared:unknown)=>{const p=prepared as any;if(p.extraction?.schema!==1||p.extraction.coreIdentity!==coreIdentity||p.extraction.magneticProfile!==magneticProfile||p.extraction.magneticPrecision!==magneticPrecision||p.extraction.validation?.status!=='conditional'||p.extraction.magneticPatchLevel!==(r.magneticPatchLevel??null)||JSON.stringify(p.extraction.bandHz)!==JSON.stringify(r.bandHz))throw new Error('LOCAL_CACHE: metadata/version/band refusal');electrical!.solveLocal({prepared,frequenciesHz:[]},remaining());};
      // Independent holdouts need the live full operator. This request-scoped
      // evidence is deliberately separate from the reusable reduced cache.
      const cached=r.frequencyAudit?{
        value:prepare(),evidence:{status:'cold' as const,key,schema:1,assembly:1,training:1,restoreSeconds:0,writeFailed:false}
      }:localCache.obtain(key,prepare,validate);
      if(r.frequencyAudit&&!cached.value.frequencyAudit)throw new Error('LOCAL_FREQUENCY_AUDIT: independent native evidence missing');
      if(process.env.P7_CACHE_DEBUG==='1')process.stderr.write('P7_CACHE '+JSON.stringify({dir:localCache.dir,key,status:cached.evidence.status,writeFailed:cached.evidence.writeFailed})+'\n');
      const {frequencyAudit,...prepared}=cached.value;
      remaining();const response=electrical.solveLocal({prepared,frequenciesHz:r.frequenciesHz,drive:r.drive,loadsOhm:r.loadsOhm},remaining(),r.observablesOnly);
      remaining();
      const auditEvidence=frequencyAudit?{...frequencyAudit,
        key:localCache.key({producer:'local-frequency-audit-v1',extractionKey:key,policy:r.frequencyAudit,
          intrinsic:true,drive:'all physical unit-port drives',loads:[],outputDrive:r.drive,outputLoads:r.loadsOhm}),
        storage:'request-result'
      }:undefined;
      result={...response,prepared,...(auditEvidence?{frequencyAudit:auditEvidence}:{}),cache:{...cached.evidence,training:cached.evidence.status==='cold'?cached.value.candidate.training.length:0},resources:{peakRssBytes:process.resourceUsage().maxRSS*1024}};
      }catch(error){if(error&&typeof error==='object')Object.assign(error,{localResources:{peakRssBytes:process.resourceUsage().maxRSS*1024,metric:'OS worker process high-water RSS including Node and preceding native work'}});throw error;}finally{if(!retainDcForScenario){electrical.clear();electrical=undefined;}}
    }
    else if(problem.kind==='transient-network'){
      electrical ??= new addon.AnalysisElectricalSession();
      try {result=electrical.solveTransient(problem.request);}
      finally{if(!retainDcForScenario){electrical.clear();electrical=undefined;}}
    }
    else if (problem.kind === 'geometry-validation') result = addon.validateAnalysisGeometry(problem.request);
    else if (problem.kind === 'cross-section') {
      electrical ??= new addon.AnalysisElectricalSession();
      try {
        const core=checkedCoreIdentity();const {boardIdentity,budget,...physical}=problem.request;
        const key=localCache.key({core,kind:'section-v3',request:physical});
        const sectionStart=performance.now(),seconds=budget?.maxSeconds??Number((physical.line as any)?.geometry?.maxSeconds??120);const sectionBudget=()=>{const left=seconds-(performance.now()-sectionStart)/1000;if(!Number.isFinite(left)||left<=0)throw new Error('SECTION_TIMEOUT: shared cache/field/propagation deadline');return {...budget,maxSeconds:left};};
        try {let validatedSection:any;const cached=localCache.obtain(key,()=>{const fieldKey=localCache.key({core,kind:'section-field-v1',request:sectionFieldIdentity(physical)});
          const field=localCache.obtain(fieldKey,()=>{const value=electrical!.solveCrossSection({...physical,budget:sectionBudget()});if(value.status!=='complete')throw Object.assign(new Error('section rejected'),{sectionReport:value});return {...value,fieldCacheMetadata:{schema:1,coreIdentity,key:fieldKey}};},value=>{const v=value as any;if(v.fieldCacheMetadata?.schema!==1||v.fieldCacheMetadata.coreIdentity!==coreIdentity||v.fieldCacheMetadata.key!==fieldKey||localCache.key({core,kind:'section-field-v1',request:sectionFieldIdentity({line:v.input,mesh:v.fieldRequestMesh??undefined})})!==fieldKey)throw new Error('Section field cache identity refusal');const checked=electrical!.propagateCrossSection({line:v.input,frequencies:v.frequenciesHz,mesh:v.fieldRequestMesh??undefined,budget:sectionBudget()},value);if(checked.status!=='complete')throw new Error('Stored section coefficients failed native validation');});
          const report=electrical!.propagateCrossSection({...physical,budget:sectionBudget()},field.value);if(report.status!=='complete')throw Object.assign(new Error('section replay rejected'),{sectionReport:report});
          return {...report,fieldCache:field.evidence,cacheMetadata:{schema:1,coreIdentity,units:{C:'F/m',L:'H/m',R:'Ohm/m',G:'S/m'},validation:'conditional',bandHz:[physical.frequencies[0],physical.frequencies.at(-1)]}};},value=>{
          const v=value as any;if(v.status!=='complete'||v.cacheMetadata?.coreIdentity!==coreIdentity||v.cacheMetadata?.schema!==1||!v.parameters)throw new Error('Section cache shape/version refusal');
          const rechecked=electrical!.propagateCrossSection({...physical,budget:sectionBudget()},v);if(rechecked.status!=='complete')throw new Error('Restored section coefficients refused');validatedSection=rechecked;if(!rechecked.network)return;const ports=rechecked.network.ports;const checked=electrical!.solveFrequency({instances:[{id:'cached-section',ports,model:{id:'cached-section',kind:'network',ports,data:{format:'network-parameters',representation:'S',referenceTopology:'common',nature:'passive',frequenciesHz:physical.frequencies,referenceOhm:50,parameters:rechecked.network.parameters}}}],connections:[],ports:ports.map((name:string,index:number)=>({name,index})),frequencies:physical.frequencies,representation:'S',referenceOhm:ports.map(()=>50),budget:sectionBudget()});if(checked.status!=='complete')throw new Error('Cached section failed native matrix/passivity validation');
        });result={...validatedSection,resources:{peakRssBytes:process.resourceUsage().maxRSS*1024,metric:'OS worker high-water RSS including preceding requests'},cache:{...cached.evidence,assembly:cached.evidence.status==='cold'?(cached.value as any).fieldCache.assembly:0,training:0,field:cached.evidence.status==='cold'?(cached.value as any).fieldCache:{status:'not-consulted-result-hit',assembly:0}}};}catch(error){if(error&&typeof error==='object'&&'sectionReport'in error)result={...(error.sectionReport as any),resources:{peakRssBytes:process.resourceUsage().maxRSS*1024,metric:'OS worker high-water RSS including rejected section preparation'}} as AnalysisResult;else throw error;}
      }
      finally {if(!retainDcForScenario){electrical.clear();electrical=undefined;}}
    }
    else if (problem.kind === 'frequency-network') {
      electrical ??= new addon.AnalysisElectricalSession();
      try { result=electrical.solveFrequency(problem.request); }
      finally {if(!retainDcForScenario){electrical.clear();electrical=undefined;}}
    }
    else if (problem.kind === 'channel-magnetic') {
      electrical ??= new addon.AnalysisElectricalSession();
      try {
        const {budget,...physical}=problem.request as any;const core=checkedCoreIdentity();
        const key=localCache.key({core,kind:'channel-magnetic-v1',request:physical});
        const cached=localCache.obtain(key,()=>electrical!.solveChannelMagnetic(problem.request),value=>validateChannelMagneticCache(value,physical,budget));
        result={...cached.value,elapsedMs:cached.evidence.status==='cold'?cached.value.elapsedMs:cached.evidence.restoreSeconds*1000,cache:cached.evidence};
      }
      finally {if(!retainDcForScenario){electrical.clear();electrical=undefined;}}
    }
    else if (problem.kind === 'electrical-network') {
      electrical ??= new addon.AnalysisElectricalSession();
      try { result=electrical.solve(problem.request); }
      finally {if(!retainDcForScenario){electrical.clear();electrical=undefined;}}
    }
    else if (problem.kind === 'dc') {
      session ??= new addon.AnalysisDcSession();
      try { result = session.solve(problem.request); }
      finally { if (!retainDcForScenario) { session.clear(); session=undefined; } }
    }
    else if (problem.kind === 'prepare-conduction') result = addon.prepareAnalysisConduction(problem.boardJson, { geometry: problem.geometry, mesh: problem.mesh, profile: problem.profile });
    else if (problem.kind === 'prepare-geometry') result = addon.prepareAnalysisGeometry(problem.boardJson, problem.geometry);
    else result = addon.importAnalysisBoard(problem.boardJson);
    return result as AnalysisResult<P>;
  };
}
