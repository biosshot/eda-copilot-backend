import {createHash} from 'node:crypto';
import {readFileSync} from 'node:fs';
export function verifyHash(path:string,expected:string){if(createHash('sha256').update(readFileSync(path)).digest('hex')!==expected)throw Error('HASH_MISMATCH '+path);}
export function compareValue(actual:number,reference:number,floor:number,tolerance=.08){if(!Number.isFinite(actual)||!Number.isFinite(reference)||floor<=0)throw Error('INVALID_VALUE');const error=Math.abs(actual-reference)/Math.max(Math.abs(reference),floor);return {actual,reference,error,status:error<=tolerance?'pass':'fail'};}
export function validateControl(v:any,axis:number[],ports:string[]){
 if(v.status!=='complete')throw Error('SOLVER_NOT_COMPLETE '+JSON.stringify(v.diagnostics));
 if(JSON.stringify(v.network?.ports)!==JSON.stringify(ports))throw Error('PORT_MISMATCH');
 if(JSON.stringify(v.frequenciesHz)!==JSON.stringify(axis))throw Error('BAND_MISMATCH');
 for(const key of ['capacitanceFPerM','inductanceHPerM']){const m=v.parameters[key];if(!Array.isArray(m)||m.length!==2||m.some((r:any)=>!Array.isArray(r)||r.length!==2||r.some((x:any)=>!Number.isFinite(x))))throw Error('MATRIX_INVALID');}
 if(!(v.parameters.capacitanceFPerM[0][1]<0&&v.parameters.inductanceHPerM[0][1]>0))throw Error('MUTUAL_MISSING');
}


/** Validate DSL transport metadata before comparing physical coefficients. */
export function validateDslCalculation(c:any){
 if(c.status!=='complete'||!c.solution)throw Error('MISSING_EVIDENCE');
 const units={capacitanceFPerM:'F/m',vacuumCapacitanceFPerM:'F/m',inductanceHPerM:'H/m',resistanceOhmPerM:'Ohm/m',conductanceSPerM:'S/m'};
 if(c.rlgc?.unit!=='1'||JSON.stringify(c.rlgc?.data?.units)!==JSON.stringify(units))throw Error('UNIT_MISMATCH');
 if(c.network?.kind!=='network'||typeof c.network.id!=='string')throw Error('SCHEMA_MISMATCH');
 const v=c.solution;
 validateControl(v,[1e6,1e7,1e8],['positiveInput','negativeInput','positiveOutput','negativeOutput']);
 if(v.network.referenceOhm!==50)throw Error('SCHEMA_MISMATCH');
 if(!Array.isArray(v.network.parameters)||v.network.parameters.length!==3||v.network.parameters.some((m:any)=>!Array.isArray(m)||m.length!==4||m.some((r:any)=>!Array.isArray(r)||r.length!==4||r.some((x:any)=>!x||!Number.isFinite(x.real)||!Number.isFinite(x.imaginary)))))throw Error('NETWORK_MATRIX_INVALID');
 for(const m of v.parameters.modes??[])for(const key of ['delaySPerM','impedanceOhm','capacitanceFPerM','inductanceHPerM'])if(!Number.isFinite(m[key]))throw Error('MODE_INVALID');
 if(v.parameters.modes?.length!==2)throw Error('MODE_INVALID');
}
