import {segmentedCouponDsl,segmentedCouponVariant} from './pcb-analysis-segmented-coupon.ts';

export function finitePlaneBend(){
 const board=segmentedCouponVariant('bend');
 const scale=(p:{x:number;y:number})=>{p.x*=.3;p.y*=.3;};
 for(const track of board.copper.fixed.tracks)for(const point of track.points)scale(point);
 for(const pad of board.pads){scale(pad.at);pad.shape={kind:'rect',widthMm:.18,heightMm:.14};}
 for(const component of board.components)scale(component.at);
 const rectangle=(x0:number,y0:number,x1:number,y1:number)=>[
  {x:x0,y:y0},{x:x1,y:y0},{x:x1,y:y1},{x:x0,y:y1},
 ];
 board.outline=rectangle(-.4,-.55,1.95,1.95);
 const copper={outer:rectangle(-.22,-.35,1.75,1.75),holes:[]};
 board.copper.fixed.tracks.splice(1,1);
 board.copper.fixed.zones=[{id:'finite-return-pour',kind:'pour',net:'RETURN',layers:['BOTTOM'],outline:copper,
  priority:1,preserveIslands:true,refillRules:{status:'unknown',unresolved:['Captured finite coupon']},
  filledPolygons:[{layer:'BOTTOM',source:'cad',status:'available',freshness:'unknown',polygons:[copper]}]}];
 return board;
}

export const finitePlaneBendDsl=segmentedCouponDsl.split('a.plot(')[0]
  .replace("startHz:1000,stopHz:10000,points:3,scale:'log'","startHz:1000,stopHz:3000,points:2,scale:'linear'")
  .replace('boardThicknessMm:.12','boardThicknessMm:.11')
  .replaceAll('thicknessMm:.01','thicknessMm:.005');
