//! Rust/CubeCL translation of archived score.cl and select.cl (c0aaafb).
//! Floating and integer storage are separate. No IDs are encoded in floats.
use cubecl::prelude::*;
use crate::compute::numerics::rp;

// Header slots in frame_i; offsets refer to elements, never bytes.
pub const N: usize = 0;
pub const MOVING: usize = 1;
pub const COUNT: usize = 2;
pub const NETS: usize = 3;
pub const RELS: usize = 4;
pub const COMPONENTS: usize = 5;
pub const SEGMENTS: usize = 6;
pub const HIGH: usize = 7;
pub const REDUCED: usize = 8;
pub const SMOOTH: usize = 9;
pub const SMALL: usize = 10;
pub const DENSE: usize = 11;
pub const DIVERSE: usize = 12;
pub const FIXED_I: usize = 13;
pub const NET_I: usize = 14;
pub const REF_I: usize = 15;
pub const REL_I: usize = 16;
pub const EP_I: usize = 17;
pub const CP_F: usize = 18;
pub const PAD_F: usize = 19;
pub const RULE_F: usize = 20;
pub const CP_I: usize = 21;
pub const PAD_I: usize = 22;
pub const CONFLICT_I: usize = 23;
pub const GEOMETRY_F: usize = 24;
pub const COMPONENT_I: usize = 25;
pub const SOURCES: usize = 26;
pub const PORT_F:usize=27;
pub const PATH_I:usize=28;
pub const PATH_COUNT:usize=29;
pub const ORDINARY_I:usize=30;
pub const ORDINARY_COUNT:usize=31;
pub const FLAGS:usize=32;
pub const BOUNDS_F:usize=33;
pub const TARGET_F:usize=34;
pub const OBSTACLE_F:usize=35;
pub const OBSTACLE_COUNT:usize=36;
pub const WORLD_F:usize=37;
pub const OUTLINE_F:usize=38;
pub const OUTLINE_COUNT:usize=39;
pub const WORLD_OBSTACLE_I:usize=40;
pub const WORLD_OBSTACLE_COUNT:usize=41;
pub const HEADER: usize = 43;

#[derive(CubeLaunch, CubeType)]
pub struct Input {
    pub sf: Array<f32>,
    pub si: Array<i32>,
    pub ff: Array<f32>,
    pub fi: Array<u32>,
    pub poses: Array<f32>,
    pub ids: Array<u32>,
}
#[derive(Clone, Copy, CubeType)]
pub struct B {
    pub l: f32,
    pub r: f32,
    pub t: f32,
    pub b: f32,
}
#[derive(Clone, Copy, CubeType)]
pub struct P {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, CubeType)]
pub struct Prim {
    pub bbox: B,
    pub body: B,
    pub dx: f32,
    pub dy: f32,
    pub cp: usize,
    pub nc: usize,
    pub pad: usize,
    pub np: usize,
    pub component: usize,
    pub layer: i32,
    pub pins: u32,
    pub role: u32,
    pub power: u32,
    pub components: usize,
    pub component_count: usize,
    pub boxes: usize,
    pub box_count: usize,
    pub locked: u32,
    pub ports:usize,
}
#[derive(Clone, Copy, CubeType)]
pub struct Cp {
    pub x: f32,
    pub y: f32,
    pub net: i32,
    pub owner: i32,
    pub layer: i32,
}

#[derive(Clone, Copy, CubeType)]
struct Component {
    id: usize, layer: i32, pins: u32, role: u32, power: u32,
    body: B, through: usize, through_count: usize,
}
#[cube]
fn geometry_box(d: &Input, p: Prim, index: usize) -> B {
    let f = d.fi[GEOMETRY_F] as usize + index * 4;
    B { l: rp(d.sf[f]+p.dx), r: rp(d.sf[f+1]+p.dx), t: rp(d.sf[f+2]+p.dy), b: rp(d.sf[f+3]+p.dy) }
}
#[cube]
fn component(d: &Input, p: Prim, index: usize) -> Component {
    let t = d.fi[COMPONENT_I] as usize + (p.components+index)*8;
    Component { id: d.si[t] as usize, layer: d.si[t+1], pins: d.si[t+2] as u32,
        role: d.si[t+3] as u32, power: d.si[t+4] as u32,
        body: geometry_box(d,p,d.si[t+5] as usize), through: d.si[t+6] as usize, through_count: d.si[t+7] as usize }
}
#[cube]
fn can_conflict(d: &Input, a: Prim, b: Prim) -> bool {
    let mut result=false;
    for i in 0usize..a.component_count { for j in 0usize..b.component_count {
        let ac=component(d,a,i); let bc=component(d,b,j);
        if d.si[d.fi[CONFLICT_I] as usize + ac.id*d.fi[COMPONENTS] as usize + bc.id]!=0 { result=true; }
    }}
    result
}
#[cube]
fn component_overlap(d: &Input, a: Prim, b: Prim) -> f32 {
    let mut result=0.0f32;
    for i in 0usize..a.component_count { for j in 0usize..b.component_count {
        let ac=component(d,a,i); let bc=component(d,b,j);
        let index=ac.id*d.fi[COMPONENTS] as usize + bc.id;
        if d.si[d.fi[CONFLICT_I] as usize + index]!=0 {
            let cl=d.sf[d.fi[RULE_F] as usize + index];
            if ac.layer==bc.layer {
                result=max(result,overlap(ac.body,bc.body,cl));
                for ai in 0usize..ac.through_count { for bi in 0usize..bc.through_count {
                    result=max(result,overlap(geometry_box(d,a,ac.through+ai),geometry_box(d,b,bc.through+bi),cl));
                }}
            } else {
                for bi in 0usize..bc.through_count { result=max(result,overlap(ac.body,geometry_box(d,b,bc.through+bi),cl)); }
                for ai in 0usize..ac.through_count { result=max(result,overlap(geometry_box(d,a,ac.through+ai),bc.body,cl)); }
            }
        }
    }}
    result
}
#[cube]
fn primitive_overlap(d: &Input, a: Prim, b: Prim) -> f32 {
    let mut result=0.0f32;
    for i in 0usize..a.box_count { for j in 0usize..b.box_count {
        result=max(result,overlap(geometry_box(d,a,a.boxes+i),geometry_box(d,b,b.boxes+j),d.ff[0]));
    }}
    result
}
#[cube]
fn affinity(d: &Input, p: Prim) -> f32 {
    let mut count=0u32;
    for i in 0usize..p.component_count { count+=component(d,p,i).power; }
    count as f32 / max(p.component_count as f32,1.0)
}
#[cube]
fn point_segment(p: P,a: P,b: P) -> f32 {
    let dx=b.x-a.x; let dy=b.y-a.y; let sq=dx*dx+dy*dy;
    let mut result=distance(p,a);
    if sq>0.000001 {let t=min(1.0,max(0.0,((p.x-a.x)*dx+(p.y-a.y)*dy)/sq));result=distance(p,P{x:a.x+t*dx,y:a.y+t*dy});}
    result
}
#[cube]
fn point_box(p: P,b: B) -> f32 {
    let dx=max(0.0,max(b.l-p.x,p.x-b.r)); let dy=max(0.0,max(b.t-p.y,p.y-b.b));
    (dx*dx+dy*dy).sqrt()
}
#[cube]
fn segment_box(a: P,b: P,box_: B) -> f32 {
    let mut best=min(point_box(a,box_),point_box(b,box_));
    for i in 0usize..4usize {
        let corner=P{x:choose(i==0 || i==3,box_.l,box_.r),y:choose(i<2,box_.t,box_.b)};
        best=min(best,point_segment(corner,a,b));
        let mut projected=P{x:choose(i==0,box_.l,box_.r),y:min(box_.b,max(box_.t,a.y))};
        if i>=2 {projected.x=min(box_.r,max(box_.l,a.x));projected.y=choose(i==2,box_.t,box_.b);}
        best=min(best,point_segment(projected,a,b));
    }
    if hit(a.x,a.y,b.x,b.y,box_) {best=0.0;}
    best
}
#[cube]
fn power_yield(d: &Input,candidate: usize) -> f32 {
    let mut result=0.0f32;
    let n=d.fi[N] as usize;
    let mut nc=0usize;
    for i in 0usize..n {nc+=load_prim(d,i,candidate).nc;}
    if n>=3 && n<=36 && nc<=240 {
        let corridor=max(1.2,d.ff[0]*1.75);
        for ni in 0usize..d.fi[NETS] as usize {
            let net=d.fi[NET_I] as usize+ni*8;
            if (d.fi[net+7]&8)!=0 {
                let mut selected=Array::<u32>::new(8usize);
                let mut size=0usize;
                for j in 0usize..d.fi[net+1] as usize {
                    let ri=d.fi[net] as usize+j;
                    let pi=d.fi[d.fi[REF_I] as usize+ri*2] as usize;
                    if affinity(d,load_prim(d,pi,candidate))<0.75 && size<8 {selected[size]=ri as u32;size+=1;}
                }
                for i in 0usize..size {for j in i+1..size {
                    let ia=selected[i] as usize;let ib=selected[j] as usize;
                    let pa=d.fi[d.fi[REF_I] as usize+ia*2] as usize;let pb=d.fi[d.fi[REF_I] as usize+ib*2] as usize;
                    let ca=ref_cp(d,ia,candidate);let cb=ref_cp(d,ib,candidate);
                    let a=P{x:ca.x,y:ca.y};let b=P{x:cb.x,y:cb.y};let dist=distance(a,b);
                    if pa!=pb && dist>=0.5 && dist<=45.0 {for k in 0usize..n {
                        let p=load_prim(d,k,candidate);let power=affinity(d,p);
                        if k!=pa && k!=pb && power>=0.55 && p.locked==0 {
                            let outside=max(a.x,b.x)<p.bbox.l-corridor || min(a.x,b.x)>p.bbox.r+corridor || max(a.y,b.y)<p.bbox.t-corridor || min(a.y,b.y)>p.bbox.b+corridor;
                            if !outside {let depth=max(0.0,corridor-segment_box(a,b,p.bbox));result+=(depth*depth*220.0+depth*80.0)*power*choose((d.fi[net+7]&16)!=0,1.5,1.0);}
                        }
                    }}
                }}
            }
        }
    }
    result
}

