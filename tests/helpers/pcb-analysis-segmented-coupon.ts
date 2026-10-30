import {readFileSync} from 'node:fs';
export function segmentedCoupon(){return JSON.parse(readFileSync('tests/fixtures/analysis-segmented-coupon.json','utf8'));}
export const segmentedCouponDsl=`const a=analysis({board});
const from=a.port({positive:pad('J0','1'),negative:pad('J0','2'),positiveLayer:'TOP',negativeLayer:'BOTTOM'});
const to=a.port({positive:pad('J1','1'),negative:pad('J1','2'),positiveLayer:'TOP',negativeLayer:'BOTTOM'});
const channel=a.channel({from,to});
const layers=stackup({boardThicknessMm:.12,viaPlatingThicknessUm:25,platingMaterial:'cu',diameterConvention:'finished_hole',layers:[{kind:'copper',name:'TOP',thicknessMm:.01,material:'cu'},{kind:'dielectric',name:'substrate',thicknessMm:.1,material:'d'},{kind:'copper',name:'BOTTOM',thicknessMm:.01,material:'cu'}]});
const run=()=>a.impedance().target(channel).method('layered').stackup(layers).materials({cu:{conductivitySM:58e6,state:'Explicit uniform copper coupon'},d:{relativePermittivity:4,state:'Explicit lossless coupon dielectric'}}).frequency({startHz:1000,stopHz:10000,points:3,scale:'log'}).budget({maxSeconds:300,maxMemoryMiB:2048}).run();const result=await run();
a.plot({title:'Actual automatic section/local composition',kind:'table',data:result.solution});`;
