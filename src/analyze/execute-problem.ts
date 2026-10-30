import {LocalModelCache,localCoreIdentity,localExtractionKey,sectionFieldIdentity} from './local-model-cache.ts';
import { createRequire } from 'node:module';
import { backendResource } from '../runtime/resources.ts';
import type { AnalysisProblem, AnalysisResult } from './types.ts';

let workerModelCache:LocalModelCache|undefined;

/** Used only inside the existing process workers. DSL calls native in its own
 * disposable process, so cancellation cannot orphan a nested native worker. */
export function createProblemExecutor(retainDcForScenario = false) {
  const localCache=workerModelCache??=new LocalModelCache();let coreIdentity:string|undefined;let magneticProfile:string|undefined;
  let session: import('../../native/analyze/index.js').AnalysisDcSession | undefined;
  let electrical: import('../../native/analyze/index.js').AnalysisElectricalSession | undefined;
  return <P extends AnalysisProblem>(problem: P): AnalysisResult<P> => {
    const addon = createRequire(import.meta.url)(backendResource('native', 'node-addon', 'index.cjs')) as typeof import('../../native/node-addon/index.cjs');
    let result: AnalysisResult;
    if(problem.kind==='board-section'){electrical ??=new addon.AnalysisElectricalSession();result=electrical.sliceBoardSection(problem.boardJson,problem.stack,problem.slice);}
    else if(problem.kind==='local-extraction'){
      electrical ??= new addon.AnalysisElectricalSession();try{const r=problem.request;const requestStart=performance.now();const remaining=()=>{const seconds=(r.remainingSeconds??300)-(performance.now()-requestStart)/1000;if(!Number.isFinite(seconds)||seconds<=0||seconds>Number(process.env.P7_VERIFY_SECONDS??300))throw new Error('LOCAL_DEADLINE: shared planning/cache/prepare/solve deadline');return seconds;};
      if(r.bandHz[0]!==1000||r.bandHz[1]!==100000||r.frequenciesHz.some(f=>f<r.bandHz[0]||f>r.bandHz[1]))throw new Error('LOCAL_BAND: unsupported band; explicit validation/rebuild required');
      const activeProfile=process.env.P7_MAGNETIC_PROFILE??'baseline';if(magneticProfile!==undefined&&magneticProfile!==activeProfile)throw new Error('LOCAL_CACHE: magnetic profile changed during executor lifetime');coreIdentity ??= localCoreIdentity();magneticProfile=activeProfile;
      const key=localExtractionKey(localCache,coreIdentity,r);
      const cached=localCache.obtain(key,()=>({...electrical!.prepareLocal(r.boardJson,r.geometry,remaining(),r.magneticPatchLevel),extraction:{schema:1,coreIdentity,magneticProfile,magneticPatchLevel:r.magneticPatchLevel??null,bandHz:r.bandHz,units:{position:'m',C:'F',L:'H',R:'Ohm',current:'A',loss:'W'},references:'physical finite differential ports; exterior capacitance retained',validation:{status:'conditional',electric:'accepted-v1',fullReduced:'incomplete'},assumptions:[(r.geometry as {allowInsulatedCopperCuts?:boolean}).allowInsulatedCopperCuts?'insulated ROI cuts':'complete copper islands; no insulated cuts','explicit materials/plating','finite exterior domain','no dielectric loss or arbitrary RF applicability']}}),prepared=>{const p=prepared as any;if(p.extraction?.schema!==1||p.extraction.coreIdentity!==coreIdentity||p.extraction.magneticProfile!==magneticProfile||p.extraction.validation?.status!=='conditional'||p.extraction.magneticPatchLevel!==(r.magneticPatchLevel??null)||JSON.stringify(p.extraction.bandHz)!==JSON.stringify(r.bandHz))throw new Error('LOCAL_CACHE: metadata/version/band refusal');electrical!.solveLocal({prepared,frequenciesHz:[]},remaining());});
      remaining();const response=electrical.solveLocal({prepared:cached.value,frequenciesHz:r.frequenciesHz,drive:r.drive,loadsOhm:r.loadsOhm},remaining(),r.observablesOnly);
      remaining();result={...response,prepared:cached.value,cache:{...cached.evidence,training:cached.evidence.status==='cold'?cached.value.candidate.training.length:0},resources:{peakRssBytes:process.resourceUsage().maxRSS*1024}};
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
        const activeProfile=process.env.P7_MAGNETIC_PROFILE??'baseline';if(magneticProfile!==undefined&&magneticProfile!==activeProfile)throw new Error('LOCAL_CACHE: magnetic profile changed during executor lifetime');coreIdentity ??= localCoreIdentity();magneticProfile=activeProfile;const {boardIdentity,budget,...physical}=problem.request;
        const key=localCache.key({core:coreIdentity,kind:'section-v3',request:physical});
        const sectionStart=performance.now(),seconds=budget?.maxSeconds??Number((physical.line as any)?.geometry?.maxSeconds??120);const sectionBudget=()=>{const left=seconds-(performance.now()-sectionStart)/1000;if(!Number.isFinite(left)||left<=0)throw new Error('SECTION_TIMEOUT: shared cache/field/propagation deadline');return {...budget,maxSeconds:left};};
        try {let validatedSection:any;const cached=localCache.obtain(key,()=>{const fieldKey=localCache.key({core:coreIdentity,kind:'section-field-v1',request:sectionFieldIdentity(physical)});
          const field=localCache.obtain(fieldKey,()=>{const value=electrical!.solveCrossSection({...physical,budget:sectionBudget()});if(value.status!=='complete')throw Object.assign(new Error('section rejected'),{sectionReport:value});return {...value,fieldCacheMetadata:{schema:1,coreIdentity,key:fieldKey}};},value=>{const v=value as any;if(v.fieldCacheMetadata?.schema!==1||v.fieldCacheMetadata.coreIdentity!==coreIdentity||v.fieldCacheMetadata.key!==fieldKey||localCache.key({core:coreIdentity,kind:'section-field-v1',request:sectionFieldIdentity({line:v.input,mesh:v.fieldRequestMesh??undefined})})!==fieldKey)throw new Error('Section field cache identity refusal');const checked=electrical!.propagateCrossSection({line:v.input,frequencies:v.frequenciesHz,mesh:v.fieldRequestMesh??undefined,budget:sectionBudget()},value);if(checked.status!=='complete')throw new Error('Stored section coefficients failed native validation');});
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
