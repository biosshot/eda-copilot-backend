//! Board-specific geometric evaluator. No block-score formulas are used here.
use cubecl::prelude::*;
use crate::compute::numerics::rp;

#[derive(CubeLaunch, CubeType)]
pub struct Input {
    pub sf: Array<f32>,
    pub si: Array<u32>,
    pub pf: Array<f32>,
    pub pi: Array<u32>,
    pub frame: Array<u32>,
    pub geometry: Array<f32>,pub pairs:Array<f32>,pub unary:Array<f32>,pub unary_tags:Array<u32>,
    pub fixed_geometry:Array<f32>,pub fixed_pairs:Array<f32>,pub fixed_unary:Array<f32>,pub fixed_tags:Array<u32>,
}
#[derive(Clone, Copy, CubeType)]
pub struct Costs {pub first:f32,pub second:f32}
#[derive(Clone, Copy, CubeType)]
pub struct B { pub l: f32, pub r: f32, pub t: f32, pub b: f32 }

#[cube]
fn min(a: f32, b: f32) -> f32 { if a < b { a } else { b } }
#[cube]
fn max(a: f32, b: f32) -> f32 { if a > b { a } else { b } }
#[cube]
fn pose(g: &Input, c: usize, i: usize) -> usize {
    let nf = g.frame[1] as usize;
    let nm = g.frame[2] as usize;
    let mut slot = 6 + i;
    if i >= nf { slot = 6 + nf + c * nm + i - nf; }
    g.frame[slot] as usize
}
#[cube]
fn template(g: &Input, p: usize) -> usize { g.pi[p] as usize }
#[cube]
fn coord(g: &Input, p: usize, f: usize, axis: usize) -> f32 {
    let t=template(g,p);
    let _axis=axis;
    let offset=g.pi[p+3] as usize+f-g.si[t+15] as usize;
    let mut value=0.0;
    if p/4<g.frame[1] as usize {value=g.fixed_geometry[offset];}else{value=g.geometry[offset];}value
}
#[cube]
fn box_at(g: &Input, p: usize, f: usize) -> B {
    B { l: coord(g,p,f,0), r:coord(g,p,f+1,0), t:coord(g,p,f+2,1), b:coord(g,p,f+3,1) }
}
#[cube]
fn raw_box(g: &Input, f: usize) -> B { B { l:g.sf[f],r:g.sf[f+1],t:g.sf[f+2],b:g.sf[f+3] } }
#[cube]
fn union(a: B, b: B) -> B { B { l:min(a.l,b.l),r:max(a.r,b.r),t:min(a.t,b.t),b:max(a.b,b.b) } }
#[cube]
fn packing(g: &Input, p: usize) -> B {
    box_at(g,p,g.si[template(g,p)+18] as usize)
}
#[cube]
fn overlap(a: B,b: B,c: f32) -> f32 {
    let x1=a.r+c-b.l; let x2=b.r+c-a.l;
    let y1=a.b+c-b.t; let y2=b.b+c-a.t;
    let mut result=0.0;
    if x1>0.0 && x2>0.0 && y1>0.0 && y2>0.0 { result=min(min(x1,x2),min(y1,y2)); }
    result
}
#[cube]
fn packing_overlap(g:&Input,a:usize,b:usize,c:f32)->f32 {
    let ta=template(g,a); let tb=template(g,b); let mut result=0.0;
    // A union bound can only reject pairs which the exact box loops reject.
    if overlap(packing(g,a),packing(g,b),c)>0.0 {
        for i in 0..g.si[ta+4] as usize { for j in 0..g.si[tb+4] as usize {
            result=max(result,overlap(box_at(g,a,g.si[ta+3] as usize+i*4),box_at(g,b,g.si[tb+3] as usize+j*4),c));
        }}
    }result
}
#[cube]
fn component(g:&Input,p:usize,i:usize)->usize { g.si[template(g,p)+5] as usize+i*8 }
#[cube]
fn component_clearance(g:&Input,a:usize,b:usize)->f32 {
    g.sf[g.si[3] as usize+g.si[a] as usize*g.si[1] as usize+g.si[b] as usize]
}
#[cube]
fn component_conflict(g:&Input,a:usize,b:usize)->bool {
    g.si[g.si[2] as usize+g.si[a] as usize*g.si[1] as usize+g.si[b] as usize]!=0
}
#[cube]
fn can_conflict(g:&Input,a:usize,b:usize)->bool {
    let ai=g.si[template(g,a)] as usize;let bi=g.si[template(g,b)] as usize;
    g.si[g.si[20] as usize+ai*g.si[12] as usize+bi]!=0
}
#[cube]
fn hard_overlap(g:&Input,a:usize,b:usize)->f32 {
    let na=g.si[template(g,a)+6] as usize; let nb=g.si[template(g,b)+6] as usize;
    let ai=g.si[template(g,a)] as usize;let bi=g.si[template(g,b)] as usize;
    let broad=g.sf[g.si[19] as usize+ai*g.si[12] as usize+bi];
    let mut result=0.0;
    if packing_overlap(g,a,b,broad)>0.0 {
        for i in 0..na {for j in 0..nb {
            let ca=component(g,a,i); let cb=component(g,b,j);
            if component_conflict(g,ca,cb) {
                let clearance=component_clearance(g,ca,cb);
                let ba=box_at(g,a,g.si[ca+2] as usize);let bb=box_at(g,b,g.si[cb+2] as usize);
                if g.si[ca+1]==g.si[cb+1] {
                    result=max(result,overlap(ba,bb,clearance));
                    for x in 0..g.si[ca+4] as usize {for y in 0..g.si[cb+4] as usize {
                        result=max(result,overlap(box_at(g,a,g.si[ca+3] as usize+x*4),box_at(g,b,g.si[cb+3] as usize+y*4),clearance));
                    }}
                } else {
                    for y in 0..g.si[cb+4] as usize {result=max(result,overlap(ba,box_at(g,b,g.si[cb+3] as usize+y*4),clearance));}
                    for x in 0..g.si[ca+4] as usize {result=max(result,overlap(box_at(g,a,g.si[ca+3] as usize+x*4),bb,clearance));}
                }
            }
        }}
    }
    if result<=1e-9 {result=0.0;} result
}
#[cube]
fn outside_bounds(b:B,board:B)->f32 {
    max(board.l-b.l,0.0)+max(b.r-board.r,0.0)+max(board.t-b.t,0.0)+max(b.b-board.b,0.0)
}
#[derive(Clone, Copy, CubeType)]
pub struct P { pub x:f32, pub y:f32 }
#[cube]
fn corner(b:B,i:usize)->P {
    let mut x=b.l;let mut y=b.t;
    if i==1 || i==2 {x=b.r;}if i==2 || i==3 {y=b.b;}P{x,y}
}
#[cube]
fn polygon_point(g:&Input,i:usize)->P {let f=g.si[4] as usize+i*2;P{x:g.sf[f],y:g.sf[f+1]}}
#[cube]
fn on_segment(p:P,a:P,b:P)->bool {
    ((b.x-a.x)*(p.y-a.y)-(b.y-a.y)*(p.x-a.x)).abs()<1e-6
        && p.x<=max(a.x,b.x)+1e-6 && p.x>=min(a.x,b.x)-1e-6
        && p.y<=max(a.y,b.y)+1e-6 && p.y>=min(a.y,b.y)-1e-6
}
#[cube]
fn distance_segment(p:P,a:P,b:P)->f32 {
    let dx=b.x-a.x;let dy=b.y-a.y;let length=dx*dx+dy*dy;
    let mut x=p.x-a.x;let mut y=p.y-a.y;
    if length>=1e-6 {
        let t=max(0.0,min(1.0,((p.x-a.x)*dx+(p.y-a.y)*dy)/length));
        let px=a.x+t*dx;let py=a.y+t*dy;x=p.x-px;y=p.y-py;
    }(x*x+y*y).sqrt()
}
#[cube]
fn inside_polygon(g:&Input,p:P)->bool {
    let count=g.si[5] as usize;let mut inside=false;let mut boundary:bool=false;
    if count>=3 {let mut j=count-1;for i in 0..count {
        let a=polygon_point(g,i);let b=polygon_point(g,j);
        if on_segment(p,a,b) {boundary=true;}
        if (a.y>p.y)!=(b.y>p.y) {
            if p.x<((b.x-a.x)*(p.y-a.y))/(b.y-a.y)+a.x {inside=!inside;}
        }j=i;
    }}inside || boundary
}
#[cube]
fn distance_polygon(g:&Input,p:P)->f32 {
    let count=g.si[5] as usize;let mut result=f32::from_bits(0x7f800000u32);
    for i in 0..count {result=min(result,distance_segment(p,polygon_point(g,i),polygon_point(g,(i+1)%count)));}result
}
#[cube]
fn crosses(a:P,b:P,c:P,d:P)->bool {
    let denominator=(a.x-b.x)*(c.y-d.y)-(a.y-b.y)*(c.x-d.x);let mut result=false;
    if denominator.abs()>=1e-6 {
        let t=((a.x-c.x)*(c.y-d.y)-(a.y-c.y)*(c.x-d.x))/denominator;
        let u=-((a.x-b.x)*(a.y-c.y)-(a.y-b.y)*(a.x-c.x))/denominator;
        result=t>=-1e-6 && t<=1.0+1e-6 && u>=-1e-6 && u<=1.0+1e-6;
    }result
}
#[cube]
fn outside_polygon(g:&Input,b:B,board:B,edge:f32)->bool {
    let mut outside=false;
    for i in 0..4 {let p=corner(b,i);
        if p.x<board.l+edge-1e-6 || p.x>board.r-edge+1e-6 || p.y<board.t+edge-1e-6 || p.y>board.b-edge+1e-6
            || !inside_polygon(g,p) || (edge>0.0 && distance_polygon(g,p)+1e-6<edge) {outside=true;}
    }
    for i in 0..4 {for j in 0..g.si[5] as usize {
        if crosses(corner(b,i),corner(b,(i+1)%4),polygon_point(g,j),polygon_point(g,(j+1)%g.si[5] as usize)) {outside=true;}
    }}outside
}
#[cube]
fn polygon_severity(g:&Input,b:B,bounds:B,edge:f32)->f32 {
    let mut severity=outside_bounds(b,bounds);
    for i in 0..4 {
        let p=corner(b,i);let d=distance_polygon(g,p);
        if !inside_polygon(g,p) || (edge>0.0 && d+1e-6<edge) {severity+=max(d,0.01)+max(edge-d,0.0);}
    }severity
}
#[cube]
fn envelope(g:&Input,p:usize,layer:u32)->B {
    let t=template(g,p); let nc=g.si[t+6] as usize;
    let mut result=B{l:f32::from_bits(0x7f800000u32),r:-f32::from_bits(0x7f800000u32),t:f32::from_bits(0x7f800000u32),b:-f32::from_bits(0x7f800000u32)};
    if nc==0 {for i in 0..g.si[t+4] as usize {add_box(&mut result,box_at(g,p,g.si[t+3] as usize+i*4));}}
    else {for i in 0..nc {
        let c=component(g,p,i);
        if g.si[c+1]==layer {add_box(&mut result,box_at(g,p,g.si[c+2] as usize));}
        else {for j in 0..g.si[c+4] as usize {add_box(&mut result,box_at(g,p,g.si[c+3] as usize+j*4));}}
    }} result
}
#[cube]
fn edge_distance(b:B,board:B,inset:f32,edge:u32)->f32 {
    let mut d=(b.b-(board.b-inset)).abs();
    if edge==0 {d=(b.l-(board.l+inset)).abs();}
    if edge==1 {d=(b.r-(board.r-inset)).abs();}
    if edge==2 {d=(b.t-(board.t+inset)).abs();} d
}

