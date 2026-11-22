import type {PiAcTerm} from './pi-board-request.ts';

/** AC algebra of the board PI endpoint (contract sections 4, 5, 13). Pure functions, no physics beyond linear algebra.
 * The reduced board model enters as its open-port wave matrix S (reference Z0 on every port; well conditioned, unlike the
 * capacitive open-port Z). Sources, loads and decoupling legs are one-port terminations with reflection coefficient
 * G=(1-Z0 Y)/(1+Z0 Y) (short = -1 exactly, open = +1). Column j of the result has every other port terminated and the
 * port's own termination removed, exactly as the DC matrix of stage 2. */
export interface Cx{re:number;im:number}
export const PI_REFERENCE_OHM=50;
const add=(a:Cx,b:Cx):Cx=>({re:a.re+b.re,im:a.im+b.im});
const sub=(a:Cx,b:Cx):Cx=>({re:a.re-b.re,im:a.im-b.im});
const mul=(a:Cx,b:Cx):Cx=>({re:a.re*b.re-a.im*b.im,im:a.re*b.im+a.im*b.re});
const div=(a:Cx,b:Cx):Cx=>{const d=b.re*b.re+b.im*b.im;return {re:(a.re*b.re+a.im*b.im)/d,im:(a.im*b.re-a.re*b.im)/d};};
const abs=(a:Cx)=>Math.hypot(a.re,a.im);
const ZERO:Cx={re:0,im:0};

/** Dense complex Gaussian elimination with row-scaled partial pivoting. Returns null when singular. */
export function solveComplex(matrix:readonly (readonly Cx[])[],rhs:readonly (readonly Cx[])[]):Cx[][]|null{
 const n=matrix.length,k=rhs[0]?.length??0;
 const a=matrix.map((row,i)=>[...row,...rhs[i]]);
 const scale=a.map(row=>Math.max(...row.slice(0,n).map(abs)));
 if(scale.some(s=>!(s>0)||!Number.isFinite(s)))return null;
 for(let c=0;c<n;c++){
  let p=c;for(let r=c+1;r<n;r++)if(abs(a[r][c])/scale[r]>abs(a[p][c])/scale[p])p=r;
  if(!(abs(a[p][c])/scale[p]>1e-13))return null;
  [a[c],a[p]]=[a[p],a[c]];[scale[c],scale[p]]=[scale[p],scale[c]];
  for(let r=c+1;r<n;r++){
   const f=div(a[r][c],a[c][c]);
   if(f.re!==0||f.im!==0)for(let m=c;m<n+k;m++)a[r][m]=sub(a[r][m],mul(f,a[c][m]));
  }
 }
 const x=Array.from({length:n},()=>Array<Cx>(k).fill(ZERO));
 for(let r=n-1;r>=0;r--)for(let m=0;m<k;m++){
  let s=a[r][n+m];
  for(let q=r+1;q<n;q++)s=sub(s,mul(a[r][q],x[q][m]));
  x[r][m]=div(s,a[r][r]);
 }
 return x;
}

/** Complex admittance of one port termination at angular frequency w (`undefined` = open, `'short'` = zero impedance). */
export function termAdmittance(term:PiAcTerm,w:number):Cx|'short'{
 if(term.kind==='short')return 'short';
 if(term.kind==='shunt')return {re:term.g,im:w*term.c};
 const {r,l,c}=term;
 if(c===undefined){
  if(r===0&&(l===0||w===0))return 'short';
  return div({re:1,im:0},{re:r,im:w*l});
 }
 // Y = jwC/(1-w^2 L C+jwRC): finite at w=0 and for ideal parts.
 return div({re:0,im:w*c},{re:1-w*w*l*c,im:w*r*c});
}
/** Sum of the terminations of one port; any short makes the port a short. */
export function portAdmittance(terms:readonly PiAcTerm[],w:number):Cx|'short'|undefined{
 let y:Cx|undefined;
 for(const term of terms){
  const v=termAdmittance(term,w);
  if(v==='short')return 'short';
  y=y?add(y,v):v;
 }
 return y;
}
/** Reflection coefficient of a termination of admittance `y` against `PI_REFERENCE_OHM` (open when undefined). */
export function reflection(y:Cx|'short'|undefined):Cx{
 if(y==='short')return {re:-1,im:0};
 if(y===undefined)return {re:1,im:0};
 if(!Number.isFinite(y.re)||!Number.isFinite(y.im))return {re:-1,im:0};
 const zy:Cx={re:PI_REFERENCE_OHM*y.re,im:PI_REFERENCE_OHM*y.im};
 return div(sub({re:1,im:0},zy),add({re:1,im:0},zy));
}

/** `z[i][j]` (V at port i per 1 A at port j; other ports terminated by `gamma`, own termination removed) from the
 * board wave matrix `s` at one frequency. A null column means no finite response (lossless singular loop). */