#[cube]
fn endpoint_facing(source:P,target:P,bbox:B)->f32 {
    let c=center(bbox);let mut value=0.0f32;
    if distance(source,c)>=0.05 {
        let sx=source.x-c.x;let sy=source.y-c.y;let tx=target.x-c.x;let ty=target.y-c.y;
        let sl=(sx*sx+sy*sy).sqrt();let tl=(tx*tx+ty*ty).sqrt();
        let mut dot=0.0f32;
        if sl>0.000001 && tl>0.000001 {dot=(sx/sl)*(tx/tl)+(sy/sl)*(ty/tl);}
        value=max(0.0,1.0-dot);
    }value
}
#[cube]
fn magnitude(p:P)->f32 {(p.x*p.x+p.y*p.y).sqrt()}
#[cube]
fn unit(p:P)->P {let length=magnitude(p);let mut q=P{x:0.0,y:0.0};if length>0.000001 {q.x=p.x/length;q.y=p.y/length;}q}
#[cube]
fn dot(a:P,b:P)->f32 {a.x*b.x+a.y*b.y}
#[cube]
fn port(d:&Input,candidate:usize,reference:usize,normal:bool)->P {
    let p=load_prim(d,d.fi[reference] as usize,candidate);
    let f=d.fi[PORT_F] as usize+(p.ports+d.fi[reference+1] as usize)*4;
    let mut q=P{x:rp(d.sf[f]+p.dx),y:rp(d.sf[f+1]+p.dy)};
    if normal {q.x=d.sf[f+2];q.y=d.sf[f+3];}q
}
#[cube]
fn topology(d:&Input,candidate:usize)->f32 {
    let mut total=0.0f32;
    for path in 0usize..d.fi[PATH_COUNT] as usize {
        let info=d.fi[PATH_I] as usize+path*3;let refs=d.fi[info] as usize;let count=d.fi[info+1] as usize;let f=d.fi[info+2] as usize;
        if count>=3 {
            let first=port(d,candidate,refs,false);let last=port(d,candidate,refs+(count-1)*3,false);
            let chord=P{x:last.x-first.x,y:last.y-first.y};let direct=magnitude(chord);
            if direct>=0.001 {
                let axis=unit(chord);let mut previous_edge=P{x:0.0,y:0.0};let mut edges:u32=0;
                let mut length=0.0f32;let mut backtrack=0.0f32;let mut turns=0.0f32;let mut facing=0.0f32;
                for i in 1usize..count {
                    let a=port(d,candidate,refs+(i-1)*3,false);let b=port(d,candidate,refs+i*3,false);
                    let edge=P{x:b.x-a.x,y:b.y-a.y};let dist=magnitude(edge);
                    if dist>0.001 {
                        length+=dist;backtrack+=max(-dot(edge,axis),0.0);let direction=unit(edge);
                        if edges>0 {turns+=max(1.0-dot(previous_edge,direction),0.0);}
                        previous_edge.x=direction.x;previous_edge.y=direction.y;edges+=1;
                    }
                    let order=d.fi[refs+(i-1)*3+2] as i32;
                    if d.ff[f+4]!=0.0 && order>=0 && order%2==0 && d.fi[refs+i*3+2] as i32==order+1 {
                        let link=unit(edge);
                        if magnitude(link)>=0.001 {
                            let an=port(d,candidate,refs+(i-1)*3,true);let bn=port(d,candidate,refs+i*3,true);
                            if magnitude(an)>0.001 {facing+=max(1.0-dot(unit(an),link),0.0);}
                            if magnitude(bn)>0.001 {facing+=max(1.0-dot(unit(bn),P{x:-link.x,y:-link.y}),0.0);}
                        }
                    }
                }
                if edges>=2 {total+=rp((max(length-direct,0.0)*d.ff[f]+backtrack*42.0+turns*d.ff[f+1]+facing*12.0)*d.ff[f+2]*d.ff[f+3]);}
            }
        }
    }
    let mut ordinary=0.0f32;
    for pair in 0usize..d.fi[ORDINARY_COUNT] as usize {
        let info=d.fi[ORDINARY_I] as usize+pair*3;let refs=d.fi[info] as usize;let count=d.fi[info+1] as usize;
        let mut best=f32::from_bits(0x7f800000u32);
        for j in 0usize..count {
            let t=refs+j*4;let a=cp_at(d,load_prim(d,d.fi[t] as usize,candidate),d.fi[t+1] as usize);
            let b=cp_at(d,load_prim(d,d.fi[t+2] as usize,candidate),d.fi[t+3] as usize);
            best=min(best,distance(P{x:a.x,y:a.y},P{x:b.x,y:b.y}));
        }
        if count>0 {ordinary+=min(best,30.0)*d.ff[d.fi[info+2] as usize]*0.5;}
    }
    total+ordinary
}

