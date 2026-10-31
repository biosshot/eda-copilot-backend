/** D1 postprocessing only: no field extraction or network solver. */
export function d1Observables(r:any){
 if(r.status!=='complete'||r.ports?.length!==4||!r.parameters?.length)throw Error('D1_MISSING_MULTIPORT');
 const D=[[1,0],[0,1],[-1,0],[0,-1]];
 const electricEnergyJ=r.loadedEnergyGram?.map((e:any)=>Array.from({length:2},(_,i)=>Array.from({length:2},(_,j)=>D.reduce((sum,row,a)=>sum+D.reduce((v,col,b)=>v+row[i]*col[j]*e.electricJPerA2[a][b],0),0))));
 const magneticEnergyJ=r.loadedEnergyGram?.map((e:any)=>Array.from({length:2},(_,i)=>Array.from({length:2},(_,j)=>D.reduce((sum,row,a)=>sum+D.reduce((v,col,b)=>v+row[i]*col[j]*e.magneticJPerA2[a][b],0),0))));
 return {electricEnergyJ,magneticEnergyJ,ports:r.ports,frequenciesHz:r.frequenciesHz,S:r.parameters,loadedLoopReactiveH:r.parameters.map((s:any,f:number)=>Array.from({length:2},(_,i)=>Array.from({length:2},(_,j)=>25*D.reduce((sum,row,a)=>sum+D.reduce((v,col,b)=>v+row[i]*col[j]*s[a][b].imaginary,0),0)/(2*Math.PI*r.frequenciesHz[f])))),convention:'D^T Z_loaded D; Z_loaded=25(I+S) ohm, four physical50ohm shunts; reactive/omega includes electric effects, not assumed pure L'};
}
export function compareD1(actual:any,reference:any,tolerance=.08){
 if(JSON.stringify(actual.ports)!==JSON.stringify(reference.ports)||JSON.stringify(actual.frequenciesHz)!==JSON.stringify(reference.frequenciesHz))throw Error('D1_PHYSICAL_PORT_AXIS');
 let wave=0,magnetic=0,energy=0;for(let f=0;f<reference.S.length;f++)for(let i=0;i<4;i++)for(let j=0;j<4;j++){const a=actual.S[f][i][j],b=reference.S[f][i][j];if(![a.real,a.imaginary,b.real,b.imaginary].every(Number.isFinite))throw Error('D1_NONFINITE');wave=Math.max(wave,Math.hypot(a.real-b.real,a.imaginary-b.imaginary)/Math.max(Math.hypot(b.real,b.imaginary),1e-4));}
 for(let f=0;f<reference.S.length;f++)for(let i=0;i<2;i++)for(let j=0;j<2;j++){const a=actual.loadedLoopReactiveH[f][i][j],b=reference.loadedLoopReactiveH[f][i][j];if(!Number.isFinite(a)||!Number.isFinite(b))throw Error('D1_NONFINITE');magnetic=Math.max(magnetic,Math.abs(a-b)/Math.max(Math.abs(b),1e-11));}
 if(actual.magneticEnergyJ&&reference.magneticEnergyJ)for(let f=0;f<reference.S.length;f++)for(let i=0;i<2;i++)for(let j=0;j<2;j++){const a=actual.magneticEnergyJ[f][i][j],b=reference.magneticEnergyJ[f][i][j];if(!Number.isFinite(a)||!Number.isFinite(b))throw Error('D1_NONFINITE');energy=Math.max(energy,Math.abs(a-b)/Math.max(Math.abs(b),5e-12));}
 return {wave,magnetic,energy,maxRelativeChange:Math.max(wave,magnetic,energy),status:Math.max(wave,magnetic,energy)<=tolerance?'pass':'fail',tolerance,absoluteFloors:{S:1e-4,loadedLoopReactiveH:1e-11,magneticEnergyJ:5e-12}};
}