export function terminatedZ(s:readonly (readonly Cx[])[],gamma:readonly Cx[]):(Cx|null)[][]{
 const n=s.length,z=Array.from({length:n},()=>Array<Cx|null>(n).fill(null));
 for(let j=0;j<n;j++){
  const t=[...Array(n).keys()].filter(p=>p!==j);
  // b_T = (I - S_TT G)^-1 S_Tj a ; a = 1 at port j.
  const m=t.map((p,r)=>t.map((q,c)=>sub(r===c?{re:1,im:0}:ZERO,mul(s[p][q],gamma[q]))));
  const b=t.length?solveComplex(m,t.map(p=>[s[p][j]])):[];
  if(!b)continue;
  let bj=s[j][j];
  t.forEach((q,r)=>{bj=add(bj,mul(mul(s[j][q],gamma[q]),b[r][0]));});
  // V = sqrt(Z0)(a+b), I = (a-b)/sqrt(Z0): z_ij = Z0 (a_i+b_i)/(a_j-b_j).
  const current=sub({re:1,im:0},bj);
  if(!(abs(current)>0))continue;
  z[j][j]=div({re:PI_REFERENCE_OHM*(1+bj.re),im:PI_REFERENCE_OHM*bj.im},current);
  t.forEach((q,r)=>{
   // a_q = G_q b_q, so a_q+b_q=(1+G_q) b_q.
   const v=mul(add({re:1,im:0},gamma[q]),b[r][0]);
   z[q][j]=div({re:PI_REFERENCE_OHM*v.re,im:PI_REFERENCE_OHM*v.im},current);
  });
 }
 return z;
}
/** Y = (1/Z0)(I-S)(I+S)^-1 of a wave matrix; null when I+S is singular (an ideal short). */
export function admittanceFromS(s:readonly (readonly Cx[])[]):Cx[][]|null{
 const n=s.length,one=(i:number,j:number):Cx=>({re:i===j?1:0,im:0});
 const plus=s.map((row,i)=>row.map((v,j)=>add(one(i,j),v)));
 const minus=s.map((row,i)=>row.map((v,j)=>sub(one(i,j),v)));
 // (I+S)^-1 and (I-S) commute, so X=(I+S)^-1(I-S)=(I-S)(I+S)^-1.
 const x=solveComplex(plus,minus);
 return x&&x.map(row=>row.map(v=>({re:v.re/PI_REFERENCE_OHM,im:v.im/PI_REFERENCE_OHM})));
}
/** Relative Frobenius difference of two matrices. */
export function relativeDifference(a:readonly (readonly Cx[])[],b:readonly (readonly Cx[])[]):number{
 let num=0,den=0;
 a.forEach((row,i)=>row.forEach((v,j)=>{num+=abs(sub(v,b[i][j]))**2;den+=abs(b[i][j])**2;}));
 return Math.sqrt(num/Math.max(den,Number.MIN_VALUE));
}

export interface Extremum{kind:'maximum'|'minimum';frequencyHz:number;magnitudeOhm:number;refinedFrequencyHz:number|null;refinedMagnitudeOhm:number|null}
/** Interior local extrema of |Z| over consecutive available points (null = unavailable). `maximum` is an anti-resonance, `minimum`
 * a series resonance. The refined value is the vertex of a parabola through three points in log-log scale (no new solve). */
export function magnitudeExtrema(frequencyHz:readonly number[],magnitude:readonly (number|null)[]):Extremum[]{
 const found:Extremum[]=[];
 for(let i=1;i+1<frequencyHz.length;i++){
  const [a,b,c]=[magnitude[i-1],magnitude[i],magnitude[i+1]];
  if(a===null||b===null||c===null||!(a>0&&b>0&&c>0))continue;
  const kind=b>a&&b>=c?'maximum':b<a&&b<=c?'minimum':undefined;
  if(!kind)continue;
  const x=[i-1,i,i+1].map(k=>Math.log(frequencyHz[k])),y=[a,b,c].map(Math.log);
  const s0=(y[1]-y[0])/(x[1]-x[0]),curvature=((y[2]-y[1])/(x[2]-x[1])-s0)/(x[2]-x[0]);
  const vertex=curvature===0?NaN:(x[0]+x[1]-s0/curvature)/2;
  const ok=Number.isFinite(vertex)&&vertex>x[0]&&vertex<x[2]&&(kind==='maximum'?curvature<0:curvature>0);
  found.push({kind,frequencyHz:frequencyHz[i],magnitudeOhm:b,
   refinedFrequencyHz:ok?Math.exp(vertex):null,
   refinedMagnitudeOhm:ok?Math.exp(y[0]+s0*(vertex-x[0])+curvature*(vertex-x[0])*(vertex-x[1])):null});
 }
 return found;
}