#[cube]
fn static_box(d:&Input,f:usize)->B {B{l:d.sf[f],r:d.sf[f+1],t:d.sf[f+2],b:d.sf[f+3]}}
#[cube]
fn corner(b:B,i:usize)->P {P{x:choose(i==0 || i==3,b.l,b.r),y:choose(i<2,b.t,b.b)}}
#[cube]
fn outline_point(d:&Input,i:usize)->P {let f=d.fi[OUTLINE_F] as usize+i*2;P{x:d.sf[f],y:d.sf[f+1]}}
#[cube]
fn on_edge(p:P,a:P,b:P)->bool {
    cross(a,b,p).abs()<0.000001 && p.x<=max(a.x,b.x)+0.000001 && p.x>=min(a.x,b.x)-0.000001 && p.y<=max(a.y,b.y)+0.000001 && p.y>=min(a.y,b.y)-0.000001
}
#[cube]
fn polygon_contains(d:&Input,p:P)->bool {
    let n=d.fi[OUTLINE_COUNT] as usize;let mut inside:bool=false;let mut boundary:bool=false;
    if n>=3 {for i in 0usize..n {
        let a=outline_point(d,i);let b=outline_point(d,(i+n-1)%n);
        if on_edge(p,a,b) {boundary=true;}
        if (a.y>p.y)!=(b.y>p.y) && p.x<((b.x-a.x)*(p.y-a.y))/(b.y-a.y)+a.x {inside=!inside;}
    }}inside || boundary
}
#[cube]
fn polygon_distance(p:P,a:P,b:P)->f32 {
    let dx=b.x-a.x;let dy=b.y-a.y;let sq=dx*dx+dy*dy;let mut distance_=distance(p,a);
    if sq>=0.000001 {let t=min(1.0,max(0.0,((p.x-a.x)*dx+(p.y-a.y)*dy)/sq));distance_=distance(p,P{x:a.x+t*dx,y:a.y+t*dy});}distance_
}
#[cube]
fn edge_crosses(a:P,b:P,c:P,e:P)->bool {
    let den=(a.x-b.x)*(c.y-e.y)-(a.y-b.y)*(c.x-e.x);let mut result=false;
    if den.abs()>=0.000001 {
        let t=((a.x-c.x)*(c.y-e.y)-(a.y-c.y)*(c.x-e.x))/den;
        let u=-((a.x-b.x)*(a.y-c.y)-(a.y-b.y)*(a.x-c.x))/den;
        result=t>=-0.000001 && t<=1.0+0.000001 && u>=-0.000001 && u<=1.0+0.000001;
    }result
}
#[cube]
fn inside_world(d:&Input,b:B)->bool {
    let f=d.fi[WORLD_F] as usize;let bounds=static_box(d,f);let cl=d.sf[f+4];let mut valid=true;
    for i in 0usize..4usize {
        let p=corner(b,i);
        if p.x<bounds.l+cl-0.000001 || p.x>bounds.r-cl+0.000001 || p.y<bounds.t+cl-0.000001 || p.y>bounds.b-cl+0.000001 || !polygon_contains(d,p) {valid=false;}
        let mut dist=f32::from_bits(0x7f800000u32);
        for j in 0usize..d.fi[OUTLINE_COUNT] as usize {
            let a=outline_point(d,j);let next=outline_point(d,(j+1)%d.fi[OUTLINE_COUNT] as usize);
            dist=min(dist,polygon_distance(p,a,next));
            if edge_crosses(p,corner(b,(i+1)%4),a,next) {valid=false;}
        }
        if cl>0.0 && dist+0.000001<cl {valid=false;}
    }valid
}
#[cube]
fn unary_hard(d:&Input,p:Prim,checkpoint:bool)->u32 {
    let mut hard=0u32;
    if d.fi[WORLD_F]!=u32::MAX && p.locked==0 {
        for ci in 0usize..p.component_count {
            let c=component(d,p,ci);let mut valid=inside_world(d,c.body);
            for j in 0usize..c.through_count {if !inside_world(d,geometry_box(d,p,c.through+j)) {valid=false;}}
            if !valid {hard+=1;}
            for oi in 0usize..d.fi[WORLD_OBSTACLE_COUNT] as usize {
                let t=d.fi[WORLD_OBSTACLE_I] as usize+oi*3;
                if d.si[t] as usize==c.id {
                    let layer=d.si[t+1];let f=d.si[t+2] as usize;let b=static_box(d,f);let cl=d.sf[f+4];let mut collides=false;
                    if layer<0 || layer==c.layer {collides=overlap(c.body,b,cl)>0.0;} else {
                        for j in 0usize..c.through_count {if overlap(geometry_box(d,p,c.through+j),b,cl)>0.0 {collides=true;}}
                    }
                    if collides {hard+=1;}
                }
            }
        }
    }
    for oi in 0usize..d.fi[OBSTACLE_COUNT] as usize {
        let b=static_box(d,d.fi[OBSTACLE_F] as usize+oi*4);let mut collides=false;
        if (d.fi[FLAGS]&2)!=0 {
            for i in 0usize..p.box_count {if overlap(geometry_box(d,p,p.boxes+i),b,d.ff[0])>0.0 {collides=true;}}
        } else {
            for i in 0usize..p.component_count {let c=component(d,p,i);
                if overlap(c.body,b,d.ff[0])>0.0 {collides=true;}
                for j in 0usize..c.through_count {if overlap(geometry_box(d,p,c.through+j),b,d.ff[0])>0.0 {collides=true;}}
            }
        }
        if collides {hard+=1;}
    }
    if d.fi[BOUNDS_F]!=u32::MAX && !(checkpoint && d.fi[WORLD_F]!=u32::MAX && p.locked!=0) {
        let b=static_box(d,d.fi[BOUNDS_F] as usize);
        if p.bbox.l<b.l || p.bbox.r>b.r || p.bbox.t<b.t || p.bbox.b>b.b {hard+=1;}
    }hard
}

#[cube]
fn choose(test: bool, a: f32, b: f32) -> f32 {
    let mut v = a;
    if !test {
        v = b;
    }
    v
}
#[cube]
fn min(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}
#[cube]
fn max(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}
#[cube]
fn distance(a: P, b: P) -> f32 {
    let x = a.x - b.x;
    let y = a.y - b.y;
    (x * x + y * y).sqrt()
}
#[cube]
fn center(b: B) -> P {
    P {
        x: rp((b.l + b.r) / 2.0),
        y: rp((b.t + b.b) / 2.0),
    }
}
#[cube]
fn overlap(a: B, b: B, c: f32) -> f32 {
    let x1 = a.r + c - b.l;
    let x2 = b.r + c - a.l;
    let y1 = a.b + c - b.t;
    let y2 = b.b + c - a.t;
    let mut value = min(min(x1, x2), min(y1, y2));
    if x1 <= 0.0 || x2 <= 0.0 || y1 <= 0.0 || y2 <= 0.0 {
        value = 0.0;
    }
    value
}
#[cube]
fn cross(a: P, b: P, c: P) -> f32 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}
#[cube]
fn load_prim(d: &Input, i: usize, candidate: usize) -> Prim {
    let mut f = i * 10 + 3;
    let mut t = d.fi[FIXED_I] as usize + i * 15;
    let moving = i == d.fi[MOVING] as usize && candidate < d.fi[COUNT] as usize;
    if moving {
        let template = d.ids[candidate * 2] as usize;
        f = template * 10;
        t = template * 15;
    }
    let mut v = Array::<f32>::new(10usize);
    let mut tags = Array::<i32>::new(15usize);
    #[unroll]
    for j in 0usize..10usize {
        if moving {
            v[j] = d.sf[f + j];
        } else {
            v[j] = d.ff[f + j];
        }
    }
    #[unroll]
    for j in 0usize..15usize {
        if moving {
            tags[j] = d.si[t + j];
        } else {
            tags[j] = d.fi[t + j] as i32;
        }
    }
    if moving {
        let dx = d.poses[candidate * 2];
        let dy = d.poses[candidate * 2 + 1];
        v[0] = rp(v[0] + dx);
        v[1] = rp(v[1] + dx);
        v[2] = rp(v[2] + dy);
        v[3] = rp(v[3] + dy);
        v[4] = rp(v[4] + dx);
        v[5] = rp(v[5] + dx);
        v[6] = rp(v[6] + dy);
        v[7] = rp(v[7] + dy);
        v[8] = dx;
        v[9] = dy;
    }
    Prim {
        bbox: B {
            l: v[0],
            r: v[1],
            t: v[2],
            b: v[3],
        },
        body: B {
            l: v[4],
            r: v[5],
            t: v[6],
            b: v[7],
        },
        dx: v[8],
        dy: v[9],
        cp: tags[0] as usize,
        nc: tags[1] as usize,
        pad: tags[2] as usize,
        np: tags[3] as usize,
        component: tags[4] as usize,
        layer: tags[5],
        pins: tags[6] as u32,
        role: tags[7] as u32,
        power: tags[8] as u32,
        components: tags[9] as usize, component_count: tags[10] as usize, boxes: tags[11] as usize, box_count: tags[12] as usize, locked: tags[13] as u32,ports:tags[14] as usize,
    }
}
#[cube]
fn cp_at(d: &Input, p: Prim, j: usize) -> Cp {
    let f = d.fi[CP_F] as usize + (p.cp + j) * 2;
    let t = d.fi[CP_I] as usize + (p.cp + j) * 4;
    Cp {
        x: rp(d.sf[f] + p.dx),
        y: rp(d.sf[f + 1] + p.dy),
        net: d.si[t],
        owner: d.si[t + 1],
        layer: d.si[t + 2],
    }
}
#[cube]
fn ref_cp(d: &Input, j: usize, candidate: usize) -> Cp {
    let r = d.fi[REF_I] as usize + j * 2;
    cp_at(
        d,
        load_prim(d, d.fi[r] as usize, candidate),
        d.fi[r + 1] as usize,
    )
}
#[cube]
fn ep(d: &Input, ri: usize, side: usize, candidate: usize) -> P {
    let t = d.fi[REL_I] as usize + ri * 10 + side * 3;
    let mut result=P{x:0.0,y:0.0};
    if d.fi[t] as i32 == -2 {
        let f=3+d.fi[N] as usize*10+ri*11+7+side*2;
        result.x=d.ff[f];result.y=d.ff[f+1];
    } else {
    let p = load_prim(d, d.fi[t] as usize, candidate);
    let count = d.fi[t + 2] as usize;
    let mid=center(p.bbox);result.x=mid.x;result.y=mid.y;
    if count > 0 {
        let mut x = 0.0f32;
        let mut y = 0.0f32;
        for j in 0usize..count {
            let q = cp_at(
                d,
                p,
                d.fi[d.fi[EP_I] as usize + d.fi[t + 1] as usize + j] as usize,
            );
            x += q.x;
            y += q.y;
        }
        result.x = x / count as f32;
        result.y = y / count as f32;
    }
    }
    result
}
#[cube]
fn insert(points: &mut Array<f32>, n: usize, p: P) {
    let mut k = n;
    while k > 0 {
        let x = points[(k - 1) * 2];
        let y = points[(k - 1) * 2 + 1];
        if !(x > p.x || (x == p.x && y > p.y)) {
            break;
        }
        points[k * 2] = x;
        points[k * 2 + 1] = y;
        k -= 1;
    }
    points[k * 2] = p.x;
    points[k * 2 + 1] = p.y;
}
#[cube]
fn point(points: &Array<f32>, i: usize) -> P {
    P {
        x: points[i * 2],
        y: points[i * 2 + 1],
    }
}