#[derive(Clone, Copy, CubeType)]
pub struct Endpoint { pub x:f32, pub y:f32, pub owner:u32, pub present:bool }
#[cube]
fn find_pose(g:&Input,c:usize,id:u32)->usize {
    let mut found=0xffffffffusize;
    if id<g.si[12] {
        let i=g.frame[g.frame[5] as usize+id as usize];
        if i!=0xffffffffu32 {found=pose(g,c,i as usize);}
    }found
}
#[cube]
fn center(b:B)->P {P{x:rp((b.l+b.r)/2.0),y:rp((b.t+b.b)/2.0)}}
#[cube]
fn endpoint(g:&Input,c:usize,e:usize)->Endpoint {
    let kind=g.si[e];let owner=g.si[e+1];let index=g.si[e+2];
    let mut result=Endpoint{x:0.0,y:0.0,owner:owner,present:false};
    if kind==0 {
        let f=g.si[e+3] as usize;assign_e(&mut result,Endpoint{x:g.sf[f],y:g.sf[f+1],owner:0xffffffffu32,present:true});
    }else if kind<4 {
        let p=find_pose(g,c,owner);
        if p!=0xffffffffusize {
            let t=template(g,p);let mut xy=center(packing(g,p));
            if kind==1 {
                let cp=g.si[t+9] as usize+index as usize*2;
                let f=g.si[cp] as usize;assign_p(&mut xy,P{x:coord(g,p,f,0),y:coord(g,p,f+1,1)});
            }else if kind==2 {
                let mut x=0.0;let mut y=0.0;let mut count:u32=0;
                for i in 0..g.si[t+10] as usize {
                    let cp=g.si[t+9] as usize+i*2;
                    if g.si[cp+1]==index {
                        let f=g.si[cp] as usize;x+=coord(g,p,f,0);y+=coord(g,p,f+1,1);count+=1;
                    }
                }
                assign_p(&mut xy,center(box_at(g,p,g.si[t+2] as usize)));
                if count>0 {assign_p(&mut xy,P{x:rp(x/count as f32),y:rp(y/count as f32)});}
            }
            assign_e(&mut result,Endpoint{x:xy.x,y:xy.y,owner:owner,present:true});
        }
    }result
}
#[cube]
fn relation_term(g:&Input,c:usize,i:usize)->Costs {
    let mut first=0.0;let mut second=0.0;
    {
        let r=g.si[13] as usize+i*7;let flags=g.si[r+2];
        if flags&1==0 {
            let from=endpoint(g,c,g.si[r] as usize);let mut to=endpoint(g,c,g.si[r+1] as usize);
            if from.present && to.present && from.owner!=to.owner {
                let f=g.si[r+6] as usize;let weight=g.sf[f];
                if flags&4!=0 && flags&8!=0 {to.x=rp(to.x+g.sf[f+3]);to.y=rp(to.y+g.sf[f+4]);}
                let dx=from.x-to.x;let dy=from.y-to.y;let distance=(dx*dx+dy*dy).sqrt();
                let mut multiplier=1.0;if flags&2!=0 {multiplier=5.0;}
                let mut limits=0.0;
                if g.si[r+3]!=0 {let excess=max(distance-g.sf[f+1],0.0);limits+=(excess*excess*1500.0+excess*120.0)*weight*multiplier;}
                if g.si[r+4]!=0 {let shortage=max(g.sf[f+2]-distance,0.0);limits+=(shortage*shortage*800.0+shortage*80.0)*weight*multiplier;}
                first=distance*weight+min(limits,250000.0);
                if flags&4!=0 && g.si[r+5]<4 {
                    let side=g.si[r+5];let mut ax=0.0;let mut ay=0.0;
                    if distance>0.000001 {ax=dx/distance;ay=dy/distance;}
                    let mut desired_x=0.0;let mut desired_y=0.0;
                    if side==0 {desired_x=-1.0;}if side==1 {desired_x=1.0;}
                    if side==2 {desired_y=-1.0;}if side==3 {desired_y=1.0;}
                    second=max(1.0-ax*desired_x-ay*desired_y,0.0)*weight*120.0;
                }
            }
        }
    }Costs{first,second}
}
#[cube]
fn placement(g:&Input,p:usize,id:u32)->usize {
    let t=template(g,p);let mut found=0xffffffffusize;
    for i in 0..g.si[t+12] as usize {let q=g.si[t+11] as usize+i*3;if g.si[q+1]==id {found=q;}}
    found
}
#[cube]
fn alignment_center(g:&Input,p:usize,id:u32)->P {
    let mut result=center(box_at(g,p,g.si[template(g,p)+2] as usize));
    if id!=0xffffffffu32 {let q=placement(g,p,id);if q!=0xffffffffusize {let f=g.si[q] as usize;assign_p(&mut result,P{x:coord(g,p,f,0),y:coord(g,p,f+1,1)});}}
    result
}
#[cube]
fn alignment_term(g:&Input,c:usize,i:usize)->Costs {
    let mut first=0.0;let mut second=0.0;
    {
        let r=g.si[15] as usize+i*8;let a=find_pose(g,c,g.si[r]);let b=find_pose(g,c,g.si[r+1]);
        if a!=0xffffffffusize && b!=0xffffffffusize {
            let f=g.si[r+4] as usize;let ac=alignment_center(g,a,g.si[r+2]);let bc=alignment_center(g,b,g.si[r+3]);
            let error=max(min((ac.x-bc.x).abs(),(ac.y-bc.y).abs())-g.sf[f+1],0.0);
            let mut cost=error-0.5;if error<=1.0 {cost=error*error/2.0;}
            first=g.sf[f]*g.sf[f+2]*cost;
            if g.si[r+5]!=0xffffffffu32 {
                let ap=placement(g,a,g.si[r+5]);let bp=placement(g,b,g.si[r+6]);
                if ap!=0xffffffffusize && bp!=0xffffffffusize {
                    let difference=g.si[ap+2] as i32-g.si[bp+2] as i32;
                    // The CPU source multiplies weight and similarity before
                    // (1-cos)/2; preserve its multiplication/division sequence.
                    second=g.sf[f+3]*g.sf[f+2]*g.sf[g.si[r+7] as usize+(difference+359) as usize]/2.0;
                }
            }
        }
    }Costs{first,second}
}

