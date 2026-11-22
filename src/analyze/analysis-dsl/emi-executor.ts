import type {AnalysisTask,ExecutionResult} from './executor.ts';
import type * as T from './spec-doc.js';
import {object} from './descriptions.ts';
import {AnalysisDslError} from './schema.ts';
import {createTransientExecutor} from './transient-executor.ts';

/** Linear-driver levels are used only by bit stimuli; waveform stimuli ignore them. Schema still needs highV > lowV. */
const DRIVER_LOW_V=0,DRIVER_HIGH_V=1;
/** Direct DFT cost guard: bins * samples multiply-adds. */
const MAX_SPECTRUM_OPERATIONS=2e8;

type Series={kind:'series';unit:string;axis:{name:string;unit:string;values:number[]};values:(number|null)[];validity:{status:string;reason?:string};diagnostics:unknown[]};

function refuse(code:string,message:string):never{throw new AnalysisDslError(code,message,[{code,severity:'error',message}]);}

interface PortRef { id:string; owner:string; name:string }
function portRef(descriptors:readonly unknown[],port:T.Port):PortRef{
 const d=descriptors.find(x=>object(x)&&x.id===port.id);
 const input=object(d)?d.input:undefined;
 if(!object(d)||d.kind!=='port'||!object(input)||typeof input.owner!=='string'||typeof input.name!=='string')refuse('EMI_PORT_UNKNOWN','EMI port must be a declared port of a standalone network');
 return{id:String(d.id),owner:input.owner as string,name:input.name as string};
}

/** Uniform resampling by linear interpolation: samples at t0 + n*dt, n = 0..N-1, all inside the solved axis. */
function resample(times:readonly number[],values:readonly (number|null)[],t0:number,dtS:number,count:number):number[]{
 for(let i=1;i<times.length;i++)if(!(times[i]!>times[i-1]!))refuse('EMI_WAVEFORM_UNAVAILABLE','Transient time axis is not strictly increasing');
 const first=times[0]!,last=times.at(-1)!,eps=dtS*1e-9;
 if(t0<first-eps||t0+(count-1)*dtS>last+eps)refuse('EMI_WAVEFORM_UNAVAILABLE','Transient axis does not cover the discarded-and-windowed observation');
 const out:number[]=[];let j=0;
 for(let n=0;n<count;n++){
  const t=t0+n*dtS;
  while(j<times.length-2&&times[j+1]!<t)j++;
  const a=times[j]!,b=times[j+1]!,va=values[j],vb=values[j+1];
  if(typeof va!=='number'||typeof vb!=='number')refuse('EMI_WAVEFORM_UNAVAILABLE','Measurement waveform has unavailable samples inside the spectrum window');
  const frac=b>a?Math.min(1,Math.max(0,(t-a)/(b-a))):0;
  out.push(va+(vb-va)*frac);
 }
 return out;
}

/** Single-sided amplitude spectrum on bins f_k = k/(N dt), restricted to [startHz, stopHz].
 * Window is periodic Hann (sum w = N/2) or rectangular (sum w = N). Amplitude correction 2/sum(w),
 * the DC-free Nyquist bin uses 1/sum(w). Direct DFT with a cost guard; no zero-padding. */
function amplitudeSpectrum(values:readonly number[],dtS:number,window:'hann'|'rectangular',startHz:number,stopHz:number){
 const N=values.length;
 const w=Array.from({length:N},(_,n)=>window==='hann'?0.5*(1-Math.cos(2*Math.PI*n/N)):1);
 const sumW=w.reduce((a,b)=>a+b,0);
 const df=1/(N*dtS),nyquistBin=N%2===0?N/2:-1;
 const kMin=Math.max(1,Math.ceil(startHz/df-1e-9)),kMax=Math.min(Math.floor(stopHz/df+1e-9),Math.floor(N/2));
 const bins=Math.max(0,kMax-kMin+1);
 if(bins*N>MAX_SPECTRUM_OPERATIONS)refuse('EMI_SPECTRUM_TOO_LARGE',`Spectrum needs ${bins} bins x ${N} samples; reduce the range or increase sampleStepS`);
 const cos=new Float64Array(N),sin=new Float64Array(N);
 for(let i=0;i<N;i++){cos[i]=Math.cos(2*Math.PI*i/N);sin[i]=Math.sin(2*Math.PI*i/N);}
 const x=values.map((v,n)=>v*w[n]!);
 const frequencyHz:number[]=[],amplitudeV:number[]=[],weight:number[]=[];
 for(let k=kMin;k<=kMax;k++){
  let re=0,im=0;
  for(let n=0;n<N;n++){const i=(k*n)%N;re+=x[n]!*cos[i]!;im-=x[n]!*sin[i]!;}
  const isNyquist=k===nyquistBin;
  frequencyHz.push(k*df);
  amplitudeV.push((isNyquist?1:2)*Math.hypot(re,im)/sumW);
  weight.push(isNyquist?1:0.5);
 }
 /** Equivalent noise bandwidth in bins: N sum(w^2)/(sum w)^2 (Hann 1.5, rectangular 1). */
 const enbw=N*w.reduce((a,b)=>a+b*b,0)/(sumW*sumW);
 return{frequencyHz,amplitudeV,weight,enbw};
}

