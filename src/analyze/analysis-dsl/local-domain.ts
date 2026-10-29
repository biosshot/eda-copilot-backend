import {AnalysisDslError} from './schema.ts';
/** Declared normalized floors for small transfer responses and losses. These
 * certify only domain changes at the supplied verification points, never total
 * model accuracy. No phase error is evaluated at a response zero. */
export function domainChange(a:any,b:any){
 if(a.samples.length!==b.samples.length)throw new AnalysisDslError('LOCAL_DOMAIN','Domain verification axes disagree');
 let voltage=0,loss=0;
 for(let k=0;k<a.samples.length;k++){
  const x=a.samples[k],y=b.samples[k];
  if(JSON.stringify(x.drive)!==JSON.stringify(y.drive)||x.frequencyHz!==y.frequencyHz||x.portRealV.length!==y.portRealV.length)throw new AnalysisDslError('LOCAL_DOMAIN','Domain frequency/physical port mapping changed');
  const scale=Math.max(1e-12,...y.portRealV.map((v:number,i:number)=>Math.hypot(v,y.portImaginaryV[i])));
  for(let i=0;i<x.portRealV.length;i++)voltage=Math.max(voltage,Math.hypot(x.portRealV[i]-y.portRealV[i],x.portImaginaryV[i]-y.portImaginaryV[i])/Math.max(scale*1e-3,Math.hypot(y.portRealV[i],y.portImaginaryV[i])));
  loss=Math.max(loss,Math.abs(x.totalLossW-y.totalLossW)/Math.max(1e-18,Math.abs(y.totalLossW)));
 }
 return {voltageRelativeChange:voltage,totalLossRelativeChange:loss,maxRelativeChange:Math.max(voltage,loss),voltageAbsoluteFloorFractionOfLargestPort:1e-3,lossAbsoluteFloorW:1e-18};
}

/** Canonical power-wave validation under explicit mathematical reference loads.
 * Not a physical receiver assumption or a certificate of unloaded impedance.
 * Norton drive d, reference R: normalized outgoing wave (2V/R-d)/||d||.
 * A -40dB absolute amplitude floor is declared before measurement. */
export function referenceDomainChange(a:any,b:any,referenceOhm=50){
 if(!Number.isFinite(referenceOhm)||referenceOhm<=0)throw new AnalysisDslError('LOCAL_DOMAIN','Positive wave reference required');
 const transform=(r:any)=>({samples:r.samples.map((s:any)=>{const norm=Math.hypot(...s.drive);if(!norm)throw new AnalysisDslError('LOCAL_DOMAIN','Nonzero incident drive required');return {...s,portRealV:s.portRealV.map((v:number,i:number)=>(2*v/referenceOhm-s.drive[i])/norm),portImaginaryV:s.portImaginaryV.map((v:number)=>2*v/referenceOhm/norm)};})});
 const x=transform(a),y=transform(b);domainChange(x,y);let wave=0,loss=0;
 for(let k=0;k<x.samples.length;k++){const p=x.samples[k],q=y.samples[k];for(let i=0;i<p.portRealV.length;i++)wave=Math.max(wave,Math.hypot(p.portRealV[i]-q.portRealV[i],p.portImaginaryV[i]-q.portImaginaryV[i])/Math.max(.01,Math.hypot(q.portRealV[i],q.portImaginaryV[i])));loss=Math.max(loss,Math.abs(p.totalLossW-q.totalLossW)/Math.max(1e-18,Math.abs(q.totalLossW)));}
 return {waveRelativeChange:wave,totalLossRelativeChange:loss,maxRelativeChange:Math.max(wave,loss),waveAbsoluteAmplitudeFloor:.01,lossAbsoluteFloorW:1e-18,referenceOhm,observable:'canonical loaded power waves and intrinsic Cu loss; unloaded reactive impedance and regional loss unverified'};
}