#[cube]
fn cp(g:&Input,p:usize,index:usize)->P {
    let t=template(g,p);let f=g.si[g.si[t+9] as usize+index*2] as usize;
    P{x:coord(g,p,f,0),y:coord(g,p,f+1,1)}
}
#[cube]
fn ordinary_term(g:&Input,c:usize,i:usize)->f32 {
    let mut score=0.0;
    {
        let pair=g.frame[3] as usize+i*5;
        let a=pose(g,c,g.frame[pair] as usize);let b=pose(g,c,g.frame[pair+1] as usize);
        let mut distance=f32::from_bits(0x7f800000u32);
        for net in 0..g.frame[pair+4] as usize {
            let q=g.frame[pair+3] as usize+net*4;
            let mut shortest=f32::from_bits(0x7f800000u32);
            for x in 0..g.frame[q+1] as usize {for y in 0..g.frame[q+3] as usize {
                let left=cp(g,a,g.frame[g.frame[q] as usize+x] as usize);
                let right=cp(g,b,g.frame[g.frame[q+2] as usize+y] as usize);
                let dx=left.x-right.x;let dy=left.y-right.y;
                shortest=min(shortest,(dx*dx+dy*dy).sqrt());
            }}distance=min(distance,shortest);
        }
        score+=min(distance,30.0)*g.pf[g.frame[pair+2] as usize]*0.5;
    }score
}