/** Detector over bands of width bandwidthHz starting at startHz; absent bandwidth means one band over the whole range.
 * peak = max single-sided line amplitude. rms = sqrt(sum A^2/2 / ENBW) over the band's lines (Nyquist line weight 1):
 * the ENBW division removes the window's energy spreading so one on-bin tone reads A/sqrt(2). Not a CISPR receiver. */
function detect(sp:{frequencyHz:number[];amplitudeV:number[];weight:number[];enbw:number},detector:'rms'|'peak',startHz:number,stopHz:number,bandwidthHz?:number){
 const width=bandwidthHz??stopHz-startHz;
 const count=bandwidthHz===undefined?1:Math.max(1,Math.ceil((stopHz-startHz)/bandwidthHz-1e-12));
 const hits=new Array<number>(count).fill(0),peak=new Array<number>(count).fill(0),power=new Array<number>(count).fill(0);
 sp.frequencyHz.forEach((f,i)=>{
  const b=Math.min(count-1,Math.max(0,Math.floor((f-startHz)/width+1e-9)));
  const a=sp.amplitudeV[i]!;
  hits[b]!++;peak[b]=Math.max(peak[b]!,a);power[b]=power[b]!+sp.weight[i]!*a*a;
 });
 const values:(number|null)[]=[],centersHz:number[]=[];
 for(let b=0;b<count;b++){
  const lo=startHz+b*width,hi=Math.min(lo+width,stopHz);
  centersHz.push((lo+hi)/2);
  values.push(hits[b]===0?null:detector==='peak'?peak[b]!:Math.sqrt(power[b]!/sp.enbw));
 }
 return{values,centersHz,empty:hits.filter(h=>h===0).length,singleBand:bandwidthHz===undefined};
}

/** P12 v1 conducted/coupling EMI for a standalone network only. Excitations and measurements are
 * rewritten into an si-style transient task and run through the existing transient executor; this module
 * only resamples, windows and detects. Coupling uses the same network path: no separate coupling physics. */
