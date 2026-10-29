import {LocalModelCache,localCoreIdentity,localExtractionKey} from './local-model-cache.ts';
import { createRequire } from 'node:module';
import { backendResource } from '../runtime/resources.ts';
import type { AnalysisProblem, AnalysisResult } from './types.ts';

let workerModelCache:LocalModelCache|undefined;

/** Used only inside the existing process workers. DSL calls native in its own
 * disposable process, so cancellation cannot orphan a nested native worker. */
export function createProblemExecutor(retainDcForScenario = false) {
  const localCache=workerModelCache??=new LocalModelCache();let coreIdentity:string|undefined;
  let session: import('../../native/analyze/index.js').AnalysisDcSession | undefined;
  let electrical: import('../../native/analyze/index.js').AnalysisElectricalSession | undefined;
  return <P extends AnalysisProblem>(problem: P): AnalysisResult<P> => {
    const addon = createRequire(import.meta.url)(backendResource('native', 'node-addon', 'index.cjs')) as typeof import('../../native/node-addon/index.cjs');
    let result: AnalysisResult;
    if(problem.kind==='local-extraction'){
      electrical ??= new addon.AnalysisElectricalSession();try{const r=problem.request;const requestStart=performance.now();const remaining=()=>{const seconds=(r.remainingSeconds??300)-(performance.now()-requestStart)/1000;if(!Number.isFinite(seconds)||seconds<=0||seconds>300)throw new Error('LOCAL_DEADLINE: shared planning/cache/prepare/solve deadline');return seconds;};
      if(r.bandHz[0]!==1000||r.bandHz[1]!==100000||r.frequenciesHz.some(f=>f<r.bandHz[0]||f>r.bandHz[1]))throw new Error('LOCAL_BAND: unsupported band; explicit validation/rebuild required');
      coreIdentity ??= localCoreIdentity();
      const key=localExtractionKey(localCache,coreIdentity,r);
      const cached=localCache.obtain(key,()=>({...electrical!.prepareLocal(r.boardJson,r.geometry,remaining(),r.magneticPatchLevel),extraction:{schema:1,coreIdentity,magneticPatchLevel:r.magneticPatchLevel??null,bandHz:r.bandHz,units:{position:'m',C:'F',L:'H',R:'Ohm',current:'A',loss:'W'},references:'physical finite differential ports; exterior capacitance retained',validation:{status:'conditional',electric:'accepted-v1',fullReduced:'incomplete'},assumptions:[(r.geometry as {allowInsulatedCopperCuts?:boolean}).allowInsulatedCopperCuts?'insulated ROI cuts':'complete copper islands; no insulated cuts','explicit materials/plating','finite exterior domain','no dielectric loss or arbitrary RF applicability']}}),prepared=>{const p=prepared as any;if(p.extraction?.schema!==1||p.extraction.coreIdentity!==coreIdentity||p.extraction.validation?.status!=='conditional'||p.extraction.magneticPatchLevel!==(r.magneticPatchLevel??null)||JSON.stringify(p.extraction.bandHz)!==JSON.stringify(r.bandHz))throw new Error('LOCAL_CACHE: metadata/version/band refusal');electrical!.solveLocal({prepared,frequenciesHz:[]},remaining());});
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
        coreIdentity ??= localCoreIdentity();const {boardIdentity,budget,...physical}=problem.request;
        const key=localCache.key({core:coreIdentity,kind:'section-v2',request:physical});
        try {const cached=localCache.obtain(key,()=>{const report=electrical!.solveCrossSection({...physical,budget});if(report.status!=='complete'){throw Object.assign(new Error('section rejected'),{sectionReport:report});}return {...report,cacheMetadata:{schema:1,coreIdentity,units:{C:'F/m',L:'H/m',R:'Ohm/m',G:'S/m'},validation:'conditional',bandHz:[physical.frequencies[0],physical.frequencies.at(-1)]}};},value=>{
          const v=value as any;if(v.status!=='complete'||v.cacheMetadata?.coreIdentity!==coreIdentity||v.cacheMetadata?.schema!==1||!v.parameters||!v.network)throw new Error('Section cache shape/version refusal');
          const ports=v.network.ports;const checked=electrical!.solveFrequency({instances:[{id:'cached-section',ports,model:{id:'cached-section',kind:'network',ports,data:{format:'network-parameters',representation:'S',referenceTopology:'common',nature:'passive',frequenciesHz:physical.frequencies,referenceOhm:50,parameters:v.network.parameters}}}],connections:[],ports:ports.map((name:string,index:number)=>({name,index})),frequencies:physical.frequencies,representation:'S',referenceOhm:ports.map(()=>50),budget});if(checked.status!=='complete')throw new Error('Cached section failed native matrix/passivity validation');
        });result={...cached.value,cache:cached.evidence};}catch(error){if(error&&typeof error==='object'&&'sectionReport'in error)result=error.sectionReport as AnalysisResult;else throw error;}
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