#[derive(Clone, Copy, CubeType)]
pub struct Port {pub point:P,pub normal:P,pub present:bool}
#[cube]
fn resolve_port(g:&Input,c:usize,group:usize)->Port {
    let mut result=Port{point:P{x:0.0,y:0.0},normal:P{x:0.0,y:0.0},present:false};
    let mut best_ref=0xffffffffu32;let mut best_state=0xffffffffusize;
    for e in 0..g.si[group+2] as usize {
        let entry=g.si[group+1] as usize+e*3;let owner=g.si[entry];let reference=g.si[entry+2];
        for i in 0..g.frame[1] as usize+g.frame[2] as usize {
            let p=pose(g,c,i);let t=template(g,p);
            if g.si[t]==owner && (!result.present || reference<best_ref || (reference==best_ref && i<best_state)) {
                let f=g.si[g.si[t+13] as usize+g.si[entry+1] as usize] as usize;
                assign_port(&mut result,Port{point:P{x:coord(g,p,f,0),y:coord(g,p,f+1,1)},normal:P{x:g.sf[f+2],y:g.sf[f+3]},present:true});
                best_ref=reference;best_state=i;
            }
        }
    }result
}
#[cube]
fn magnitude(p:P)->f32 {(p.x*p.x+p.y*p.y).sqrt()}
#[cube]
fn normalize(p:P)->P {
    let length=magnitude(p);let mut result=P{x:0.0,y:0.0};
    if length>0.000001 {assign_p(&mut result,P{x:p.x/length,y:p.y/length});}result
}
#[cube]
fn dot(a:P,b:P)->f32 {a.x*b.x+a.y*b.y}
#[cube]
fn path_term(g:&Input,c:usize,path:usize)->f32 {
    let mut total=0.0;
    {
        let info=g.si[17] as usize+path*4;let groups=g.si[info] as usize;let count=g.si[info+1] as usize;
        let mut first=P{x:0.0,y:0.0};let mut last=P{x:0.0,y:0.0};let mut resolved:u32=0;
        for i in 0..count {let port=resolve_port(g,c,groups+i*4);
            if port.present {if resolved==0 {assign_p(&mut first,port.point);}assign_p(&mut last,port.point);resolved+=1;}
        }
        let chord=P{x:last.x-first.x,y:last.y-first.y};let direct=magnitude(chord);
        if resolved>=3 && direct>=0.001 {
            let axis=normalize(chord);let mut previous=first;let mut have_point=false;
            let mut previous_edge=P{x:0.0,y:0.0};let mut edges:u32=0;
            let mut distance=0.0;let mut backtrack=0.0;let mut turns=0.0;
            for i in 0..count {let port=resolve_port(g,c,groups+i*4);
                if port.present {
                    if have_point {
                        let edge=P{x:port.point.x-previous.x,y:port.point.y-previous.y};let length=magnitude(edge);
                        if length>0.001 {
                            distance+=length;backtrack+=max(-dot(edge,axis),0.0);
                            let unit=normalize(edge);
                            if edges>0 {turns+=max(1.0-dot(previous_edge,unit),0.0);}
                            assign_p(&mut previous_edge,unit);edges+=1;
                        }
                    }assign_p(&mut previous,port.point);have_point=true;
                }
            }
            if edges>=2 {
                let mut facing=0.0;
                if g.si[info+3]!=0 {
                    for i in 0..count {
                        let group=groups+i*4;let order=g.si[group] as i32;let target=g.si[group+3];
                        if order>=0 && order%2==0 && target!=0xffffffffu32 {
                            let source=resolve_port(g,c,group);let dest=resolve_port(g,c,groups+target as usize*4);
                            if source.present && dest.present {
                                let link=normalize(P{x:dest.point.x-source.point.x,y:dest.point.y-source.point.y});
                                if magnitude(link)>=0.001 {
                                    if magnitude(source.normal)>0.001 {facing+=max(1.0-dot(normalize(source.normal),link),0.0);}
                                    if magnitude(dest.normal)>0.001 {facing+=max(1.0-dot(normalize(dest.normal),P{x:-link.x,y:-link.y}),0.0);}
                                }
                            }
                        }
                    }
                }
                let f=g.si[info+2] as usize;let detour=max(distance-direct,0.0);
                total+=rp((detour*g.sf[f]+backtrack*42.0+turns*g.sf[f+1]+facing*12.0)*g.sf[f+2]*g.sf[f+3]);
            }
        }
    }total
}
#[cube(launch_unchecked)]
pub fn materialize(g:&Input,values:&mut Array<f32>,start:u32) {
    let p=(CUBE_POS_X as usize+start as usize)*4;let local=CUBE_POS_Y as usize*128+UNIT_POS_X as usize;
    let t=template(g,p);let len=g.si[t+16] as usize;
    if local<len {
        let axis=g.si[g.si[t+17] as usize+local] as usize;
        let mut value=g.sf[g.si[t+15] as usize+local];let offset=g.pi[p+1] as usize;
        if axis<2 {for step in 0..g.pi[p+2] as usize {value=rp(value+g.pf[offset+step*2+axis]);}}
        let output=g.pi[p+3] as usize+local;values[output]=value;
    }
}
#[cube(launch_unchecked)]
pub fn unary(g:&Input,values:&mut Array<f32>,counts:&mut Array<u32>,start:u32,end:u32,#[comptime] full_ranks:bool) {
    let index=ABSOLUTE_POS as usize+start as usize;
    if index<end as usize {
        let board=raw_box(g,0);let full=raw_box(g,4);let mut hard=0u32;let mut bias=0.0;
        let obstacles=g.si[7] as usize;let slot=index*(obstacles+6);
            let a=index*4;let ta=template(g,a);let nc=g.si[ta+6] as usize;
            let mut outside=false;let mut outside_severity=0.0;
            if nc==0 {
                for k in 0..g.si[ta+4] as usize {
                    let b=box_at(g,a,g.si[ta+3] as usize+k*4);
                    let edge=g.sf[g.si[ta+8] as usize];
                    if outside_polygon(g,b,full,edge) {outside=true;}
                    let bounds=B{l:full.l+edge,r:full.r-edge,t:full.t+edge,b:full.b-edge};
                    if full_ranks {outside_severity+=polygon_severity(g,b,bounds,edge);}
                }
            } else {for k in 0..nc {
                let c=component(g,a,k);let edge=g.sf[g.si[c+6] as usize];let bounds=raw_box(g,g.si[c+5] as usize);
                let mut component_severity=0.0;
                for q in 0..g.si[c+4] as usize+1 {
                    let mut off=g.si[c+2] as usize;
                    if q>0 {off=g.si[c+3] as usize+(q-1)*4;}
                    let b=box_at(g,a,off);
                    if g.si[c+7]!=0 {
                        let d=outside_bounds(b,bounds);if d>0.0 {outside=true;}component_severity+=d;
                    }else {
                        if outside_polygon(g,b,full,edge) {outside=true;}
                        if full_ranks {component_severity+=polygon_severity(g,b,bounds,edge);}
                    }
                }
                outside_severity+=component_severity;
            }}
            if outside {hard+=1;}
            let b=packing(g,a);let ep=g.si[ta+7] as usize;
            let mut violation=0.0;let mut ep_score=0.0;
            if ep!=0 {
                let ef=g.si[ep+1] as usize;let inset=max(g.sf[ef],0.0);let mut distance=f32::from_bits(0x7f800000u32);ep_score=f32::from_bits(0x7f800000u32);
                for e in 0..g.si[ep] as usize {
                    let edge=g.si[ep+4+e];let d=edge_distance(b,full,inset,edge);distance=min(distance,d);
                    let mut exact=0.0;
                    if edge<2 && g.si[ep+3]!=0 {exact=(rp((b.t+b.b)/2.0)-g.sf[ef+2]).abs();}
                    if edge>=2 && g.si[ep+2]!=0 {exact=(rp((b.l+b.r)/2.0)-g.sf[ef+1]).abs();}
                    if full_ranks {ep_score=min(ep_score,d*800.0+exact*35.0);}
                }
                violation=max(distance-max(0.05,g.sf[9]*0.51),0.0)*8.0;
            }
            if violation>0.0 {hard+=1;}
            values[slot]=outside_severity;values[slot+1]=violation;
            for ob in 0..g.si[7] as usize {
                let obstacle=raw_box(g,g.si[6] as usize+ob*4);let mut depth=0.0;
                for k in 0..g.si[ta+4] as usize {depth=max(depth,overlap(box_at(g,a,g.si[ta+3] as usize+k*4),obstacle,g.sf[8]));}
                if depth>0.0 {hard+=1;}values[slot+2+ob]=depth;
            }
            let mut region_severity=0.0;
            for r in 0..g.si[9] as usize {
                let ri=g.si[8] as usize+r*2;let region=raw_box(g,g.si[ri] as usize);let mask=g.si[ri+1];let mut violated=false;
                for k in 0..nc {
                    let c=component(g,a,k);
                    if g.si[g.si[10] as usize+r*g.si[1] as usize+g.si[c] as usize]==0 {
                        let layer=g.si[c+1];let mut depth=0.0;
                        if mask&(1u32<<layer)!=0 {depth=overlap(box_at(g,a,g.si[c+2] as usize),region,0.0);}
                        if mask&(1u32<<(1u32-layer))!=0 {
                            for q in 0..g.si[c+4] as usize {depth=max(depth,overlap(box_at(g,a,g.si[c+3] as usize+q*4),region,0.0));}
                        }
                        if depth>0.0 {violated=true;}region_severity+=depth;
                    }
                }
                if violated {hard+=1;}
            }
            values[slot+2+obstacles]=region_severity;
            if full_ranks && g.si[ta+1]&1==0 {
                let area=(b.r-b.l)*(b.b-b.t);let board_area=max((board.r-board.l)*(board.b-board.t),1.0);let ratio=area/board_area;
                if ratio>=0.055 || g.si[ta+1]&2!=0 {
                    let distance=min(min((b.l-board.l).abs(),(board.r-b.r).abs()),min((b.t-board.t).abs(),(board.b-b.b).abs()));
                    bias+=distance*min(1.0+ratio*60.0,8.0);
                }
            }
            values[slot+3+obstacles]=bias;values[slot+4+obstacles]=ep_score;counts[index]=hard;

    }
}
#[cube]
fn fixed_pair_count(n:usize)->usize {let mut count=0usize;if n>1 {count=n*(n-1)/2;}count}
#[cube]
fn pair_index(g:&Input,c:usize,i:usize,j:usize)->usize {
    let nf=g.frame[1] as usize;let nm=g.frame[2] as usize;let fixed=fixed_pair_count(nf);
    let cross=nf*nm+fixed_pair_count(nm);let mut result=0usize;
    if j<nf {result=i*(2*nf-i-1)/2+j-i-1;}
    else {
        let mut local=i*nm+j-nf;
        if i>=nf {let k=i-nf;local=nf*nm+k*(2*nm-k-1)/2+j-i-1;}
        result=fixed+c*cross+local;
    }result
}
#[cube(launch_unchecked)]
pub fn pair_terms(g:&Input,values:&mut Array<f32>,start:u32,end:u32,#[comptime] full_ranks:bool) {
    let index=ABSOLUTE_POS as usize+start as usize;let nf=g.frame[1] as usize;let nm=g.frame[2] as usize;
    let fixed=fixed_pair_count(nf);let cross=nf*nm+fixed_pair_count(nm);
    if index<end as usize {
        let mut candidate=0usize;let mut ordinal=index;let mut size=nf;
        let is_fixed=index<fixed;
        if !is_fixed {candidate=(index-fixed)/cross;ordinal=(index-fixed)%cross;size=nf+nm;}
        let mut i=0usize;let mut j=0usize;let mut preceding=0usize;let mut found=false;
        if !is_fixed && ordinal<nf*nm {
            i=ordinal/nm;j=nf+ordinal%nm;
        } else {
            let mut start=0usize;
            if !is_fixed {ordinal-=nf*nm;start=nf;}
            for row in start..size {
                let first=row+1;let count=size-first;
                if !found && ordinal<preceding+count {i=row;j=first+ordinal-preceding;found=true;}
                preceding+=count;
            }
        }
        let a=pose(g,candidate,i);let ta=template(g,a);
        let mut envelope_score=0.0;let mut spacing=0.0;
                let b=pose(g,candidate,j); let d=hard_overlap(g,a,b);
                if full_ranks && can_conflict(g,a,b) {
                    let mut depth=0.0;let mut closest=f32::from_bits(0x7f800000u32);
                    for layer in 0..2u32 {
                        let ea=envelope(g,a,layer);let eb=envelope(g,b,layer);
                        if ea.l<=ea.r && eb.l<=eb.r {
                            depth=max(depth,overlap(ea,eb,g.sf[8]));
                            let dx=max(max(ea.l-eb.r,eb.l-ea.r),0.0);let dy=max(max(ea.t-eb.b,eb.t-ea.b),0.0);
                            closest=min(closest,(dx*dx+dy*dy).sqrt());
                        }
                    }
                    if depth>0.0 {envelope_score=depth*depth*700.0+depth*140.0;}
                    let tb=template(g,b);
                    let exempt=g.si[g.si[11] as usize+g.si[ta] as usize*g.si[12] as usize+g.si[tb] as usize]!=0;
                    if g.sf[11]>0.0 && closest<f32::from_bits(0x7f800000u32) && !(g.si[ta+1]&1!=0 && g.si[tb+1]&1!=0) && !exempt {
                        let deficit=max(g.sf[8]+g.sf[11]-closest,0.0);
                        spacing=18.0*deficit*deficit;
                    }
                }
        values[index*3]=d;values[index*3+1]=envelope_score;values[index*3+2]=spacing;
    }
}
#[cube(launch_unchecked)]
pub fn ranks(g:&Input,scores:&mut Array<f32>,tags:&mut Array<u32>,terms:&mut Array<f32>,#[comptime] verify:bool) {
    let candidate=CUBE_POS as usize;let lane=UNIT_POS as usize;
    let mut first=SharedMemory::<f32>::new(128usize);let mut second=SharedMemory::<f32>::new(128usize);
    let mut relations=0.0;let mut alignment=0.0;let mut path=0.0;let mut ordinary=0.0;
    // Calculate independent terms across the workgroup, then retain the exact
    // CPU addition order. Floating-point reduction is deliberately not a tree.
    for kind in 0..4usize {
        let mut count=g.si[14] as usize;
        if kind==1 {count=g.si[16] as usize;}if kind==2 {count=g.si[18] as usize;}if kind==3 {count=g.frame[4] as usize;}
        let mut offset=0usize;
        while offset<count {
            let i=offset+lane;let mut a=0.0;let mut b=0.0;
            if i<count {
                if kind==0 {let value=relation_term(g,candidate,i);a=value.first;b=value.second;}
                if kind==1 {let value=alignment_term(g,candidate,i);a=value.first;b=value.second;}
                if kind==2 {a=path_term(g,candidate,i);}
                if kind==3 {a=ordinary_term(g,candidate,i);}
            }
            first[lane]=a;second[lane]=b;sync_cube();
            if lane==0 {for j in 0..128usize {if offset+j<count {
                if kind==0 {relations+=first[j];relations+=second[j];}
                if kind==1 {alignment+=first[j];alignment+=second[j];}
                if kind==2 {path+=first[j];}
                if kind==3 {ordinary+=first[j];}
            }}}
            sync_cube();offset+=128;
        }
    }
    if lane==0 {
        let nf=g.frame[1] as usize;let nm=g.frame[2] as usize;let n=nf+nm;
        let mut hard=0u32;let mut severity=0.0;let mut envelope_score=0.0;let mut spacing=0.0;
        let mut bias=0.0;let mut edge_score=0.0;
        let mut extent=B{l:f32::from_bits(0x7f800000u32),r:-f32::from_bits(0x7f800000u32),t:f32::from_bits(0x7f800000u32),b:-f32::from_bits(0x7f800000u32)};
        for i in 0..n {
            let a=pose(g,candidate,i);let ta=template(g,a);
            for k in 0..g.si[ta+4] as usize {add_box(&mut extent,box_at(g,a,g.si[ta+3] as usize+k*4));}
            for j in i+1..n {
                let off=pair_index(g,candidate,i,j)*3;let depth=pair_value(g,off);
                if depth>0.0 {hard+=1;}severity+=depth;envelope_score+=pair_value(g,off+1);spacing+=pair_value(g,off+2);
            }
        }
        let obstacles=g.si[7] as usize;
        for i in 0..n {
            let p=pose(g,candidate,i);let slot=p/4*(obstacles+6);
            hard+=unary_tag(g,p/4);severity+=unary_value(g,slot)+unary_value(g,slot+1);
            for ob in 0..obstacles {severity+=unary_value(g,slot+2+ob);}
            severity+=unary_value(g,slot+2+obstacles);bias+=unary_value(g,slot+3+obstacles);edge_score+=unary_value(g,slot+4+obstacles);
        }
        let mut area_weight=0.8;let mut perimeter_weight=1.5;let mut overlap_weight=1.0;let mut edge_weight=1.0;let mut relation_weight=1.0;let mut topology_weight=5.0;
        if g.si[0]!=0 {area_weight=1.2;perimeter_weight=8.0;overlap_weight=1.2;edge_weight=0.4;relation_weight=0.35;topology_weight=3.0;}
        let mut score=0.0;let mut w=0.0;let mut h=0.0;

        if n>0 {
            if extent.l<=extent.r {w=extent.r-extent.l;h=extent.b-extent.t;}
            score=relations*relation_weight+severity*1000000.0+envelope_score*overlap_weight+w*h*area_weight*g.sf[10]
                +(w+h)*perimeter_weight*g.sf[10]+spacing+alignment+bias*edge_weight+edge_score+(path+ordinary)*topology_weight;
        }
        scores[candidate*2]=severity;scores[candidate*2+1]=score;tags[candidate]=hard;
        if verify {
            let t0=candidate*10;let t1=t0+1;let t2=t0+2;let t3=t0+3;let t4=t0+4;
            let t5=t0+5;let t6=t0+6;let t7=t0+7;let t8=t0+8;let t9=t0+9;
            terms[t0]=relations;terms[t1]=envelope_score;terms[t2]=w*h;terms[t3]=w+h;terms[t4]=spacing;
            terms[t5]=alignment;terms[t6]=bias;terms[t7]=edge_score;terms[t8]=path+ordinary;terms[t9]=ordinary;
        }
    }
}

