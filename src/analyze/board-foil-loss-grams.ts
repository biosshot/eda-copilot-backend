import type {AnalysisBoardFoilCheckedReport,AnalysisBoardFoilLossGrams} from '../../native/analyze/index.js';
type Complex={real:number;imaginary:number};
type Matrix=Complex[][];
function fail(why:string):never{throw Error('CHANNEL_FOIL_EVIDENCE: '+why);}
const close=(a:number,b:number)=>Number.isFinite(a)&&Number.isFinite(b)&&Math.abs(a-b)<=1e-12+1e-8*(Math.abs(a)+Math.abs(b));

function positiveGram(g:Matrix,n:number){
 if(!Array.isArray(g)||g.length!==n||g.some(row=>!Array.isArray(row)||row.length!==n||row.some(z=>!z||!Number.isFinite(z.real)||!Number.isFinite(z.imaginary))))fail('invalid loss Gram dimensions or values');
 for(let i=0;i<n;i++)for(let j=0;j<n;j++)if(!close(g[i][j].real,g[j][i].real)||!close(g[i][j].imaginary,-g[j][i].imaginary))fail('loss Gram must be Hermitian');
 // Realification has the same PSD condition as the complex Hermitian Gram.
 const a=Array.from({length:2*n},(_,i)=>Array.from({length:2*n},(_,j)=>i<n?(j<n?g[i][j].real:-g[i][j-n].imaginary):(j<n?g[i-n][j].imaginary:g[i-n][j-n].real)));
 const scale=Math.max(0,...g.map((row,i)=>Math.abs(row[i].real)));
 const floor=256*Number.EPSILON*n*scale+Number.MIN_VALUE*(2*n)**3;
 for(let k=0;k<2*n;k++){
  let p=k;for(let i=k+1;i<2*n;i++)if(a[i][i]>a[p][p])p=i;
  [a[k],a[p]]=[a[p],a[k]];for(const row of a)[row[k],row[p]]=[row[p],row[k]];
  const pivot=a[k][k];if(pivot< -floor)fail('loss Gram is not positive semidefinite');
  if(pivot<=floor){for(let i=k;i<2*n;i++)for(let j=k;j<2*n;j++)if(Math.abs(a[i][j])>floor)fail('loss Gram has an inconsistent zero pivot');return;}
  for(let i=k+1;i<2*n;i++)for(let j=i;j<2*n;j++){const value=a[i][j]-a[i][k]*a[j][k]/pivot;if(!Number.isFinite(value))fail('loss Gram factor overflow');a[i][j]=value;a[j][i]=value;}
 }
}

export function validateBoardFoilLossGrams(report:AnalysisBoardFoilCheckedReport,owners:readonly {id:string;resistanceOhm:number}[]){
 const refs=report.referenceOhm,n=refs.length;
 if(n<1||n>16)fail('loss Gram port count exceeds the physical port bound');
 for(const [f,sample] of report.samples.entries()){
  const grams=sample.lossGrams;if(grams===undefined)continue;if(!grams)fail('malformed loss Gram evidence');
  positiveGram(grams.copperWPerA2,n);
  if(!Array.isArray(grams.packages)||grams.packages.length!==owners.length||grams.packages.some(p=>!p||typeof p.id!=='string'||!p.id.trim())||new Set(grams.packages.map(p=>p.id)).size!==owners.length)fail('missing or duplicate loss Gram owner');
  for(const owner of owners){const g=grams.packages.find(p=>p.id===owner.id);if(!g)fail('missing package loss Gram');positiveGram(g.powerWPerA2,n);}
  for(let i=0;i<n;i++)for(let j=0;j<n;j++){
   let real=grams.copperWPerA2[i][j].real,imaginary=grams.copperWPerA2[i][j].imaginary;
   if(i===j&&!close(real,sample.columns[i].copperLossW))fail('copper Gram diagonal mismatch');
   for(const owner of owners){
    const g=grams.packages.find(p=>p.id===owner.id)!.powerWPerA2[i][j];
    const ci=sample.columns[i].packages?.find(p=>p.id===owner.id),cj=sample.columns[j].packages?.find(p=>p.id===owner.id);
    if(!ci||!cj)fail('missing package current for loss Gram');
    const expectedReal=owner.resistanceOhm*(ci.currentA.real*cj.currentA.real+ci.currentA.imaginary*cj.currentA.imaginary);
    const expectedImaginary=owner.resistanceOhm*(ci.currentA.real*cj.currentA.imaginary-ci.currentA.imaginary*cj.currentA.real);
    if(!close(g.real,expectedReal)||!close(g.imaginary,expectedImaginary)||(i===j&&!close(g.real,ci.lossW)))fail('package loss Gram does not match physical current');
    real+=g.real;imaginary+=g.imaginary;
   }
   let expectedReal=i===j?1:0,expectedImaginary=0;
   for(const row of report.parameters[f]){const a=row[i],b=row[j];expectedReal-=a.real*b.real+a.imaginary*b.imaginary;expectedImaginary-=a.real*b.imaginary-a.imaginary*b.real;}
   const scale=(Math.sqrt(refs[i])/2)*(Math.sqrt(refs[j])/2);
   if(!close(real,scale*expectedReal)||!close(imaginary,scale*expectedImaginary))fail('loss Gram does not match complex wave power');
  }
 }
}

export function projectBoardFoilLossGrams(grams:AnalysisBoardFoilLossGrams,drive:readonly Complex[]){
 if(!drive.length||drive.length>16||!Array.isArray(grams.packages)||grams.packages.some(p=>!p||typeof p.id!=='string'||!p.id.trim())||new Set(grams.packages.map(p=>p.id)).size!==grams.packages.length)fail('invalid loss projection dimensions or owners');
 const project=(g:Matrix)=>{
  positiveGram(g,drive.length);let power=0,imaginary=0,norm=0;
  for(const d of drive){if(!Number.isFinite(d.real)||!Number.isFinite(d.imaginary))fail('nonfinite loss drive');norm+=d.real*d.real+d.imaginary*d.imaginary;}
  if(!Number.isFinite(norm))fail('loss drive norm overflow');
  for(let i=0;i<drive.length;i++)for(let j=0;j<drive.length;j++){
   const a=drive[i],b=drive[j],v=g[i][j];const re=v.real*b.real-v.imaginary*b.imaginary,im=v.real*b.imaginary+v.imaginary*b.real;
   power+=a.real*re+a.imaginary*im;imaginary+=a.real*im-a.imaginary*re;
  }
  const scale=Math.max(0,...g.map((row,i)=>Math.abs(row[i].real)))*norm;
  const floor=256*Number.EPSILON*drive.length*drive.length*scale+Number.MIN_VALUE*16*drive.length*drive.length;
  if(!Number.isFinite(power)||!Number.isFinite(imaginary)||power< -floor||Math.abs(imaginary)>floor)fail('invalid projected loss power');
  return Math.max(0,power);
 };
 const copperLossW=project(grams.copperWPerA2),packages=grams.packages.map(p=>({id:p.id,lossW:project(p.powerWPerA2)}));
 const totalLossW=copperLossW+packages.reduce((sum,p)=>sum+p.lossW,0);
 if(!Number.isFinite(totalLossW))fail('total loss projection overflow');
 return {copperLossW,packages,totalLossW};
}