#[cube]
fn make_tree(d: &Input, ni: usize, candidate: usize, out: &mut Array<f32>, tags: &mut Array<i32>, #[comptime] net_capacity:usize) {
    let net = d.fi[NET_I] as usize + ni * 8;
    let n = d.fi[net + 1] as usize;
    let mut points = Array::<f32>::new(net_capacity*2);
    let mut meta = Array::<i32>::new(net_capacity*2);
    let mut used = Array::<u32>::new(net_capacity);
    let mut distances = Array::<f32>::new(comptime![if net_capacity<=32 {net_capacity*net_capacity} else {1}]);
    for i in 0usize..n {
        let p = ref_cp(d, d.fi[net] as usize + i, candidate);
        points[i * 2] = p.x;
        points[i * 2 + 1] = p.y;
        meta[i * 2] = p.layer;
        meta[i * 2 + 1] = p.owner;
        used[i] = 0;
    }
    used[0] = 1;
    if comptime![net_capacity<=32] {
    for i in 0usize..n {
        for j in i + 1..n {
            let v = distance(point(&points, i), point(&points, j));
            distances[i * net_capacity + j] = v;
            distances[j * net_capacity + i] = v;
        }
    }
    }
    for edge in 0usize..n - 1 {
        let mut found = false;
        let mut bf = 0usize;
        let mut bt = 0usize;
        let mut best = 0.0f32;
        for f in 0usize..n {
            if used[f] != 0 {
                for t in 0usize..n {
                    if used[t] == 0 {
                        let mut v=0.0f32;
                        if comptime![net_capacity<=32] {v=distances[f*net_capacity+t];} else {v=distance(point(&points,f),point(&points,t));}
                        if !found
                            || v < best - 0.001
                            || ((v - best).abs() <= 0.001 && (f < bf || (f == bf && t < bt)))
                        {
                            found = true;
                            bf = f;
                            bt = t;
                            best = v;
                        }
                    }
                }
            }
        }
        used[bt] = 1;
        out[edge * 4] = points[bf * 2];
        out[edge * 4 + 1] = points[bf * 2 + 1];
        out[edge * 4 + 2] = points[bt * 2];
        out[edge * 4 + 3] = points[bt * 2 + 1];
        let a = meta[bf * 2];
        let b = meta[bt * 2];
        let mut layer = a * 0i32 - 1i32;
        if a == b {
            layer = a;
        } else if a < 0 {
            layer = b;
        } else if b < 0 {
            layer = a;
        }
        let mut owner = a * 0i32 - 1i32;
        if meta[bf * 2 + 1] == meta[bt * 2 + 1] {
            owner = meta[bf * 2 + 1];
        }
        tags[edge * 4] = d.fi[net + 4] as i32;
        tags[edge * 4 + 1] = layer;
        tags[edge * 4 + 2] = owner;
        tags[edge * 4 + 3] = d.fi[net + 5] as i32;
    }
}
#[cube]
fn hit(ax: f32, ay: f32, bx: f32, by: f32, p: B) -> bool {
    let mut valid =
        !(p.r < min(ax, bx) || p.l > max(ax, bx) || p.b < min(ay, by) || p.t > max(ay, by));
    let mut lo = 0.0f32;
    let mut hi = 1.0f32;
    for axis in 0usize..2usize {
        let start = choose(axis == 0, ax, ay);
        let delta = choose(axis == 0, bx - ax, by - ay);
        let lower = choose(axis == 0, p.l, p.t);
        let upper = choose(axis == 0, p.r, p.b);
        if valid {
            if delta.abs() < 1e-9 {
                if start < lower || start > upper {
                    valid = false;
                }
            } else {
                let t1 = (lower - start) / delta;
                let t2 = (upper - start) / delta;
                lo = max(lo, min(t1, t2));
                hi = min(hi, max(t1, t2));
                if lo > hi {
                    valid = false;
                }
            }
        }
    }
    valid
}
#[cube]
fn pad_at(d: &Input, p: Prim, j: usize) -> B {
    let f = d.fi[PAD_F] as usize + (p.pad + j) * 4;
    B {
        l: rp(d.sf[f] + p.dx),
        r: rp(d.sf[f + 1] + p.dx),
        t: rp(d.sf[f + 2] + p.dy),
        b: rp(d.sf[f + 3] + p.dy),
    }
}
#[cube(launch_unchecked)]
pub fn frame_pads(d: &Input, out: &mut Array<f32>) {
    let i = CUBE_POS as usize;
    if i != d.fi[MOVING] as usize {
        let p = load_prim(d, i, d.fi[COUNT] as usize);
        let mut j = UNIT_POS as usize;
        while j < p.np {
            let b = pad_at(d, p, j);
            let f = (p.pad + j) * 4;
            out[f] = b.l;
            out[f + 1] = b.r;
            out[f + 2] = b.t;
            out[f + 3] = b.b;
            j += 128;
        }
    }
}
#[cube(launch_unchecked)]
pub fn frame_mst(d: &Input, segments: &mut Array<f32>, tags: &mut Array<i32>, #[comptime] net_capacity:usize) {
    let ni = ABSOLUTE_POS as usize;
    if ni < d.fi[NETS] as usize {
        let net = d.fi[NET_I] as usize + ni * 8;
        let count = d.fi[net + 1] as usize;
        if d.fi[net + 5] != 0 && count >= 2 {
            let mut s = Array::<f32>::new(net_capacity*4);
            let mut t = Array::<i32>::new(net_capacity*4);
            make_tree(d, ni, d.fi[COUNT] as usize, &mut s, &mut t,net_capacity);
            for j in 0usize..(count - 1) * 4 {
                let k = d.fi[net + 6] as usize * 4 + j;
                segments[k] = s[j];
                tags[k] = t[j];
            }
        }
    }
}
#[cube(launch_unchecked)]
pub fn frame_hits(
    d: &Input,
    segments: &Array<f32>,
    tags: &Array<i32>,
    pads: &Array<f32>,
    costs: &mut Array<i32>,
) {
    let si = CUBE_POS as usize;
    let s = si * 4;
    let lane = UNIT_POS as usize;
    let mut total = 0i32;
    for i in 0usize..d.fi[N] as usize {
        if i != d.fi[MOVING] as usize {
            let p = load_prim(d, i, d.fi[COUNT] as usize);
            let mut j = lane;
            while j < p.np {
                let t = d.fi[PAD_I] as usize + (p.pad + j) * 3;
                let f = (p.pad + j) * 4;
                if tags[s] != d.si[t]
                    && !(tags[s + 1] >= 0 && d.si[t + 1] >= 0 && tags[s + 1] != d.si[t + 1])
                    && !(tags[s + 2] >= 0 && tags[s + 2] == d.si[t + 2])
                {
                    if hit(
                        segments[s],
                        segments[s + 1],
                        segments[s + 2],
                        segments[s + 3],
                        B {
                            l: pads[f],
                            r: pads[f + 1],
                            t: pads[f + 2],
                            b: pads[f + 3],
                        },
                    ) {
                        total += tags[s + 3];
                    }
                }
                j += 128;
            }
        }
    }
    let mut partial = SharedMemory::<i32>::new(128usize);
    partial[lane] = total;
    sync_cube();
    #[unroll]
    for exponent in 1usize..8usize {
        let step = 128usize >> exponent;
        if lane < step {
            partial[lane] += partial[lane + step];
        }
        sync_cube();
    }
    if lane == 0 {
        costs[si] = partial[0];
    }
}
#[cube(launch_unchecked)]
pub fn full(
    d: &Input,
    static_segments: &Array<f32>,
    static_tags: &Array<i32>,
    fixed_costs: &Array<i32>,
    pads: &Array<f32>,
    scores: &mut Array<f32>,
    tags: &mut Array<u32>,
    mask: &Array<u32>,
    use_mask: u32,
    #[comptime] pad_capacity: usize,
    #[comptime] segment_capacity: usize,
    #[comptime] net_capacity: usize,
) {
    let candidate = CUBE_POS as usize;
    let lane = UNIT_POS as usize;
    if use_mask == 0 || mask[candidate] != 0 {
        let mut segments = SharedMemory::<f32>::new(segment_capacity*4);
        let mut meta = SharedMemory::<i32>::new(segment_capacity*4);
        let mut cached = SharedMemory::<i32>::new(segment_capacity);
        // Transform each moving pad once per candidate, shared by all segments.
        let mut moving_pads = SharedMemory::<f32>::new(pad_capacity * 4);
        let moving = load_prim(d, d.fi[MOVING] as usize, candidate);
        let mut pj = lane;
        while pj < moving.np {
            let b = pad_at(d, moving, pj);
            let f = pj * 4;
            moving_pads[f] = b.l;
            moving_pads[f + 1] = b.r;
            moving_pads[f + 2] = b.t;
            moving_pads[f + 3] = b.b;
            pj += 128;
        }
        let mut ni = lane;
        while ni < d.fi[NETS] as usize {
            let net = d.fi[NET_I] as usize + ni * 8;
            let count = d.fi[net + 1] as usize;
            if d.fi[net + 5] != 0 && count >= 2 {
                let mut local_s = Array::<f32>::new(net_capacity*4);
                let mut local_t = Array::<i32>::new(net_capacity*4);
                let changed = (d.fi[net + 7] & 2) != 0;
                if changed {
                    make_tree(d, ni, candidate, &mut local_s, &mut local_t,net_capacity);
                }
                for j in 0usize..count - 1 {
                    let k = d.fi[net + 6] as usize + j;
                    let s = k * 4;
                    if changed {
                        for v in 0usize..4usize {
                            segments[s + v] = local_s[j * 4 + v];
                            meta[s + v] = local_t[j * 4 + v];
                        }
                        let mut cost = d.fi[net] as i32 * 0i32 - 1i32;
                        for other in 0usize..count - 1 {
                            let target = d.fi[net + 6] as usize + other;
                            let mut same = true;
                            for v in 0usize..4usize {
                                if local_s[j * 4 + v] != static_segments[target * 4 + v]
                                    || local_t[j * 4 + v] != static_tags[target * 4 + v]
                                {
                                    same = false;
                                }
                            }
                            if same {
                                cost = fixed_costs[target];
                                break;
                            }
                        }
                        cached[k] = cost;
                    } else {
                        for v in 0usize..4usize {
                            segments[s + v] = static_segments[s + v];
                            meta[s + v] = static_tags[s + v];
                        }
                        cached[k] = fixed_costs[k];
                    }
                }
            }
            ni += 128;
        }
        sync_cube();
        let mut total = 0i32;
        for si in 0usize..d.fi[SEGMENTS] as usize {
            let s = si * 4;
            if cached[si] >= 0 && lane == 0 {
                total += cached[si];
            }
            for i in 0usize..d.fi[N] as usize {
                if cached[si] < 0 || i == d.fi[MOVING] as usize {
                    let p = load_prim(d, i, candidate);
                    let mut j = lane;
                    while j < p.np {
                        let t = d.fi[PAD_I] as usize + (p.pad + j) * 3;
                        if meta[s] != d.si[t]
                            && !(meta[s + 1] >= 0 && d.si[t + 1] >= 0 && meta[s + 1] != d.si[t + 1])
                            && !(meta[s + 2] >= 0 && meta[s + 2] == d.si[t + 2])
                        {
                            let mut b = B {
                                l: 0.0,
                                r: 0.0,
                                t: 0.0,
                                b: 0.0,
                            };
                            if i == d.fi[MOVING] as usize {
                                let f = j * 4;
                                b.l = moving_pads[f];
                                b.r = moving_pads[f + 1];
                                b.t = moving_pads[f + 2];
                                b.b = moving_pads[f + 3];
                            }
                            if i != d.fi[MOVING] as usize {
                                let f = (p.pad + j) * 4;
                                b.l = pads[f];
                                b.r = pads[f + 1];
                                b.t = pads[f + 2];
                                b.b = pads[f + 3];
                            }
                            if hit(
                                segments[s],
                                segments[s + 1],
                                segments[s + 2],
                                segments[s + 3],
                                b,
                            ) {
                                total += meta[s + 3];
                            }
                        }
                        j += 128;
                    }
                }
            }
        }
        let mut partial = SharedMemory::<i32>::new(128usize);
        partial[lane] = total;
        sync_cube();
        #[unroll]
        for exponent in 1usize..8usize {
            let step = 128usize >> exponent;
            if lane < step {
                partial[lane] += partial[lane + step];
            }
            sync_cube();
        }
        if lane == 0 {
            scores[candidate] += partial[0] as f32 * 45.0;
            scores[candidate] += topology(d,candidate)*choose(d.fi[HIGH]!=0,2.5,4.0);
            tags[candidate * 3 + 1] = 1;
        }
    }
}

