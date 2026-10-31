import {readFileSync} from 'node:fs';
export function segmentedCoupon(){return JSON.parse(readFileSync('tests/fixtures/analysis-segmented-coupon.json','utf8'));}
export const segmentedCouponDsl=`const a=analysis({board});
const from=a.port({positive:pad('J0','1'),negative:pad('J0','2'),positiveLayer:'TOP',negativeLayer:'BOTTOM'});
const to=a.port({positive:pad('J1','1'),negative:pad('J1','2'),positiveLayer:'TOP',negativeLayer:'BOTTOM'});
const channel=a.channel({from,to});
const layers=stackup({boardThicknessMm:.12,viaPlatingThicknessUm:25,platingMaterial:'cu',diameterConvention:'finished_hole',layers:[{kind:'copper',name:'TOP',thicknessMm:.01,material:'cu'},{kind:'dielectric',name:'substrate',thicknessMm:.1,material:'d'},{kind:'copper',name:'BOTTOM',thicknessMm:.01,material:'cu'}]});
const run=()=>a.impedance().target(channel).method('layered').stackup(layers).materials({cu:{conductivitySM:58e6,state:'Explicit uniform copper coupon'},d:{relativePermittivity:4,state:'Explicit lossless coupon dielectric'}}).frequency({startHz:1000,stopHz:10000,points:3,scale:'log'}).budget({maxSeconds:300,maxMemoryMiB:2048}).run();const result=await run();
a.plot({title:'Actual automatic section/local composition',kind:'table',data:result.solution});`;

export function segmentedCouponVariant(variant='straight'){
 const board=segmentedCoupon();
 if(variant==='bend'){
  board.outline=[{x:-.3,y:-.6},{x:5.6,y:-.6},{x:5.6,y:5.3},{x:-.3,y:5.3}];
  for(const track of board.copper.fixed.tracks)track.points=[{x:0,y:0},{x:5,y:0},{x:5,y:5}];
  for(const p of board.pads.filter((p:any)=>p.component==='J1')){p.at={x:5,y:5};p.rotationDeg=90;}
  board.components[1].at={x:5,y:5};board.components[1].rotationDeg=90;
 }else if(variant==='coupled'){
  board.nets.push({name:'VICTIM'});board.copper.fixed.tracks.push({id:'victim',net:'VICTIM',layer:'TOP',widthMm:.1,points:[{x:0,y:.18},{x:10,y:.18}]});
 }else if(variant==='meander'){
  board.outline=[{x:-.3,y:-.6},{x:10.3,y:-.6},{x:10.3,y:.8},{x:-.3,y:.8}];
  for(const track of board.copper.fixed.tracks)track.points=[{x:0,y:0},{x:10,y:0},{x:10,y:.18},{x:0,y:.18}];
  for(const pad of board.pads.filter((p:any)=>p.component==='J1'))pad.at={x:0,y:.18};board.components[1].at={x:0,y:.18};
 }else if(variant==='branch'){
  board.outline=[{x:-.3,y:-.6},{x:10.3,y:-.6},{x:10.3,y:2.5},{x:-.3,y:2.5}];
  board.copper.fixed.tracks.push({id:'signal-stub',net:'SIGNAL',layer:'TOP',widthMm:.1,points:[{x:5,y:0},{x:5,y:2}]},{id:'return-stub',net:'RETURN',layer:'BOTTOM',widthMm:.5,points:[{x:5,y:0},{x:5,y:2}]});
 }else if(variant==='via'){
  board.outline=[{x:-.3,y:-.9},{x:10.3,y:-.9},{x:10.3,y:.6},{x:-.3,y:.6}];
  board.copper.fixed.tracks[0].points=[{x:0,y:0},{x:5,y:0}];
  board.copper.fixed.tracks.push({id:'signal-bottom',net:'SIGNAL',layer:'BOTTOM',widthMm:.1,points:[{x:5,y:0},{x:10,y:0}]});
  board.copper.fixed.tracks[1].widthMm=.1;board.copper.fixed.tracks[1].points=[{x:0,y:-.38},{x:10,y:-.38}];
  board.copper.fixed.vias.push({id:'signal-via',net:'SIGNAL',at:{x:5,y:0},diameterMm:.5,drillMm:.2,type:'through',fromLayer:'TOP',toLayer:'BOTTOM'});
  board.pads[1].layers=['BOTTOM'];for(const p of board.pads.filter((p:any)=>p.net==='RETURN'))p.at.y=-.38;
 }else if(variant==='rotated45'||variant==='rotated90'){
  const angle=variant==='rotated45'?45:90,theta=angle*Math.PI/180;
  const rotate=(p:any)=>{const x=p.x,y=p.y;p.x=x*Math.cos(theta)-y*Math.sin(theta);p.y=x*Math.sin(theta)+y*Math.cos(theta);};
  for(const p of board.outline)rotate(p);
  for(const p of board.pads){rotate(p.at);p.rotationDeg+=angle;}
  for(const c of board.components){rotate(c.at);c.rotationDeg+=angle;}
  for(const track of board.copper.fixed.tracks)for(const p of track.points)rotate(p);
 }else if(variant!=='straight')throw new Error('Unsupported coupon variant: '+variant);
 return board;
}

export function segmentedCouponDslFor(variant='straight'){
 if(variant==='meander')return segmentedCouponDsl.replace("negative:pad('J1','2')","negative:pad('J0','2')");
 if(variant!=='via')return segmentedCouponDsl;
 return segmentedCouponDsl.replace("positive:pad('J1','1'),negative:pad('J1','2'),positiveLayer:'TOP'","positive:pad('J1','1'),negative:pad('J1','2'),positiveLayer:'BOTTOM'").replace('boardThicknessMm:.12','boardThicknessMm:.22').replace("name:'substrate',thicknessMm:.1","name:'substrate',thicknessMm:.2");
}