export function createEmiExecutor(transient=createTransientExecutor()){
 return async(task:AnalysisTask<'emi'>):Promise<ExecutionResult>=>{
  const c=task.config;
  if(task.board||c.kind==='risk')refuse('EMI_BOARD_NOT_IMPLEMENTED','EMI risk/board analysis is not implemented in v1: it needs P7 HF extraction, and P7 only covers 1-10 kHz');
  if(c.kind!=='conducted'&&c.kind!=='coupling')refuse('EMI_KIND_REQUIRED','EMI requires kind conducted or coupling');
  if(task.variantChanges.length)refuse('EMI_VARIANT_NOT_IMPLEMENTED','EMI variant analysis is not implemented in v1');
  if(c.circuit)refuse('EMI_CIRCUIT_NOT_IMPLEMENTED','EMI on an explicit circuit is not implemented in v1; use a standalone network');
  if(c.sources||c.loads||c.components||c.temperatureC!==undefined||c.stackup||c.materials)refuse('EMI_OPTIONS_NOT_IMPLEMENTED','EMI standalone network accepts no sources, loads, components, stackup, materials or temperature');
  if(!c.time)refuse('EMI_FREQUENCY_DOMAIN_NOT_IMPLEMENTED','EMI frequency-domain excitation is not implemented in v1; supply time');
  const spectrum=c.spectrum;
  if(!spectrum)refuse('EMI_SPECTRUM_REQUIRED','EMI requires an explicit spectrum block');
  if(!(spectrum.discardS>=0)||spectrum.discardS>=c.time.stopS)refuse('EMI_SPECTRUM_RANGE_INVALID','spectrum.discardS must be in [0, time.stopS)');
  if(!(spectrum.startHz>0)||!(spectrum.stopHz>spectrum.startHz))refuse('EMI_SPECTRUM_RANGE_INVALID','spectrum range must satisfy 0 < startHz < stopHz');

  const excitations=(c.excitations??[]) as readonly T.Excitation[];
  if(excitations.length===0)refuse('EMI_EXCITATION_REQUIRED','EMI requires at least one excitation');
  const measurementList=c.measurement===undefined?[]:(Array.isArray(c.measurement)?c.measurement:[c.measurement]) as readonly T.Measurement[];
  if(measurementList.length===0)refuse('EMI_MEASUREMENT_REQUIRED','EMI requires at least one measurement');

  const drivers:Record<string,unknown>[]=[];
  for(const e of excitations){
   if(!('port' in e))refuse('EMI_COMPONENT_EXCITATION_NOT_IMPLEMENTED','Component-placement excitations are not implemented in v1; use a port excitation');
   if(e.currentA!==undefined)refuse('EMI_CURRENT_EXCITATION_NOT_IMPLEMENTED','Current excitations are not implemented: the transient path has only Thevenin voltage drivers');
   if(e.voltageSpectrum||e.currentSpectrum)refuse('EMI_SPECTRUM_EXCITATION_NOT_IMPLEMENTED','voltageSpectrum/currentSpectrum excitations are not supported on a standalone network in v1; the frequency path is circuit-only');
   if(e.voltageV===undefined)refuse('EMI_EXCITATION_REQUIRED','Excitation has no voltage waveform');
   const R=e.outputResistanceOhm;
   if(typeof R!=='number'||!(R>0)||!Number.isFinite(R))refuse('EMI_SOURCE_RESISTANCE_REQUIRED','Voltage excitations need an explicit positive outputResistanceOhm; ideal sources are not representable in v1');
   const voltageV:T.Waveform=typeof e.voltageV==='number'?{kind:'pwl',points:[[0,e.voltageV],[c.time.stopS,e.voltageV]],interpolation:'linear'}:e.voltageV;
   drivers.push({port:e.port,model:{kind:'linear-driver',lowV:DRIVER_LOW_V,highV:DRIVER_HIGH_V,outputResistanceOhm:R},stimulus:{kind:'waveform',voltageV}});
  }

  const measurements=measurementList.map((m,i)=>{
   if(m.quantity==='current')refuse('EMI_CURRENT_MEASUREMENT_NOT_IMPLEMENTED','Current measurement is not implemented: the transient path returns port voltages only');
   if(m.model!==undefined&&(m.model as {kind?:string}).kind!=='rc-input')refuse('EMI_MEASUREMENT_MODEL_NOT_IMPLEMENTED','Measurement load model must be an explicit rc-input');
   return{...m,ref:portRef(task.descriptors,m.port),index:i};
  });
  const owners=new Set([...excitations.map(e=>('port' in e?portRef(task.descriptors,e.port).owner:'')),...measurements.map(m=>m.ref.owner)]);
  if(owners.size!==1)refuse('EMI_MULTIPLE_NETWORKS','All excitation and measurement ports must belong to one standalone network');
  const owner=[...owners][0]!;
  const network=task.descriptors.find(d=>object(d)&&d.id===owner);
  if(!object(network)||!['network','connect'].includes(String(network.kind))||!object(network.input))refuse('EMI_TARGET_NOT_IMPLEMENTED','EMI v1 accepts only a standalone network or connect target');

  const ids=measurements.map(m=>m.id??m.ref.name);
  if(new Set(ids).size!==ids.length)refuse('EMI_DUPLICATE_MEASUREMENT','Measurement ids must be unique');
  const receivers=measurements.filter(m=>m.model!==undefined).map(m=>({port:m.port,model:m.model}));

  const siTask={...task,kind:'si',config:{network:{kind:'network',id:owner,contextId:task.contextId},drivers,receivers,time:c.time,numerics:c.numerics,budget:c.budget}} as unknown as AnalysisTask<'si'>;
  const run=await transient(siTask);
  const portSeries=(name:string)=>{
   const s=run.metrics[`network-voltage:${name}`] as unknown as Series|undefined;
   if(!s||s.kind!=='series'||!Array.isArray(s.values))refuse('EMI_RESULT_UNAVAILABLE','Transient executor returned no port voltage');
   return s;
  };
  const validityOf=(s:Series)=>s.validity.status==='conditional'?{status:'conditional' as const,reason:String(s.validity.reason??'Conditional transient solution')}:{status:'valid' as const};

  const diagnostics:T.Diagnostic[]=[...run.diagnostics];
  const metrics:Record<string,T.DataDescription>={};
  const spectra:Record<string,{frequencyHz:number[];amplitudeV:number[]}>={};
  let conditional=false;

  for(const d of task.descriptors){
   if(!object(d)||d.kind!=='port'||!object(d.input)||d.input.owner!==owner)continue;
   const s=run.metrics[`network-voltage:${String(d.input.name)}`] as unknown as Series|undefined;
   if(!s||s.kind!=='series')continue;
   metrics[`voltage:${String(d.id)}`]={kind:'series',unit:'V',axis:s.axis,values:s.values,validity:validityOf(s),diagnostics:[]} as unknown as T.DataDescription;
  }

  const dt=c.time.sampleStepS;
  for(const m of measurements){
   const s=portSeries(m.ref.name);
   if(s.validity.status==='conditional')conditional=true;
   const t0=spectrum.discardS;
   const times=s.axis.values;
   const count=Math.floor((times.at(-1)!-t0)/dt+1e-9)+1;
   if(count<2)refuse('EMI_WINDOW_TOO_SHORT','Observation after discardS has fewer than two samples');
   if(spectrum.stopHz>1/(2*dt)*(1+1e-12))refuse('EMI_SPECTRUM_ABOVE_NYQUIST',`spectrum.stopHz ${spectrum.stopHz} Hz exceeds Nyquist ${1/(2*dt)} Hz`);
   if(spectrum.startHz<1/(count*dt)*(1-1e-12))refuse('EMI_SPECTRUM_BELOW_RESOLUTION',`spectrum.startHz ${spectrum.startHz} Hz is below 1/T = ${1/(count*dt)} Hz`);
   const uniform=resample(times,s.values,t0,dt,count);
   const sp=amplitudeSpectrum(uniform,dt,spectrum.window,spectrum.startHz,spectrum.stopHz);
   spectra[m.id??m.ref.name]={frequencyHz:sp.frequencyHz,amplitudeV:sp.amplitudeV};
   const detected=detect(sp,m.detector??'peak',spectrum.startHz,spectrum.stopHz,m.bandwidthHz);
   if(detected.empty)diagnostics.push({code:'EMI_BAND_EMPTY',severity:'warning',message:`${detected.empty} detector band(s) contain no spectral bins and are unavailable`});
   const validity=validityOf(s);
   const key=`measurement:${JSON.stringify(m.id??m.ref.name)}`;
   if(detected.singleBand){
    const v=detected.values[0];
    metrics[key]=typeof v==='number'
     ?{kind:'scalar',unit:'V',value:v,validity,diagnostics:[]} as unknown as T.DataDescription
     :{kind:'scalar',unit:'V',validity:{status:'unavailable',reason:'Spectrum window has no bins in the requested range'},diagnostics:[]} as unknown as T.DataDescription;
   }else metrics[key]={kind:'series',unit:'V',axis:{name:'frequencyHz',unit:'Hz',values:detected.centersHz},values:detected.values,validity,diagnostics:[]} as unknown as T.DataDescription;
  }
  metrics.spectra={kind:'table',unit:'V',validity:{status:conditional?'conditional':'valid',...(conditional?{reason:'Conditional transient solution'}:{})},diagnostics:[],data:spectra} as unknown as T.DataDescription;

  return {
   id:`${task.contextId}:emi`,status:'complete',method:'Standalone network conducted/coupling EMI: existing SI transient path, uniform resampling, windowed single-sided amplitude spectrum, line detectors',
   diagnostics,
   assumptions:[
    'Each voltage excitation is a Thevenin linear-driver with the explicit outputResistanceOhm given on that excitation; ideal (zero-impedance) sources are refused.',
    'Measurements are port node voltages from the existing transient path; a measurement model (rc-input) loads the port, otherwise the port is unloaded.',
    'Transient output on its adaptive axis is resampled to a uniform grid of sampleStepS starting at spectrum.discardS by linear interpolation; this is an approximation of the solver output, not an exact transform.',
    'Window: periodic Hann w=0.5(1-cos(2 pi n/N)) or rectangular. Amplitude correction 2/sum(w) makes a bin-centred tone read its true amplitude; off-bin tones read low by window scalloping (Hann up to about 1.4 dB). No zero-padding.',
    'Spectrum bins are k/(N dt); tones off the bin grid leak to neighbouring bins.',
    'Detector peak = max line amplitude in band; rms = sqrt(sum A^2/2 / ENBW) over lines in band, ENBW = 1.5 bins for Hann (1 for rectangular), so a single on-bin tone reads A/sqrt(2). Default detector when omitted is peak. Band edges start at spectrum.startHz with step bandwidthHz; without bandwidthHz one band covers the whole range.',
    'NOT a CISPR quasi-peak or average receiver: no IF bandwidth, charge/discharge weighting, CISPR band rules or meter time constants.',
    'Coupling kind uses the same network transient path; the network must author any coupling element itself.',
    'Risk/board/channel/differential targets, current excitations, frequency-domain (voltageSpectrum/currentSpectrum) excitations and current measurements are refused.',
   ],
   metrics,
  };
 };
}