#[cube]
fn before(ah: u32, a: f32, ai: usize, bh: u32, b: f32, bi: usize) -> bool {
    ah < bh || (ah == bh && (a < b || (a == b && ai < bi)))
}
#[cube(launch_unchecked)]
pub fn clear_best(best: &mut Array<u32>) {
    best[UNIT_POS as usize] = u32::MAX;
}
#[cube(launch_unchecked)]
pub fn rank(
    d: &Input,
    scores: &Array<f32>,
    tags: &Array<u32>,
    best: &mut Array<u32>,
    mask: &mut Array<u32>,
    completed: u32,
    seeds: u32,
) {
    let i = ABSOLUTE_POS as usize;
    if i < d.fi[COUNT] as usize {
        if seeds != 0 {
            mask[i] = 0;
        }
        if completed == 0 || tags[i * 3 + 1] != 0 {
            let diverse = d.fi[DIVERSE] != 0;
            let mut cap: u32 = 16u32;
            if diverse {
                cap = 64u32;
            }
            let mut gr = 0u32;
            let mut br = 0u32;
            let bucket = tags[i * 3 + 2];
            for j in 0usize..d.fi[COUNT] as usize {
                if completed == 0 || tags[j * 3 + 1] != 0 {
                    if before(tags[j * 3], scores[j], j, tags[i * 3], scores[i], i) {
                        gr += 1;
                        if tags[j * 3 + 2] == bucket {
                            br += 1;
                        }
                    }
                    if gr >= cap && (!diverse || br >= 4) {
                        break;
                    }
                }
            }
            if gr < cap {
                best[gr as usize] = i as u32;
            }
            if diverse && br < 4 {
                best[(64 + bucket * 4 + br) as usize] = i as u32;
            }
            if seeds != 0 && (gr < cap || (diverse && br < 4)) {
                mask[i] = 1;
            }
        }
    }
}
#[cube(launch_unchecked)]
pub fn prune(
    d: &Input,
    scores: &Array<f32>,
    tags: &Array<u32>,
    best: &Array<u32>,
    mask: &mut Array<u32>,
    operations: u32,
) {
    let i = ABSOLUTE_POS as usize;
    if i < d.fi[COUNT] as usize {
        mask[i] = 0;
        if tags[i * 3 + 1] == 0 {
            let mut last = 15usize;
            if d.fi[DIVERSE] != 0 {
                last = 63;
            }
            let mut keep = false;
            let nu = operations as f32 * 0.000000059604644775390625;
            let mut lower = f32::from_bits(0xff800000u32);
            if nu < 0.25 {
                // Factor two covers rounding of the gamma expression itself,
                // including the Vulkan division accuracy bound. Unknown or
                // large error budgets keep the candidate instead of pruning.
                let gamma = 2.0 * nu / (1.0 - nu);
                lower = scores[i] - gamma * scores[i].abs();
            }
            for k in 0usize..2usize {
                if k == 0 || d.fi[DIVERSE] != 0 {
                    let mut slot = last;
                    if k == 1 {
                        slot = (64 + tags[i * 3 + 2] * 4 + 3) as usize;
                    }
                    let target = best[slot];
                    if target == u32::MAX {
                        keep = true;
                    } else {
                        let j = target as usize;
                        if before(tags[i * 3], lower, i, tags[j * 3], scores[j], j) {
                            keep = true;
                        }
                    }
                }
            }
            if keep {
                mask[i] = 1;
            }
        }
    }
}
#[cube(launch_unchecked)]
pub fn compact(
    d: &Input,
    scores: &Array<f32>,
    tags: &Array<u32>,
    best: &Array<u32>,
    out_i: &mut Array<u32>,
    out_f: &mut Array<f32>,
) {
    // Only the already bounded (<=128) shortlist is compacted by one lane.
    let mut chosen = Array::<u32>::new(64usize);
    let mut n = 0usize;
    let mut cap = 16usize;
    if d.fi[DIVERSE] != 0 {
        cap = 64;
    }
    if d.fi[DIVERSE] != 0 {
        for slot in 64usize..128usize {
            let index = best[slot];
            if index != u32::MAX {
                chosen[n] = index;
                n += 1;
            }
        }
    }
    for slot in 0usize..cap {
        if n < cap {
            let index = best[slot];
            if index != u32::MAX {
                let mut found = false;
                for j in 0usize..n {
                    if chosen[j] == index {
                        found = true;
                    }
                }
                if !found {
                    chosen[n] = index;
                    n += 1;
                }
            }
        }
    }
    for i in 1usize..n {
        let index = chosen[i] as usize;
        let mut j = i;
        while j > 0 {
            let other = chosen[j - 1] as usize;
            if !before(
                tags[index * 3],
                scores[index],
                index,
                tags[other * 3],
                scores[other],
                other,
            ) {
                break;
            }
            chosen[j] = chosen[j - 1];
            j -= 1;
        }
        chosen[j] = index as u32;
    }
    out_i[129] = 0;
    for i in 0usize..d.fi[COUNT] as usize {
        let s = scores[i];
        if s != s || s.abs() > 3.4028234663852886e38 {
            out_i[129] = 1;
        }
    }
    out_i[0] = n as u32;
    for i in 0usize..n {
        let index = chosen[i] as usize;
        out_i[1 + i * 2] = index as u32;
        out_i[2 + i * 2] = tags[index * 3];
        out_f[i] = scores[index];
    }
}
#[cube]
fn convex(points: &mut Array<f32>, length: usize, hull: &mut Array<f32>) -> usize {
    let mut n = 0usize;
    for i in 0usize..length {
        let p = point(points, i);
        let mut keep = true;
        if n > 0 {
            let prev = point(points, n - 1);
            keep = p.x != prev.x || p.y != prev.y;
        }
        if keep {
            points[n * 2] = p.x;
            points[n * 2 + 1] = p.y;
            n += 1;
        }
    }
    let mut count = 0usize;
    if n < 3 {
        for i in 0usize..n {
            hull[i * 2] = points[i * 2];
            hull[i * 2 + 1] = points[i * 2 + 1];
        }
        count = n;
    } else {
        for i in 0usize..n {
            while count >= 2 {
                if cross(
                    point(hull, count - 2),
                    point(hull, count - 1),
                    point(points, i),
                ) > 0.0
                {
                    break;
                }
                count -= 1;
            }
            hull[count * 2] = points[i * 2];
            hull[count * 2 + 1] = points[i * 2 + 1];
            count += 1;
        }
        count -= 1;
        let lower = count;
        let mut i = n;
        while i > 0 {
            i -= 1;
            while count - lower >= 2 {
                if cross(
                    point(hull, count - 2),
                    point(hull, count - 1),
                    point(points, i),
                ) > 0.0
                {
                    break;
                }
                count -= 1;
            }
            hull[count * 2] = points[i * 2];
            hull[count * 2 + 1] = points[i * 2 + 1];
            count += 1;
        }
        count -= 1;
    }
    count
}
#[cube(launch_unchecked)]
pub fn frame_hull(d: &Input, out: &mut Array<f32>, count_out: &mut Array<u32>, #[comptime] capacity: usize) {
    let mut points = Array::<f32>::new(capacity * 2);
    let mut hull = Array::<f32>::new(capacity * 4);
    let mut n = 0usize;
    for i in 0usize..d.fi[N] as usize {
        if i != d.fi[MOVING] as usize {
            let p = load_prim(d, i, d.fi[COUNT] as usize);
            for bi in 0usize..p.box_count {
            let b = geometry_box(d, p, p.boxes + bi);
            insert(
                &mut points,
                n,
                P {
                    x: rp(b.l),
                    y: rp(b.t),
                },
            );
            n += 1;
            insert(
                &mut points,
                n,
                P {
                    x: rp(b.r),
                    y: rp(b.t),
                },
            );
            n += 1;
            insert(
                &mut points,
                n,
                P {
                    x: rp(b.r),
                    y: rp(b.b),
                },
            );
            n += 1;
            insert(
                &mut points,
                n,
                P {
                    x: rp(b.l),
                    y: rp(b.b),
                },
            );
            n += 1;
            }
        }
    }
    let count = convex(&mut points, n, &mut hull);
    for i in 0usize..count {
        insert(&mut points, i, point(&hull, i));
    }
    count_out[0] = count as u32;
    for i in 0usize..count * 2 {
        out[i] = points[i];
    }
}

