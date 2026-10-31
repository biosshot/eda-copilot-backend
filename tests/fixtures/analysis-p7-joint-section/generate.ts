import {writeFileSync} from 'node:fs';
import {segmentedCoupon,segmentedCouponDsl} from '../../helpers/pcb-analysis-segmented-coupon.ts';
const rect=(a:number,b:number,c:number,d:number)=>[{x:a,y:b},{x:c,y:b},{x:c,y:d},{x:a,y:d}];
for(const slot of [false,true]){
 const b=segmentedCoupon();for(const p of b.outline)if(p.x>1)p.x-=8;for(const c of b.components)if(c.at.x>1)c.at.x-=8;for(const p of b.pads)if(p.at.x>1)p.at.x-=8;
 b.copper.fixed.tracks=[{...b.copper.fixed.tracks[0],points:[{x:0,y:0},{x:2,y:0}]},{...b.copper.fixed.tracks[0],id:'victim',net:'VICTIM',points:[{x:0,y:.18},{x:2,y:.18}]}];b.nets.push({name:'VICTIM'});
 for(const [i,x]of [[0,0],[1,2]])b.pads.push({...structuredClone(b.pads[i]),id:'victim-'+i,number:'3',net:'VICTIM',at:{x,y:.18}});
 const polygon={outer:rect(-.1,-.25,2.1,.25),holes:slot?[rect(.9,-.08,1.1,.23)]:[]};
 b.copper.fixed.zones=[{id:'physical-return',kind:'pour',net:'RETURN',layers:['BOTTOM'],outline:polygon,priority:1,preserveIslands:true,refillRules:{status:'unknown',unresolved:['Explicit benchmark physical polygon; no CAD refill']},filledPolygons:[{layer:'BOTTOM',source:'cad',status:'available',freshness:'unknown',polygons:[polygon]}]}];
 writeFileSync('tests/fixtures/analysis-p7-joint-section/'+(slot?'slot':'continuous')+'.json',JSON.stringify(b,null,2));
}
writeFileSync('tests/fixtures/analysis-p7-joint-section/channel.js',segmentedCouponDsl);

