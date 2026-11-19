import test from 'node:test';
import assert from 'node:assert/strict';
import {cascadeTwoPorts,cabs,csub,cx,maxSingularValue,reciprocityError,terminatePorts,type CMatrix,type Cx} from '../src/analyze/channel-cascade-stitch.ts';
import {chooseChannelModel,describeChannelStructure} from '../src/analyze/channel-cascade-policy.ts';

const Z0=50;
const cexp=(a:Cx)=>cx(Math.exp(a.re)*Math.cos(a.im),Math.exp(a.re)*Math.sin(a.im));
const cosh=(g:Cx)=>{const p=cexp(g),m=cexp(cx(-g.re,-g.im));return cx((p.re+m.re)/2,(p.im+m.im)/2);};
const sinh=(g:Cx)=>{const p=cexp(g),m=cexp(cx(-g.re,-g.im));return cx((p.re-m.re)/2,(p.im-m.im)/2);};
const mul=(a:Cx,b:Cx)=>cx(a.re*b.re-a.im*b.im,a.re*b.im+a.im*b.re);
const div=(a:Cx,b:Cx)=>{const d=b.re*b.re+b.im*b.im;return cx((a.re*b.re+a.im*b.im)/d,(a.im*b.re-a.re*b.im)/d);};
const add=(...x:Cx[])=>x.reduce((s,v)=>cx(s.re+v.re,s.im+v.im),cx());
/** S of a uniform line (characteristic impedance zc, complex gamma per metre) of given length, via ABCD. */
function lineS(zc:Cx,gamma:Cx,length:number):CMatrix{
 const g=cx(gamma.re*length,gamma.im*length),A=cosh(g),B=mul(zc,sinh(g)),C=div(sinh(g),zc),z0=cx(Z0);
 const bz=div(B,z0),cz=mul(C,z0),d=add(A,bz,cz,A),neg=(x:Cx)=>cx(-x.re,-x.im);
 return [[div(add(A,bz,neg(cz),neg(A)),d),div(cx(2),d)],[div(cx(2),d),div(add(neg(A),bz,neg(cz),A),d)]];
}
const close=(a:CMatrix,b:CMatrix,tol:number)=>{for(let i=0;i<a.length;i++)for(let j=0;j<a.length;j++)assert.ok(cabs(csub(a[i][j],b[i][j]))<tol,`S${i}${j} ${cabs(csub(a[i][j],b[i][j]))}`);};

test('two identical lossless sections cascade to one section of double length',()=>{
 const zc=cx(62),gamma=cx(0,2*Math.PI*1e9*Math.sqrt(2.6)/3e8);
 close(cascadeTwoPorts(lineS(zc,gamma,0.01),lineS(zc,gamma,0.01)),lineS(zc,gamma,0.02),1e-12);
});

test('dispersive R(f) stitched per frequency point stays passive and reciprocal and equals the long line',()=>{
 for(const f of [1e7,1e8,5e8,1e9]){
  const w=2*Math.PI*f,R=40*Math.sqrt(f/1e9),L=3.2e-7,C=1.1e-10;
  const zs=cx(R,w*L),yp=cx(0,w*C),gamma=(()=>{const z=mul(zs,yp),m=Math.hypot(z.re,z.im),a=Math.atan2(z.im,z.re);return cx(Math.sqrt(m)*Math.cos(a/2),Math.sqrt(m)*Math.sin(a/2));})();
  const zc=div(zs,gamma);
  const parts=[0.004,0.0071,0.0029].map(l=>lineS(zc,gamma,l)),stitched=cascadeTwoPorts(...parts);
  assert.ok(maxSingularValue(stitched)<=1+1e-12);
  assert.ok(reciprocityError(stitched)<1e-12);
  assert.ok(maxSingularValue(stitched)<1,'lossy channel must dissipate');
  close(stitched,lineS(zc,gamma,0.014),1e-11);
 }
});

test('terminating extra section conductors with a short leaves a decoupled line unchanged',()=>{
 const line=lineS(cx(55),cx(0,30),0.01);
 const S:CMatrix=Array.from({length:4},()=>Array.from({length:4},()=>cx()));
 // ports 0,3 carry the line; ports 1,2 carry a decoupled other line.
 const other=lineS(cx(70),cx(0,25),0.01);
 for(const [i,pi] of [0,3].entries())for(const [j,pj] of [0,3].entries())S[pi][pj]=line[i][j];
 for(const [i,pi] of [1,2].entries())for(const [j,pj] of [1,2].entries())S[pi][pj]=other[i][j];
 close(terminatePorts(S,[0,3],[1,2],cx(-1,0)),line,1e-13);
});