#[cube(launch_unchecked)]
pub fn cheap(
    d: &Input,
    fixed_hull: &Array<f32>,
    hull_count: &Array<u32>,
    scores: &mut Array<f32>,
    tags: &mut Array<u32>,
    #[comptime] capacity: usize,
) {
    let candidate = ABSOLUTE_POS as usize;
    if candidate < d.fi[COUNT] as usize {
        let nprim = d.fi[N] as usize;
        let moving = d.fi[MOVING] as usize;
        let high = d.fi[HIGH] != 0;
        let mut bbox = load_prim(d, 0usize, candidate).bbox;
        for i in 1usize..nprim {
            let b = load_prim(d, i, candidate).bbox;
            bbox.l = min(bbox.l, b.l);
            bbox.r = max(bbox.r, b.r);
            bbox.t = min(bbox.t, b.t);
            bbox.b = max(bbox.b, b.b);
        }
        let w = bbox.r - bbox.l;
        let h = bbox.b - bbox.t;
        let mut points = Array::<f32>::new(capacity * 2);
        let mut hull = Array::<f32>::new(capacity * 4);
        let mut n = hull_count[0] as usize;
        for i in 0usize..n * 2 {
            points[i] = fixed_hull[i];
        }
        let moving_prim = load_prim(d, moving, candidate);
        for bi in 0usize..moving_prim.box_count {
        let b = geometry_box(d, moving_prim, moving_prim.boxes + bi);
        insert(
            &mut points,
            n,
            P {
                x: rp(b.l),
                y: rp(b.t),
            },
        );
        n += 1;
        insert(
            &mut points,
            n,
            P {
                x: rp(b.r),
                y: rp(b.t),
            },
        );
        n += 1;
        insert(
            &mut points,
            n,
            P {
                x: rp(b.r),
                y: rp(b.b),
            },
        );
        n += 1;
        insert(
            &mut points,
            n,
            P {
                x: rp(b.l),
                y: rp(b.b),
            },
        );
        n += 1;
        }
        let count = convex(&mut points, n, &mut hull);
        let mut area = 0.0f32;
        let mut perimeter = 0.0f32;
        if count < 3 {
            area = w * h;
            perimeter = w + h;
        } else {
            for i in 0usize..count {
                let a = point(&hull, i);
                let b = point(&hull, (i + 1) % count);
                area += a.x * b.y - b.x * a.y;
                perimeter += distance(a, b);
            }
            area = area.abs() / 2.0;
        }
        let mut score = w * h * (choose(high, 4.2, 3.2))
            + (w + h) * (choose(high, 5.4, 2.5))
            + area * (choose(high, 14.4, 10.0)) * (choose(d.fi[REDUCED] != 0, 0.25, 1.0))
            + perimeter * (choose(high, 12.0, 1.5));
        let mut excess = 0.0f32;
        if w > 0.0 && h > 0.0 {
            excess = max(max(w / h, h / w) - 5.0, 0.0);
        }
        score += excess * excess * max(min(w, h), 1.0) * 220.0 * (choose(high, 1.4, 1.0));
        if d.fi[SMOOTH] != 0 {
            let ratio = max(w, h) / max(min(w, h), 0.1);
            let x = max(ratio - 2.0, 0.0);
            score += x * x * min(w, h) * 12.0;
        }
        let mut ov = 0.0f32;
        let mut hard = 0u32;
        for i in 0usize..nprim {
            let a = load_prim(d, i, candidate);
            for j in i + 1..nprim {
                let b = load_prim(d, j, candidate);
                if can_conflict(d,a,b) {
                    let v=primitive_overlap(d,a,b);
                    if v>0.0 {ov+=choose((d.fi[FLAGS]&1)!=0,250000.0,10000000.0)+v*choose((d.fi[FLAGS]&1)!=0,25000.0,100000.0);}
                    let mut depth=component_overlap(d,a,b);if (d.fi[FLAGS]&2)!=0 {depth=v;}
                    if depth>0.0 {hard+=1;}
                }
            }
        }
        let mut bound_penalty=0.0f32;
        for i in 0usize..nprim {
            let p=load_prim(d,i,candidate);hard+=unary_hard(d,p,true);
            for oi in 0usize..d.fi[OBSTACLE_COUNT] as usize {let b=static_box(d,d.fi[OBSTACLE_F] as usize+oi*4);
                for bi in 0usize..p.box_count {let depth=overlap(geometry_box(d,p,p.boxes+bi),b,d.ff[0]);if depth>0.0 {ov+=10000000.0+depth*100000.0;}}
            }
            if d.fi[BOUNDS_F]!=u32::MAX {
                let b=static_box(d,d.fi[BOUNDS_F] as usize);let overflow=max(b.l-p.bbox.l,0.0)+max(p.bbox.r-b.r,0.0)+max(b.t-p.bbox.t,0.0)+max(p.bbox.b-b.b,0.0);
                if overflow>0.0 {bound_penalty+=10000000.0+overflow*500000.0;}
            }
        }
        score += ov;score+=bound_penalty;
        let mut dense = 0.0f32;
        let mut placed_components=0usize;
        for i in 0usize..nprim {placed_components+=load_prim(d,i,candidate).component_count;}
        if d.fi[DENSE]!=0 && placed_components<=20 {
            for pi in 0usize..nprim {let ap=load_prim(d,pi,candidate);
            for ai in 0usize..ap.component_count {let a=component(d,ap,ai);
                if a.role!=4 && a.pins>=8 {
                    let halo=min(1.5,max(0.25,(a.pins as f32-8.0)*0.025));
                    let b=B{l:a.body.l-halo,r:a.body.r+halo,t:a.body.t-halo,b:a.body.b+halo};
                    let sw=choose(a.role==1,1.45,1.0)*choose(a.pins>=64,1.35,choose(a.pins>=32,1.2,1.0));
                    for pj in 0usize..nprim {let bp=load_prim(d,pj,candidate);
                    for bi in 0usize..bp.component_count {let p=component(d,bp,bi);
                        if a.id!=p.id && a.layer==p.layer {
                            let depth=overlap(b,p.body,0.0);
                            if depth>0.0 {let nw=choose(p.role==2,0.55,choose(p.power!=0,1.15,choose(p.role==3,0.7,1.0)));dense+=(depth*depth*1500.0+depth*600.0)*sw*nw;}
                        }
                    }}
                }
            }}
        }
        score += dense * (choose(high, 0.45, 1.0));
        score += power_yield(d,candidate)*choose(high,0.3,1.0);
        let mut relation = 0.0f32;
        let mut exposure = 0.0f32;
        let mut facing = 0.0f32;
        for i in 0usize..d.fi[RELS] as usize {
            let t = d.fi[REL_I] as usize + i * 10;
            let f = 3 + nprim * 10 + i * 11;
            let from = d.fi[t] as i32;
            let to = d.fi[t + 3] as i32;
            let weight = d.ff[f];
            if (from >= 0 || from == -2) && (to >= 0 || to == -2) && from != to {
                let a = ep(d, i, 0usize, candidate);
                let mut b = ep(d, i, 1usize, candidate);
                if d.fi[t + 7] != 0 {
                    b.x = rp(b.x + d.ff[f + 3]);
                    b.y = rp(b.y + d.ff[f + 4]);
                }
                let dist = distance(a, b);
                let mult = choose(d.fi[t + 6] != 0, 5.0, 1.0);
                let mut limit = 0.0f32;
                if d.ff[f + 2] >= 0.0 {
                    let x = max(dist - d.ff[f + 2], 0.0);
                    limit += (x * x * 1500.0 + x * 120.0) * weight * mult;
                }
                if d.ff[f + 1] >= 0.0 {
                    let x = max(d.ff[f + 1] - dist, 0.0);
                    limit += (x * x * 800.0 + x * 80.0) * weight * mult;
                }
                let mut side = 0.0f32;
                if d.fi[t + 8] != 0 {
                    let mut dx = a.x - b.x;
                    let mut dy = a.y - b.y;
                    if dist <= 1e-6 {
                        dx = 0.0;
                        dy = 0.0;
                    } else {
                        dx /= dist;
                        dy /= dist;
                    }
                    side = max(1.0 - (dx * d.ff[f + 5] + dy * d.ff[f + 6]), 0.0) * weight * 120.0;
                }
                relation += dist * weight + min(limit, 250000.0) + side;
                if (d.fi[t+9]&2)!=0 {
                    let original_to=ep(d,i,1usize,candidate);
                    if from>=0 {facing+=endpoint_facing(a,original_to,load_prim(d,from as usize,candidate).bbox)*weight*2.5;}
                    if to>=0 {facing+=endpoint_facing(original_to,a,load_prim(d,to as usize,candidate).bbox)*weight*1.2;}
                }
            }
            if ((d.fi[t + 9]&1) != 0 || (d.fi[FLAGS]&4)==0) && (d.fi[t+9]&4)==0 && (from == -1 || to == -1) && (from >= 0 || from == -2 || to >= 0 || to == -2) {
                // Keep a runtime scalar: CubeCL 0.10 treats a value-producing
                // branch of usize literals as compile-time and selects side 1.
                // That can pass the missing endpoint (-1) to load_prim.
                let mut side = 0usize;
                if !(from >= 0 || from == -2) { side = 1; }
                let source = ep(d, i, side, candidate);
                let primitive = if from >= 0 || from == -2 { from } else { to } as usize;
                let mut owner_layer=0i32;
                owner_layer-=1;
                if primitive<nprim {owner_layer=load_prim(d,primitive,candidate).layer;}
                let cl = d.ff[0];
                let mut b0 = 0.0f32;
                let mut b1 = 0.0f32;
                let mut b2 = 0.0f32;
                let mut b3 = 0.0f32;
                for j in 0usize..nprim {
                    let p = load_prim(d, j, candidate);
                    if j != primitive {
                    for ci in 0usize..p.component_count {
                        let c=component(d,p,ci);
                        let mut count=c.through_count; if c.layer==owner_layer || owner_layer<0 {count=1;}
                        for bi in 0usize..count {
                        let mut b=c.body;
                        if c.layer!=owner_layer && owner_layer>=0 {let other=geometry_box(d,p,c.through+bi);b.l=other.l;b.r=other.r;b.t=other.t;b.b=other.b;}
                        if source.y >= b.t - cl && source.y <= b.b + cl {
                            b0 += max(b.r + cl - max(source.x, b.l - cl), 0.0);
                            b1 += max(min(source.x, b.r + cl) - b.l + cl, 0.0);
                        }
                        if source.x >= b.l - cl && source.x <= b.r + cl {
                            b2 += max(b.b + cl - max(source.y, b.t - cl), 0.0);
                            b3 += max(min(source.y, b.b + cl) - b.t + cl, 0.0);
                        }
                        }
                    }
                    }
                }
                let mut blocked=min(min(b0,b1),min(b2,b3));
                if (d.fi[FLAGS]&4)==0 {blocked=min(min((source.x-bbox.l).abs(),(bbox.r-source.x).abs()),min((source.y-bbox.t).abs(),(bbox.b-source.y).abs()));}
                exposure += blocked * weight * (choose(placed_components < 5, 1.25, 5.0));
            }
        }
        score += relation * (choose(high, 0.45, 1.0));
        score += exposure * (choose(high, 0.35, 1.0));
        score += facing*choose(high,0.55,1.0);
        let small = ((d.fi[FLAGS]&8)!=0 && d.fi[SMALL]!=0) || ((d.fi[FLAGS]&8)==0 && placed_components>0 && placed_components<5);
        let mut signal = 0.0f32;
        let mut ground = 0.0f32;
        for ni in 0usize..d.fi[NETS] as usize {
            let net = d.fi[NET_I] as usize + ni * 8;
            let count = d.fi[net + 1] as usize;
            let g = d.fi[net + 3] != 0;
            if d.fi[net + 2] >= 2 && !(g && !small && count > 3) {
                let mut l = f32::from_bits(0x7f800000u32);
                let mut r = score * 0.0 - f32::from_bits(0x7f800000u32);
                let mut t = f32::from_bits(0x7f800000u32);
                let mut b = score * 0.0 - f32::from_bits(0x7f800000u32);
                for j in 0usize..count {
                    let q = ref_cp(d, d.fi[net] as usize + j, candidate);
                    l = min(l, q.x);
                    r = max(r, q.x);
                    t = min(t, q.y);
                    b = max(b, q.y);
                }
                let spread = r - l + b - t;
                if g {
                    if small || spread <= 4.0 {
                        ground += spread;
                    }
                } else {
                    signal += spread;
                }
            }
        }
        let nw = choose(high, 0.65, 1.0);
        score += signal * (choose(small, 18.0, 4.0)) * nw;
        score += ground * (choose(small, 2.5, 0.15)) * nw;
        let targets=d.fi[TARGET_F] as usize;let mut target=0.0f32;
        if d.sf[targets]>=0.0 {let excess=max(w-d.sf[targets],0.0);target+=excess*excess*20000.0+excess*2000.0;}
        if d.sf[targets+1]>=0.0 {let excess=max(h-d.sf[targets+1],0.0);target+=excess*excess*20000.0+excess*2000.0;}
        score+=target*choose(high,1.2,1.0);
        let mut long_penalty=0.0f32;
        for ni in 0usize..d.fi[NETS] as usize {
            let net=d.fi[NET_I] as usize+ni*8;
            if (d.fi[net+7]&4)!=0 && d.fi[net+1]==2 && d.fi[net+2]==2 {
                let ri=d.fi[net] as usize;
                let pa=load_prim(d,d.fi[d.fi[REF_I] as usize+ri*2] as usize,candidate);
                let pb=load_prim(d,d.fi[d.fi[REF_I] as usize+(ri+1)*2] as usize,candidate);
                let a=ref_cp(d,ri,candidate);let b=ref_cp(d,ri+1,candidate);
                let scale=max(1.0,(max(pa.bbox.r-pa.bbox.l,pa.bbox.b-pa.bbox.t)+max(pb.bbox.r-pb.bbox.l,pb.bbox.b-pb.bbox.t))*0.5+d.ff[0]);
                let excess=max(0.0,distance(P{x:a.x,y:a.y},P{x:b.x,y:b.y})-scale);
                long_penalty+=excess*excess/scale*12.0;
            }
        }
        score+=long_penalty;
        scores[candidate] = score;
        tags[candidate * 3] = hard;
        tags[candidate * 3 + 1] = 0;
        let mid = center(load_prim(d, moving, candidate).bbox);
        let mut bucket = (d.ids[candidate * 2] % 4) * 4;
        if mid.x >= d.ff[1] {
            bucket += 2;
        }
        if mid.y >= d.ff[2] {
            bucket += 1;
        }
        tags[candidate * 3 + 2] = bucket;
    }
}

