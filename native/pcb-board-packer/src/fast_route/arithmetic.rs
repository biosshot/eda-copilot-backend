//! The same ordered F32 path arithmetic is compiled for Rust and CubeCL.
#[cfg(feature = "gpu")]
use cubecl::prelude::*;

#[derive(Clone, Copy)]
#[cfg_attr(feature = "gpu", derive(CubeType))]
pub struct P { pub x: f32, pub y: f32 }
#[derive(Clone, Copy)]
#[cfg_attr(feature = "gpu", derive(CubeType))]
pub struct Path { pub a: P, pub b: P, pub c: P, pub d: P, pub count: u32 }

#[cfg_attr(feature = "gpu", cube)]
pub fn point(p: Path, i: u32) -> P {
    let mut q = p.a;
    if i == 1 { q.x = p.b.x; q.y = p.b.y; }
    if i == 2 { q.x = p.c.x; q.y = p.c.y; }
    if i == 3 { q.x = p.d.x; q.y = p.d.y; }
    q
}
#[cfg_attr(feature = "gpu", cube)]
pub fn orient(a: P, b: P, c: P) -> f32 {
    (b.x-a.x)*(c.y-a.y)-(b.y-a.y)*(c.x-a.x)
}
#[cfg_attr(feature = "gpu", cube)]
pub fn crosses(a: P, b: P, c: P, d: P) -> bool {
    crate::geometry_policy::crosses(a.x,a.y,b.x,b.y,c.x,c.y,d.x,d.y)
}
#[cfg_attr(feature = "gpu", cube)]
pub fn hits(a: P, b: P, l: f32, r: f32, t: f32, bottom: f32) -> bool {
    crate::geometry_policy::hits_box(a.x,a.y,b.x,b.y,l,r,t,bottom)
}
#[cfg_attr(feature = "gpu", cube)]
#[inline(always)]
pub fn obstacle(p: Path, l: f32, r: f32, t: f32, b: f32, clearance: f32, pad_factor: f32) -> f32 {
    let mut direct = false; let mut band = false;
    for i in 0..p.count-1 {
        let a = point(p,i); let z = point(p,i+1);
        if hits(a,z,l,r,t,b) { direct = true; }
    }
    if !direct {
        for i in 0..p.count-1 {
            if hits(point(p,i),point(p,i+1),l-clearance,r+clearance,t-clearance,b+clearance) { band = true; }
        }
    }
    let mut cost = 0.0;
    if direct { cost = 20.0*pad_factor; } else if band { cost = 2.0; }
    cost
}
#[cfg_attr(feature = "gpu", cube)]
#[inline(always)]
pub fn crossing(p: Path, a: P, b: P, factor: f32) -> f32 {
    let mut hit = false;
    for i in 0..p.count-1 { if crosses(point(p,i),point(p,i+1),a,b) { hit = true; } }
    let mut cost = 0.0;
    if hit { cost = 3.0*factor; }
    cost
}