test('partition: pad, straight track, diagonal track and output pad give three sections with joints',()=>{
 const board={
  pads:[{at:{x:0,y:0},rotationDeg:-90,shape:{kind:'rect',widthMm:1,heightMm:5}},{at:{x:12,y:1},rotationDeg:0,shape:{kind:'rect',widthMm:0.8,heightMm:0.9}}],
  copper:{fixed:{tracks:[
   {widthMm:0.6,points:[{x:0.0005,y:0},{x:10,y:0}]},
   {widthMm:0.6,points:[{x:10,y:0},{x:11,y:1}]},
  ]}},
 };
 const contacts=[{sourcePath:'pads[0]',layer:'TOP'},{sourcePath:'pads[1]',layer:'TOP'},{sourcePath:'pads[2]',layer:'BOTTOM'},{sourcePath:'pads[3]',layer:'BOTTOM'}];
 const signal=[...contacts.slice(0,2),{sourcePath:'copper.fixed.tracks[0]',layer:'TOP'},{sourcePath:'copper.fixed.tracks[1]',layer:'TOP'}];
 // Pad 1 is centred at x=12 but the track ends at (11,1): put the output pad on the track end.
 board.pads[1].at={x:11.0005,y:1};
 const choice=chooseChannelModel(board,contacts,signal);
 assert.equal(choice.mode,'cascade');
 if(choice.mode!=='cascade')return;
 assert.deepEqual(choice.sections.map(s=>s.id),['pad-input','track-0','track-1']);
 assert.ok(Math.abs(choice.sections[0].lengthMm-2.5)<1e-9&&Math.abs(choice.sections[0].widthMm-1)<1e-9);
 assert.ok(Math.abs(choice.sections[1].lengthMm-7.5)<1e-3);
 assert.ok(Math.abs(choice.sections[2].lengthMm-Math.SQRT2)<1e-9);
 assert.deepEqual(choice.joints.map(j=>j.kind),['pad-to-track','bend']);
 assert.ok(Math.abs(choice.joints[1].bendDeg-45)<1e-6);
});

test('partition: branch, via and layer change stay on the one-domain path',()=>{
 const board={
  pads:[{at:{x:0,y:0},rotationDeg:-90,shape:{kind:'rect',widthMm:1,heightMm:5}},{at:{x:10.0005,y:0},rotationDeg:0,shape:{kind:'rect',widthMm:0.8,heightMm:0.9}}],
  copper:{fixed:{tracks:[{widthMm:0.6,points:[{x:0.0005,y:0},{x:10,y:0}]},{widthMm:0.6,points:[{x:5,y:0},{x:5,y:3}]}],vias:[{}]}},
 };
 const contacts=[{sourcePath:'pads[0]',layer:'TOP'},{sourcePath:'pads[1]',layer:'TOP'},{sourcePath:'pads[2]',layer:'BOTTOM'},{sourcePath:'pads[3]',layer:'BOTTOM'}];
 const base=[...contacts.slice(0,2),{sourcePath:'copper.fixed.tracks[0]',layer:'TOP'}];
 assert.equal(chooseChannelModel(board,contacts,base).mode,'cascade');
 const stub=chooseChannelModel(board,contacts,[...base,{sourcePath:'copper.fixed.tracks[1]',layer:'TOP'}]);
 assert.equal(stub.mode,'one-domain');
 assert.equal(chooseChannelModel(board,contacts,[...base,{sourcePath:'copper.fixed.vias[0]',layer:'TOP'}]).mode,'one-domain');
 assert.equal(chooseChannelModel(board,[contacts[0],{...contacts[1],layer:'BOTTOM'},contacts[2],contacts[3]],base).mode,'one-domain');
 assert.equal(chooseChannelModel(board,contacts.slice(0,2),base).mode,'one-domain');
});

test('partition: a long output pad aligned with the last track becomes its own section',()=>{
 const board={
  pads:[{at:{x:4.2,y:-1},rotationDeg:0,shape:{kind:'rect',widthMm:0.8,heightMm:0.9}},{at:{x:15.83,y:0},rotationDeg:90,shape:{kind:'rect',widthMm:1,heightMm:5.5}}],
  copper:{fixed:{tracks:[{widthMm:0.6,points:[{x:4.2,y:-1},{x:5.2,y:0}]},{widthMm:0.6,points:[{x:5.2,y:0},{x:15.83,y:0}]}]}},
 };
 const contacts=[{sourcePath:'pads[0]',layer:'TOP'},{sourcePath:'pads[1]',layer:'TOP'},{sourcePath:'pads[2]',layer:'BOTTOM'},{sourcePath:'pads[3]',layer:'BOTTOM'}];
 const choice=chooseChannelModel(board,contacts,[...contacts.slice(0,2),{sourcePath:'copper.fixed.tracks[0]',layer:'TOP'},{sourcePath:'copper.fixed.tracks[1]',layer:'TOP'}]);
 assert.equal(choice.mode,'cascade');
 if(choice.mode!=='cascade')return;
 assert.deepEqual(choice.sections.map(s=>s.id),['track-0','track-1','pad-output']);
 assert.ok(Math.abs(choice.sections[1].lengthMm-(10.63-2.75))<1e-9&&Math.abs(choice.sections[2].lengthMm-2.75)<1e-9&&choice.sections[2].widthMm===1);
});

