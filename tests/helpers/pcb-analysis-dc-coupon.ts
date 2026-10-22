import {readFileSync} from 'node:fs';
import type {RoutingBoardV2} from '../../src/types/pcb/routing-board-v2.ts';
export function dcCoupon():RoutingBoardV2 {
  return JSON.parse(readFileSync('tests/fixtures/analysis-dc-coupon.json','utf8')) as RoutingBoardV2;
}
export const couponSetup=`const a=analysis({board});
const source=a.port({positive:pad('J1','1'),negative:pad('J1','2')});
const load=a.port({positive:pad('U1','1'),negative:pad('U1','2')});
const base=a.dc().stackup(stackup({layers:[{kind:'copper',name:'TOP',thicknessMm:.035,material:'cu'}]})).materials({cu:{conductivitySM:58e6,state:'constant isotropic coupon 20 C'}}).sources([{port:source,voltageV:1,model:'ideal'}]).mesh({maxAreaMm2:.125,minAngleDeg:0,maxRefinementSteps:64});`;
