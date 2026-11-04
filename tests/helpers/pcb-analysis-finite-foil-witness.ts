import {segmentedCoupon} from './pcb-analysis-segmented-coupon.ts';

/** Geometry of native/analyze/tests/board_foil.rs::fixture. The optional
 * authored bend stays strictly inside TOP copper and changes source provenance
 * only; its two directions allow automatic lateral-return selection. */
export function finiteFoilWitnessControl(withWitness=true){
 const board=segmentedCoupon();
 const points=(xy:number[][])=>xy.map(([x,y])=>({x,y}));
 board.outline=points([[-1.25,-1.25],[1.25,-1.25],[1.25,1.25],[-1.25,1.25]]);
 const outer=points([[-1,-1],[1,-1],[1,1],[-1,1]]);
 const hole=points([[-.25,-.25],[0,.25],[.25,-.25]]);
 const zone=(id:string,layer:string,net:string)=>{
  const polygon={outer,holes:[hole]};
  return {id,kind:'pour',net,layers:[layer],outline:polygon,priority:1,preserveIslands:true,
   refillRules:{status:'unknown',unresolved:['Authored finite foil control']},
   filledPolygons:[{layer,source:'cad',status:'available',freshness:'unknown',polygons:[polygon]}]};
 };
 board.copper.fixed.zones=[zone('signal-foil','TOP','SIGNAL'),zone('return-foil','BOTTOM','RETURN')];
 board.copper.fixed.tracks=withWitness?[{id:'contained-signal-bend',net:'SIGNAL',layer:'TOP',widthMm:.1,
  points:points([[-.75,-.25],[-.75,.75],[.5,.75]])}]:[];
 const [left,right]=points([[-.75,-.25],[.5,.75]]);
 for(const [i,pad] of board.pads.entries()){
  pad.at=i%2===0?left:right;
  pad.shape={kind:'rect',widthMm:.25,heightMm:.25};
 }
 return board;
}