test('structure: a via, a component pad and a stub name the blocks a cascade would need',()=>{
 const board={
  pads:[{at:{x:0,y:0},shape:{kind:'circle',diameterMm:1},layers:['TOP']},{at:{x:20,y:0},shape:{kind:'circle',diameterMm:2},layers:['TOP','BOTTOM']},
   {at:{x:8,y:0},shape:{kind:'rect',widthMm:1,heightMm:0.8},layers:['TOP']},{at:{x:0,y:-3},shape:{kind:'circle',diameterMm:2},layers:['TOP','BOTTOM']}],
  copper:{fixed:{tracks:[
   {widthMm:0.25,points:[{x:0,y:0},{x:8,y:0}]},{widthMm:0.25,points:[{x:8,y:0},{x:12,y:0}]},
   {widthMm:0.25,points:[{x:12,y:0},{x:12,y:0}]},
   {widthMm:0.25,points:[{x:12,y:0},{x:20,y:0}]},{widthMm:0.25,points:[{x:0,y:0},{x:0,y:-3}]}],
   vias:[{at:{x:12,y:0},diameterMm:0.6,fromLayer:'TOP',toLayer:'BOTTOM'}]}},
 };
 const contacts=[{sourcePath:'pads[0]',layer:'TOP'},{sourcePath:'pads[1]',layer:'TOP'}];
 const track=(i:number,layer:string)=>({sourcePath:'copper.fixed.tracks['+i+']',layer});
 const signal=[...contacts,{sourcePath:'pads[1]',layer:'BOTTOM'},{sourcePath:'pads[2]',layer:'TOP'},{sourcePath:'pads[3]',layer:'TOP'},{sourcePath:'copper.fixed.vias[0]',layer:'TOP'},{sourcePath:'copper.fixed.vias[0]',layer:'BOTTOM'},
  track(0,'TOP'),track(1,'TOP'),track(3,'BOTTOM'),track(4,'TOP')];
 const s=describeChannelStructure(board,contacts,signal);
 assert.equal(s.blocks.filter(b=>b.kind==='layer-transition').length,3,'via, output through-hole pad, stub-end through-hole pad');
 assert.equal(s.blocks.filter(b=>b.kind==='component-pad').length,1);
 assert.ok(s.blocks.some(b=>b.kind==='branch'),'stub branches at the input pad');
 assert.ok(s.sections.count>=3&&Math.abs(s.sections.totalLengthMm-(8+4+8+3))<1e-9);
 assert.equal(chooseChannelModel(board,[...contacts,{sourcePath:'pads[2]',layer:'BOTTOM'},{sourcePath:'pads[3]',layer:'BOTTOM'}],signal).mode,'one-domain');
});

test('structure: a four-port coupled pair with pours is a named refusal, pours are counted, all signal contacts are contacts',()=>{
 const pad=(x:number,y:number)=>({at:{x,y},shape:{kind:'rect',widthMm:1,heightMm:1},layers:['TOP']});
 const board={pads:[pad(0,0),pad(10,0),pad(0,3),pad(10,3)],copper:{fixed:{tracks:[{widthMm:0.25,points:[{x:0,y:0},{x:10,y:0}]},{widthMm:0.25,points:[{x:0,y:3},{x:10,y:3}]}],zones:[{}]}}};
 const p=(i:number)=>({sourcePath:'pads['+i+']',layer:'TOP'});
 const contacts=[p(0),p(1),p(2),p(3),p(0),p(1),p(2),p(3)];
 const signal=[p(0),p(1),p(2),p(3),{sourcePath:'copper.fixed.tracks[0]',layer:'TOP'},{sourcePath:'copper.fixed.tracks[1]',layer:'TOP'},{sourcePath:'copper.fixed.zones[0].filledPolygons[0].polygons[0]',layer:'TOP'}];
 const choice=chooseChannelModel(board,contacts,signal);
 assert.equal(choice.mode,'one-domain');
 assert.match((choice as any).reasons[0],/coupled multi-port set \(4 ports\)/);
 const s=describeChannelStructure(board,contacts,signal);
 assert.equal(s.pours,1);
 assert.ok(!s.blocks.some(b=>b.kind==='component-pad'),'the four signal pads are contacts, not component pads');
});