// Offline failure diagnostics only; never dispatched in normal scoring.
#[cube(launch_unchecked)]
pub fn inspect_geometry(d: &Input, candidate: u32, out: &mut Array<f32>) {
    let i = ABSOLUTE_POS as usize;
    if i < d.fi[N] as usize {
        let p = load_prim(d, i, candidate as usize);
        let m = load_prim(d, d.fi[MOVING] as usize, candidate as usize);
        let j = i * 16;
        out[j] = p.bbox.l;
        out[j + 1] = p.bbox.r;
        out[j + 2] = p.bbox.t;
        out[j + 3] = p.bbox.b;
        out[j + 4] = p.body.l;
        out[j + 5] = p.body.r;
        out[j + 6] = p.body.t;
        out[j + 7] = p.body.b;
        out[j + 8] = p.bbox.r + d.ff[0] - m.bbox.l;
        out[j + 9] = m.bbox.r + d.ff[0] - p.bbox.l;
        out[j + 10] = p.bbox.b + d.ff[0] - m.bbox.t;
        out[j + 11] = m.bbox.b + d.ff[0] - p.bbox.t;
        out[j + 12] = overlap(p.bbox, m.bbox, d.ff[0]);
        out[j + 13] = p.dx;
        out[j + 14] = p.dy;
        out[j + 15] = d.ff[0];
    }
}