#[cube]
fn assign_b(target:&mut B,value:B) {target.l=value.l;target.r=value.r;target.t=value.t;target.b=value.b;}
#[cube]
fn assign_p(target:&mut P,value:P) {target.x=value.x;target.y=value.y;}
#[cube]
fn assign_e(target:&mut Endpoint,value:Endpoint) {target.x=value.x;target.y=value.y;target.owner=value.owner;target.present=value.present;}
#[cube]
fn assign_port(target:&mut Port,value:Port) {
    target.point.x=value.point.x;target.point.y=value.point.y;
    target.normal.x=value.normal.x;target.normal.y=value.normal.y;target.present=value.present;
}

#[cube]
fn add_box(target:&mut B,value:B) {target.l=min(target.l,value.l);target.r=max(target.r,value.r);target.t=min(target.t,value.t);target.b=max(target.b,value.b);}

#[cube(launch_unchecked)]
pub fn legality_parent(g:&Input,parent:&mut Array<u32>) {
    let nf=g.frame[1] as usize;let mut hard=0u32;
    for i in 0..nf {let p=pose(g,0,i);hard+=unary_tag(g,p/4);}
    for pair in 0..fixed_pair_count(nf) {if pair_value(g,pair*3)>0.0 {hard+=1;}}
    parent[0]=hard;
}

