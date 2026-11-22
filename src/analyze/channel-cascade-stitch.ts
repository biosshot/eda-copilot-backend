/** Complex S-parameter algebra for the product channel cascade (P7-completion 4.4).
 * Every operation acts on one frequency point, so frequency-dependent (dispersive)
 * section losses need no causal model here: each section is sampled and stitched
 * at its own exact frequency. Port reference impedance is the same real value on every port. */
export interface Cx {re:number;im:number}
export type CMatrix=Cx[][];
export const cx=(re=0,im=0):Cx=>({re,im});
export const cadd=(a:Cx,b:Cx):Cx=>cx(a.re+b.re,a.im+b.im);
export const csub=(a:Cx,b:Cx):Cx=>cx(a.re-b.re,a.im-b.im);
export const cmul=(a:Cx,b:Cx):Cx=>cx(a.re*b.re-a.im*b.im,a.re*b.im+a.im*b.re);
export const cdiv=(a:Cx,b:Cx):Cx=>{const d=b.re*b.re+b.im*b.im;return cx((a.re*b.re+a.im*b.im)/d,(a.im*b.re-a.re*b.im)/d);};
export const cabs=(a:Cx)=>Math.hypot(a.re,a.im);
const cneg=(a:Cx):Cx=>cx(-a.re,-a.im);
const mmul=(A:CMatrix,B:CMatrix):CMatrix=>A.map((_,i)=>B[0].map((_x,j)=>A[i].reduce((s,_y,k)=>cadd(s,cmul(A[i][k],B[k][j])),cx())));
const msub=(A:CMatrix,B:CMatrix):CMatrix=>A.map((r,i)=>r.map((_,j)=>csub(A[i][j],B[i][j])));
const madd=(A:CMatrix,B:CMatrix):CMatrix=>A.map((r,i)=>r.map((_,j)=>cadd(A[i][j],B[i][j])));
const eye=(n:number):CMatrix=>Array.from({length:n},(_,i)=>Array.from({length:n},(_x,j)=>cx(i===j?1:0)));
export function minv(A:CMatrix):CMatrix{
 const n=A.length,a=A.map((r,i)=>[...r.map(x=>({...x})),...eye(n)[i]]);
 for(let c=0;c<n;c++){
  let p=c;for(let r=c+1;r<n;r++)if(cabs(a[r][c])>cabs(a[p][c]))p=r;
  [a[c],a[p]]=[a[p],a[c]];const d=a[c][c];
  if(cabs(d)===0)throw Error('CHANNEL_CASCADE: singular termination matrix');
  for(let j=0;j<2*n;j++)a[c][j]=cdiv(a[c][j],d);
  for(let r=0;r<n;r++)if(r!==c){const f=a[r][c];for(let j=0;j<2*n;j++)a[r][j]=csub(a[r][j],cmul(f,a[c][j]));}
 }
 return a.map(r=>r.slice(n));
}
const pick=(S:CMatrix,rows:number[],cols:number[]):CMatrix=>rows.map(i=>cols.map(j=>S[i][j]));
/** Terminates ports `term` with reflection coefficient `gamma` and keeps ports `keep` (in the given order). */
export function terminatePorts(S:CMatrix,keep:number[],term:number[],gamma:Cx):CMatrix{
 if(!term.length)return pick(S,keep,keep);
 const G:CMatrix=term.map((_,i)=>term.map((_x,j)=>i===j?gamma:cx()));
 const inner=minv(msub(eye(term.length),mmul(pick(S,term,term),G)));
 return madd(pick(S,keep,keep),mmul(mmul(mmul(pick(S,keep,term),G),inner),pick(S,term,keep)));
}
type Two=CMatrix;
const toT=(S:Two):CMatrix=>{const one=cx(1);return [
 [cdiv(csub(cmul(S[0][1],S[1][0]),cmul(S[0][0],S[1][1])),S[1][0]),cdiv(S[0][0],S[1][0])],
 [cdiv(cneg(S[1][1]),S[1][0]),cdiv(one,S[1][0])]];};
const fromT=(T:CMatrix):Two=>{const t22=T[1][1],one=cx(1);return [
 [cdiv(T[0][1],t22),csub(T[0][0],cdiv(cmul(T[0][1],T[1][0]),t22))],
 [cdiv(one,t22),cneg(cdiv(T[1][0],t22))]];};
/** Cascade of two-ports in the order given: port 2 of each joins port 1 of the next. Ideal joint, no discontinuity. */
export function cascadeTwoPorts(...sections:Two[]):Two{
 if(!sections.length)throw Error('CHANNEL_CASCADE: at least one section is required');
 return fromT(sections.map(toT).reduce((A,B)=>mmul(A,B)));
}
/** Largest singular value (power iteration on S^H S). Passive means <= 1. */
export function maxSingularValue(S:CMatrix):number{
 const n=S.length,SH:CMatrix=S[0].map((_,j)=>S.map(r=>cx(r[j].re,-r[j].im))),A=mmul(SH,S);
 let v:Cx[]=Array.from({length:n},(_,i)=>cx(1/Math.sqrt(n)+0.01*i)),lam=0;
 for(let it=0;it<2000;it++){
  const w=A.map(r=>r.reduce((s,x,j)=>cadd(s,cmul(x,v[j])),cx()));
  const norm=Math.sqrt(w.reduce((s,x)=>s+x.re*x.re+x.im*x.im,0));
  v=w.map(x=>cx(x.re/norm,x.im/norm));
  if(Math.abs(norm-lam)<1e-16*norm){lam=norm;break;}
  lam=norm;
 }
 return Math.sqrt(lam);
}
/** Largest |S_ij - S_ji|. */
export function reciprocityError(S:CMatrix):number{
 let m=0;for(let i=0;i<S.length;i++)for(let j=i+1;j<S.length;j++)m=Math.max(m,cabs(csub(S[i][j],S[j][i])));return m;
}
/** Phase delay of a transmission coefficient at frequency f, seconds. */
export const phaseDelayS=(s21:Cx,fHz:number)=>-Math.atan2(s21.im,s21.re)/(2*Math.PI*fHz);