// Mass candidate checks used by the existing frontier-scarcity heuristic.
#[cube(launch_unchecked)]
pub fn scarcity_probe(d: &Input, scores: &mut Array<f32>, tags: &mut Array<u32>) {
    let candidate = ABSOLUTE_POS as usize;
    if candidate < d.fi[COUNT] as usize {
        let p = load_prim(d, d.fi[MOVING] as usize, candidate);
        let mut hard = unary_hard(d,p,false);
        for i in 0usize..d.fi[MOVING] as usize {
            let core = load_prim(d, i, candidate);
            let mut depth=component_overlap(d,p,core);if (d.fi[FLAGS]&2)!=0 {depth=primitive_overlap(d,p,core);}
            if depth>0.0 {hard+=1;}
        }
        let mut sum = 0.0f32;
        let mut pins = 0u32;
        for j in 0usize..p.nc {
            let cp = cp_at(d, p, j);
            let t = d.fi[CP_I] as usize + (p.cp + j) * 4;
            if d.si[t + 3] != 0 {
                let mut nearest = f32::from_bits(0x7f800000u32);
                let mut found = false;
                for i in 0usize..d.fi[MOVING] as usize {
                    let core = load_prim(d, i, candidate);
                    for k in 0usize..core.nc {
                        let target = cp_at(d, core, k);
                        if target.net == cp.net {
                            nearest = min(
                                nearest,
                                distance(
                                    P { x: cp.x, y: cp.y },
                                    P {
                                        x: target.x,
                                        y: target.y,
                                    },
                                ),
                            );
                            found = true;
                        }
                    }
                }
                if found {
                    sum += nearest;
                    pins += 1;
                }
            }
        }
        scores[candidate] = sum;
        tags[candidate * 2] = hard;
        tags[candidate * 2 + 1] = pins;
    }
}
#[cube(launch_unchecked)]
pub fn scarcity_nearby(
    d: &Input,
    scores: &Array<f32>,
    tags: &Array<u32>,
    nearby: &mut Array<u32>,
    pins: &mut Array<u32>,
) {
    let source = ABSOLUTE_POS as usize;
    if source < d.fi[SOURCES] as usize {
        let range = d.fi[NET_I] as usize + source * 2;
        let start = d.fi[range] as usize;
        let end = d.fi[range + 1] as usize;
        let mut best = f32::from_bits(0x7f800000u32);
        let mut found = false;
        let mut count = 0u32;
        for i in start..end {
            count = tags[i * 2 + 1];
            if tags[i * 2] == 0 {
                best = min(best, scores[i]);
                found = true;
            }
        }
        if !found {
            count = 0;
        }
        pins[source] = count;
        for i in start..end {
            nearby[i] = 0;
            if found && count > 0 && tags[i * 2] == 0 && scores[i] <= best + count as f32 {
                nearby[i] = 1;
            }
        }
    }
}
#[cube(launch_unchecked)]
pub fn scarcity_legal(d: &Input, nearby: &Array<u32>, legal: &mut Array<u32>) {
    let candidate = ABSOLUTE_POS as usize;
    if candidate < d.fi[COUNT] as usize {
        legal[candidate] = 0;
        if nearby[candidate] != 0 {
            let p = load_prim(d, d.fi[MOVING] as usize, candidate);
            let mut valid = unary_hard(d,p,false)==0;
            for i in 0usize..d.fi[MOVING] as usize {
                let q = load_prim(d, i, candidate);
                let mut depth=component_overlap(d,p,q);if (d.fi[FLAGS]&2)!=0 {depth=primitive_overlap(d,p,q);}
                if depth>0.0 {valid=false;}
            }
            if valid {
                legal[candidate] = 1;
            }
        }
    }
}
#[cube(launch_unchecked)]
pub fn scarcity_counts(
    d: &Input,
    nearby: &Array<u32>,
    pins: &Array<u32>,
    legal: &Array<u32>,
    out: &mut Array<u32>,
) {
    let source = ABSOLUTE_POS as usize;
    if source < d.fi[SOURCES] as usize {
        let range = d.fi[NET_I] as usize + source * 2;
        let mut n = 0u32;
        let mut l = 0u32;
        for i in d.fi[range] as usize..d.fi[range + 1] as usize {
            n += nearby[i];
            l += legal[i];
        }
        out[source * 3] = pins[source];
        out[source * 3 + 1] = n;
        out[source * 3 + 2] = l;
    }
}