#[cube(launch_unchecked)]
pub fn legality(g:&Input,parent:&Array<u32>,tags:&mut Array<u32>) {
    let candidate=ABSOLUTE_POS as usize;
    if candidate<g.frame[0] as usize {
        let nf=g.frame[1] as usize;let n=nf+g.frame[2] as usize;let mut hard=parent[0];
        for i in nf..n {let p=pose(g,candidate,i);hard+=unary_tag(g,p/4);}
        for i in 0..n {
            let mut first=i+1;if first<nf {first=nf;}
            for j in first..n {if pair_value(g,pair_index(g,candidate,i,j)*3)>0.0 {hard+=1;}}
        }tags[candidate]=hard;
    }
}

#[cube]
fn less(ah:u32,av:f32,ascore:f32,ai:usize,bh:u32,bv:f32,bscore:f32,bi:usize)->bool {
    ah<bh || (ah==bh && (av<bv || (av==bv && (ascore<bscore || (ascore==bscore && ai<bi)))))
}
#[cube(launch_unchecked)]
pub fn clear_selection(ids:&mut Array<u32>) {
    let i=ABSOLUTE_POS as usize;if i<ids.len() {ids[i]=0xffffffffu32;}
}

// Independent candidate ranks; strict ordinal tie-break gives unique writers.
#[cube(launch_unchecked)]
pub fn select(scores:&Array<f32>,tags:&Array<u32>,alignment:&Array<u32>,ids:&mut Array<u32>,values:&mut Array<f32>,
    #[comptime] ordinary:usize,#[comptime] aligned:usize,
) {
    let i=ABSOLUTE_POS as usize;
    if i<tags.len() {
        let channel=alignment[i];let h=tags[i];let v=scores[i*2];let score=scores[i*2+1];
        let mut limit=tags.len()*0+ordinary;let mut offset=0usize;
        if channel!=0 {limit=aligned;offset=ordinary;}
        let mut rank=0usize;let mut j=0usize;
        while j<tags.len() && rank<limit {
            if alignment[j]==channel && less(tags[j],scores[j*2],scores[j*2+1],j,h,v,score,i) {rank+=1;}
            j+=1;
        }
        if rank<limit {let slot=(offset+rank)*2;
            ids[slot]=i as u32;ids[slot+1]=h;values[slot]=v;values[slot+1]=score;
        }
    }
}

#[cube]
fn pair_value(g:&Input,index:usize)->f32 {
    let mut value=0.0;if index<fixed_pair_count(g.frame[1] as usize)*3 {value=g.fixed_pairs[index];}else{value=g.pairs[index];}value
}
#[cube]
fn unary_value(g:&Input,index:usize)->f32 {
    let mut value=0.0;if index<g.frame[1] as usize*(g.si[7] as usize+6) {value=g.fixed_unary[index];}else{value=g.unary[index];}value
}
#[cube]
fn unary_tag(g:&Input,index:usize)->u32 {
    let mut value=0u32;if index<g.frame[1] as usize {value=g.fixed_tags[index];}else{value=g.unary_tags[index];}value
}
