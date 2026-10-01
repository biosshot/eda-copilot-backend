//! Refiner score kernels. Static topology/orientations plus compact pose changes.
use cubecl::prelude::*;
use crate::compute::numerics::rp;
#[derive(CubeLaunch, CubeType)]
pub struct Input {pub si:Array<u32>,pub sf:Array<f64>,pub bi:Array<u32>,pub bf:Array<f64>,pub geometry:Array<f64>,pub edges:Array<u32>,pub lengths:Array<f64>}
#[derive(Clone,Copy,CubeType)]
pub struct P {pub x:f64,pub y:f64}
#[derive(Clone,Copy,CubeType)]
pub struct B {pub l:f64,pub r:f64,pub t:f64,pub b:f64}
#[cube] fn min(a:f64,b:f64)->f64 {if a<b {a}else{b}}
#[cube] fn max(a:f64,b:f64)->f64 {if a>b {a}else{b}}
#[cube] fn length(x:f64,y:f64)->f64 {(x*x+y*y).sqrt()}
#[cube] fn distance(a:P,b:P)->f64 {length(a.x-b.x,a.y-b.y)}
#[cube] fn geometry_offset(g:&Input,c:usize,i:usize)->usize {c*g.si[11] as usize+g.si[g.si[1] as usize+i*4] as usize}
#[cube] fn point(g:&Input,c:usize,index:usize)->P {
 let a=g.si[4] as usize+index*2;let o=geometry_offset(g,c,g.si[a] as usize)+7+g.si[a+1] as usize*7;
 P{x:g.geometry[o],y:g.geometry[o+1]}
}
#[cube] fn pad_offset(g:&Input,c:usize,index:usize)->usize {
 let a=g.si[4] as usize+index*2;geometry_offset(g,c,g.si[a] as usize)+7+g.si[a+1] as usize*7
}
#[cube] fn box_at(g:&Input,o:usize)->B {B{l:g.geometry[o],r:g.geometry[o+1],t:g.geometry[o+2],b:g.geometry[o+3]}}
#[cube] fn assign_b(a:&mut B,b:B){a.l=b.l;a.r=b.r;a.t=b.t;a.b=b.b;}
#[cube] fn assign_p(a:&mut P,b:P){a.x=b.x;a.y=b.y;}
#[cube] fn target_box(g:&Input,c:usize,t:usize)->B {
 let kind=g.si[t];let id=g.si[t+1] as usize;let mut b=B{l:0.0,r:0.0,t:0.0,b:0.0};
 if kind==1 {assign_b(&mut b,box_at(g,geometry_offset(g,c,id)+3));}
 if kind==2 {let o=geometry_offset(g,c,id)+7+g.si[t+2] as usize*7;let x=g.geometry[o];let y=g.geometry[o+1];assign_b(&mut b,B{l:x,r:x,t:y,b:y});}
 if kind==3 {let f=g.si[t+1] as usize;assign_b(&mut b,B{l:g.sf[f],r:g.sf[f],t:g.sf[f+1],b:g.sf[f+1]});}
 if kind==4 {for j in 0..g.si[t+2] as usize {let q=box_at(g,geometry_offset(g,c,g.si[id+j] as usize)+3);if j==0 {assign_b(&mut b,q);}else{b.l=min(b.l,q.l);b.r=max(b.r,q.r);b.t=min(b.t,q.t);b.b=max(b.b,q.b);}}}
 b
}
#[cube] fn target_point(g:&Input,c:usize,t:usize)->P {
 let mut p=P{x:0.0,y:0.0};if g.si[t]==1 {let o=geometry_offset(g,c,g.si[t+1] as usize);p.x=g.geometry[o];p.y=g.geometry[o+1];}
 else{let b=target_box(g,c,t);p.x=(b.l+b.r)/2.0;p.y=(b.t+b.b)/2.0;}p
}
#[cube(launch_unchecked)]
pub fn materialize(g:&Input,out:&mut Array<f64>){
 let i=ABSOLUTE_POS as usize;let n=g.si[0] as usize;let c=i/n;let comp=i%n;
 if c<g.bi[0] as usize {let mut p=1+comp*3;let f=1+n*3+c*2;let begin=g.bi[f] as usize;
  for j in 0..g.bi[f+1] as usize {let q=begin+j*4;if g.bi[q] as usize==comp {p=q+1;}}
  let o=g.bi[p] as usize;let pos=g.bi[p+1] as usize;let layer=g.bi[p+2];let dx=g.bf[pos];let dy=g.bf[pos+1];
  let desc=g.si[1] as usize+comp*4;let dst=geometry_offset(g,c,comp);out[dst]=dx;out[dst+1]=dy;out[dst+2]=layer as f64;
  out[dst+3]=g.sf[o]+dx;out[dst+4]=g.sf[o+1]+dx;out[dst+5]=g.sf[o+2]+dy;out[dst+6]=g.sf[o+3]+dy;
  for pad in 0..g.si[desc+1] as usize {let src=o+4+pad*6;let a=dst+7+pad*7;
   out[a]=g.sf[src]+dx;out[a+1]=g.sf[src+1]+dy;out[a+2]=g.sf[src+2]+dx;out[a+3]=g.sf[src+3]+dx;out[a+4]=g.sf[src+4]+dy;out[a+5]=g.sf[src+5]+dy;
   let flags=g.si[g.si[desc+2] as usize+pad*4+3];let mut side=layer;if flags==1 {side=0;}if flags==2 {side=3-layer;}out[a+6]=side as f64;
  }
 }
}
#[cube(launch_unchecked)]
pub fn mst(g:&Input,connected:&mut Array<u32>,edges:&mut Array<u32>,lengths:&mut Array<f64>){
 let job=ABSOLUTE_POS as usize;let nets=g.si[2] as usize;let c=job/nets;let net=job%nets;
 if c<g.bi[0] as usize {let n=g.si[3] as usize+net*4;let start=g.si[n] as usize;let count=g.si[n+1] as usize;let es=g.si[n+2] as usize;let base=c*g.si[6] as usize+start;
  for j in 0..count {connected[base+j]=0;}if count>0 {connected[base]=1;}
  for step in 1..count {let mut best=1e300;let mut bf=0usize;let mut bt=0usize;let mut found=false;
   for a in 0..count {if connected[base+a]!=0 {for b in 0..count {if connected[base+b]==0 {
    let d=distance(point(g,c,start+a),point(g,c,start+b));
    if !found || d<best-0.001 || ((d-best).abs()<=0.001 && (a<bf || (a==bf && b<bt))) {best=d;bf=a;bt=b;found=true;}
   }}}}connected[base+bt]=1;let edge=c*g.si[5] as usize+es+step-1;edges[edge*3]=(start+bf) as u32;edges[edge*3+1]=(start+bt) as u32;edges[edge*3+2]=net as u32;lengths[edge]=best;
  }
 }
}
#[cube] fn cross(a:P,b:P,c:P)->f64 {(b.x-a.x)*(c.y-a.y)-(b.y-a.y)*(c.x-a.x)}
#[cube] fn opposite(a:f64,b:f64)->bool {(a>0.001 && b< -0.001)||(a< -0.001 && b>0.001)}
#[cube] fn intersects(a:P,b:P,c:P,d:P)->bool {opposite(cross(a,b,c),cross(a,b,d))&&opposite(cross(c,d,a),cross(c,d,b))}
#[cube] fn hits(a:P,b:P,q:B)->bool {
 let mut lo=0.0;let mut hi=1.0;let mut valid=true;
 for axis in 0..2 {let mut start=a.x;let mut delta=b.x-a.x;let mut low=q.l;let mut high=q.r;
  if axis==1 {start=a.y;delta=b.y-a.y;low=q.t;high=q.b;}
  if delta.abs()<0.000000001 {if start<low || start>high {valid=false;}}
  else{let t1=(low-start)/delta;let t2=(high-start)/delta;lo=max(lo,min(t1,t2));hi=min(hi,max(t1,t2));if lo>hi {valid=false;}}
 }valid
}
#[cube(launch_unchecked)]
pub fn segment_costs(g:&Input,out:&mut Array<f64>,masks:&mut Array<u32>){
 let job=ABSOLUTE_POS as usize;let ns=g.si[5] as usize;let c=job/ns;let i=job%ns;
 if c<g.bi[0] as usize {let e=job*3;let net=g.edges[e+2] as usize;let a=point(g,c,g.edges[e] as usize);let b=point(g,c,g.edges[e+1] as usize);let w=g.sf[g.si[g.si[3] as usize+net*4+3] as usize];let words=(ns+31)/32;for word in 0..words {masks[job*words+word]=0;}
  for j in i+1..ns {let f=(c*ns+j)*3;let other=g.edges[f+2] as usize;if other!=net {let x=point(g,c,g.edges[f] as usize);let y=point(g,c,g.edges[f+1] as usize);
   if distance(a,x)>=0.001 && distance(a,y)>=0.001 && distance(b,x)>=0.001 && distance(b,y)>=0.001 && intersects(a,b,x,y) {let off=job*words+j/32;masks[off]=masks[off]|(1u32<<(j%32) as u32);}
  }}out[job*2]=0.0;
  let mut pads=0.0;
  if g.sf[g.si[12] as usize]>0.0 {
   let ai=g.edges[e] as usize;let bi=g.edges[e+1] as usize;
   let ac=g.si[g.si[4] as usize+ai*2] as usize;let bc=g.si[g.si[4] as usize+bi*2] as usize;
   let al=g.geometry[pad_offset(g,c,ai)+6] as u32;let bl=g.geometry[pad_offset(g,c,bi)+6] as u32;let mut layer=0u32;
   if al==bl {layer=al;}else if al==0 {layer=bl;}else if bl==0 {layer=al;}
   let mut owner=0xffffffffu32;if ac==bc && g.si[g.si[1] as usize+ac*4+3]!=0 {owner=ac as u32;}
   // Obstacles are grouped into physical-pad identities; test all duplicate
   // records in the group before adding exactly one charge.
   for group in 0..g.si[15] as usize {let h=g.si[16] as usize+group*2;let mut hit=false;
    for k in 0..g.si[h+1] as usize {let o=g.si[h] as usize+k*3;let comp=g.si[o] as usize;let pad=g.si[o+1] as usize;let padnet=g.si[o+2];let q=geometry_offset(g,c,comp)+7+pad*7;let side=g.geometry[q+6] as u32;
     if owner!=comp as u32 && padnet!=net as u32 && (layer==0 || side==0 || layer==side) && hits(a,b,box_at(g,q+2)) {hit=true;}
    }if hit {pads+=1.0;}
   }
  }out[job*2+1]=pads;
 }
}
#[cube] fn dot(a:P,b:P)->f64 {a.x*b.x+a.y*b.y}
#[cube] fn normalize(p:P)->P {let l=length(p.x,p.y);let mut r=P{x:0.0,y:0.0};if l>0.000001 {r.x=p.x/l;r.y=p.y/l;}r}
#[cube] fn port(g:&Input,c:usize,index:usize)->P {target_point(g,c,g.si[index] as usize)}
#[cube] fn normal(g:&Input,c:usize,index:usize)->P {
 let p=port(g,c,index);let comp=g.si[index+2] as usize;let o=geometry_offset(g,c,comp);let dx=p.x-g.geometry[o];let dy=p.y-g.geometry[o+1];let l=length(dx,dy);let mut n=P{x:0.0,y:0.0};if l>0.001 {n.x=dx/l;n.y=dy/l;}n
}
#[cube] fn path_cost(g:&Input,c:usize,path:usize)->f64 {
 let t=g.si[10] as usize+path*5;let off=g.si[t] as usize;let count=g.si[t+1] as usize;let mut cost=0.0;
 if count>=3 {let first=port(g,c,off);let last=port(g,c,off+(count-1)*3);let direct=distance(first,last);
  if direct>=0.001 {let axis=normalize(P{x:last.x-first.x,y:last.y-first.y});let mut distance_sum=0.0;let mut back=0.0;let mut turns=0.0;let mut edges:u32=0;let mut prev=P{x:0.0,y:0.0};
   for j in 1..count {let a=port(g,c,off+(j-1)*3);let b=port(g,c,off+j*3);let edge=P{x:b.x-a.x,y:b.y-a.y};let len=length(edge.x,edge.y);
    if len>0.001 {distance_sum+=len;back+=max(-dot(edge,axis),0.0);if edges>0 {turns+=max(1.0-dot(normalize(prev),normalize(edge)),0.0);}assign_p(&mut prev,edge);edges+=1;}
   }
   if edges>=2 {let mut facing=0.0;if g.si[t+4]!=0 {for j in 0..count {let a=off+j*3;let order=g.si[a+1] as i32;if order>=0 && order%2==0 {for k in j+1..count {let b=off+k*3;if g.si[b+1] as i32==order+1 {let ap=port(g,c,a);let bp=port(g,c,b);let link=normalize(P{x:bp.x-ap.x,y:bp.y-ap.y});if length(link.x,link.y)>=0.001 {let an=normal(g,c,a);let bn=normal(g,c,b);if length(an.x,an.y)>0.001 {facing+=max(1.0-dot(normalize(an),link),0.0);}if length(bn.x,bn.y)>0.001 {facing+=max(1.0-dot(normalize(bn),P{x:-link.x,y:-link.y}),0.0);}}}}}}}
    let mut detour_weight=8.0;let mut turn_weight=5.0;if g.si[t+2]!=0 {detour_weight=22.0;turn_weight=18.0;}
    cost=rp((max(distance_sum-direct,0.0)*detour_weight+back*42.0+turns*turn_weight+facing*12.0)*g.sf[g.si[t+3] as usize])*20.0;
   }
  }
 }cost
}
#[cube(launch_unchecked)]
pub fn terms(g:&Input,out:&mut Array<f64>){
 let job=ABSOLUTE_POS as usize;let nt=g.si[7] as usize;let total=nt+g.si[9] as usize;let c=job/total;let i=job%total;
 if c<g.bi[0] as usize {let mut a=0.0;let mut b=0.0;let mut d=0.0;
  if i<nt {let t=g.si[8] as usize+i*5;let kind=g.si[t];let src=g.si[t+1] as usize;let dst=g.si[t+2] as usize;let f=g.si[t+3] as usize;let flags=g.si[t+4];let w=g.sf[f];
   if kind==0 {let dist=distance(target_point(g,c,src),target_point(g,c,dst));a=dist*w;if flags&1!=0 && dist>g.sf[f+2] {let x=dist-g.sf[f+2];b=x*x*w*20.0;}if flags&2!=0 && dist<g.sf[f+1] {let x=g.sf[f+1]-dist;d=x*x*w*20.0;}}
   if kind==1 {let x=target_box(g,c,src);let y=target_box(g,c,dst);let gap=max(max(y.l-x.r,x.l-y.r),max(y.t-x.b,x.t-y.b));if gap<g.sf[f+1] {let z=g.sf[f+1]-gap;a=z*z*w*20.0;}}
   if kind==2 {let x=g.geometry[geometry_offset(g,c,src)+2] as u32;let mut y=flags;if dst!=0xffffffffusize {y=g.geometry[geometry_offset(g,c,dst)+2] as u32;}if x!=y {a=w;}}
   if kind==3 {let x=target_box(g,c,src);let board=g.si[14] as usize;let mut distance=(g.sf[board+3]-x.b).abs();if flags==0 {distance=(x.l-g.sf[board]).abs();}if flags==1 {distance=(g.sf[board+1]-x.r).abs();}if flags==2 {distance=(x.t-g.sf[board+2]).abs();}a=distance*w;}
  }else{a=path_cost(g,c,i-nt);}
  out[job*3]=a;out[job*3+1]=b;out[job*3+2]=d;
 }
}
#[cube(launch_unchecked)]
pub fn reduce(g:&Input,segments:&Array<f64>,masks:&Array<u32>,terms:&Array<f64>,out:&mut Array<f64>){
 let c=ABSOLUTE_POS as usize;if c<g.bi[0] as usize {let ns=g.si[5] as usize;let nt=g.si[7] as usize;let total=nt+g.si[9] as usize;let mut score=0.0;let mut crossing=0.0;let mut pads=0.0;
  for j in 0..ns {let e=c*ns+j;let n=g.edges[e*3+2] as usize;let w=g.sf[g.si[g.si[3] as usize+n*4+3] as usize];let l=g.lengths[e];score+=l*10.0*w;score+=l*l*0.35*w;for k in j+1..ns {if masks[e*((ns+31)/32)+k/32]&(1u32<<(k%32) as u32)!=0 {let other=g.edges[(c*ns+k)*3+2] as usize;crossing+=(w*g.sf[g.si[g.si[3] as usize+other*4+3] as usize]).sqrt();}}for hit in 0..segments[e*2+1] as usize {let _hit=hit;pads+=w;}}
  score+=crossing*180.0;if g.sf[g.si[12] as usize]>0.0 {score+=pads*g.sf[g.si[12] as usize];}
  let mut fixed=0.0;let mut added=false;
  for j in 0..total {let mut kind=4u32;if j<nt {kind=g.si[g.si[8] as usize+j*5];}let t=(c*total+j)*3;
   if kind==2 {fixed+=terms[t];}else{if kind>=3 && !added {score+=fixed;added=true;}score+=terms[t];if kind==0 {score+=terms[t+1];score+=terms[t+2];}}
  }if !added {score+=fixed;}out[c]=score;
 }
}
