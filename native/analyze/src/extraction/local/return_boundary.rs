//! Conservative absolute normal current through an axis-aligned internal guard.
use super::{
    error,
    mixed_conductor::{MixedMap, Owner},
    Result,
};

fn density(entries: &[(usize, [f64; 3])], re: &[f64], im: &[f64]) -> Result<[[f64; 3]; 2]> {
    let mut out = [[0.; 3]; 2];
    for &(i, v) in entries {
        let (&r, &j) = re
            .get(i)
            .zip(im.get(i))
            .ok_or_else(|| error("RETURN_BOUNDARY", "missing current coordinate"))?;
        for k in 0..3 {
            out[0][k] += v[k] * r;
            out[1][k] += v[k] * j;
        }
    }
    if out.iter().flatten().any(|v| !v.is_finite()) {
        return Err(error("RETURN_BOUNDARY", "nonfinite current density"));
    }
    Ok(out)
}

// Slice a tetrahedron by one face, clip the resulting convex polygon to the
// other four box planes, and return its area. On a coincident tetrahedron face,
// only the tetrahedron on the box-interior side owns that face.
fn area(t: [[f64; 3]; 4], b: [[f64; 3]; 2], axis: usize, side: usize) -> f64 {
    let plane = b[side][axis];
    if !t.iter().any(|p| {
        if side == 0 {
            p[axis] > plane
        } else {
            p[axis] < plane
        }
    }) {
        return 0.;
    }
    let axes: Vec<_> = (0..3).filter(|&k| k != axis).collect();
    let mut pts: Vec<[f64; 2]> = Vec::new();
    let mut add = |p: [f64; 2]| {
        if !pts.iter().any(|q| q[0] == p[0] && q[1] == p[1]) {
            pts.push(p);
        }
    };
    for p in t {
        if p[axis] == plane {
            add([p[axes[0]], p[axes[1]]]);
        }
    }
    for i in 0..4 {
        for j in i + 1..4 {
            let a = t[i][axis] - plane;
            let c = t[j][axis] - plane;
            if (a < 0. && c > 0.) || (a > 0. && c < 0.) {
                let f = a / (a - c);
                add([
                    t[i][axes[0]] + f * (t[j][axes[0]] - t[i][axes[0]]),
                    t[i][axes[1]] + f * (t[j][axes[1]] - t[i][axes[1]]),
                ]);
            }
        }
    }
    if pts.len() < 3 {
        return 0.;
    }
    let center = [
        pts.iter().map(|p| p[0]).sum::<f64>() / pts.len() as f64,
        pts.iter().map(|p| p[1]).sum::<f64>() / pts.len() as f64,
    ];
    pts.sort_by(|a, c| {
        (a[1] - center[1])
            .atan2(a[0] - center[0])
            .total_cmp(&(c[1] - center[1]).atan2(c[0] - center[0]))
    });
    for d in 0..2 {
        for s in 0..2 {
            if pts.is_empty() {
                return 0.;
            }
            let bound = b[s][axes[d]];
            let inside = |p: [f64; 2]| if s == 0 { p[d] >= bound } else { p[d] <= bound };
            let mut next = Vec::new();
            let mut prev = *pts.last().unwrap();
            for &p in &pts {
                let a = inside(prev);
                let c = inside(p);
                if a != c {
                    let f = (bound - prev[d]) / (p[d] - prev[d]);
                    let mut q = [
                        prev[0] + f * (p[0] - prev[0]),
                        prev[1] + f * (p[1] - prev[1]),
                    ];
                    q[d] = bound;
                    next.push(q);
                }
                if c {
                    next.push(p);
                }
                prev = p;
            }
            pts = next;
        }
    }
    if pts.len() < 3 {
        return 0.;
    }
    let mut sum = 0.;
    for i in 0..pts.len() {
        let p = pts[i];
        let q = pts[(i + 1) % pts.len()];
        sum += (p[0] - center[0]) * (q[1] - center[1]) - (p[1] - center[1]) * (q[0] - center[0]);
    }
    sum.abs() * 0.5
}

pub(super) fn conductive(
    map: &MixedMap<'_>,
    re: &[f64],
    im: &[f64],
    bounds: [[f64; 3]; 2],
    start: std::time::Instant,
    wall: std::time::Duration,
) -> Result<[f64; 6]> {
    if bounds.iter().flatten().any(|v| !v.is_finite())
        || (0..3).any(|k| bounds[0][k] >= bounds[1][k])
    {
        return Err(error("RETURN_BOUNDARY", "invalid guard bounds"));
    }
    let planar = map.planar_current_supports()?;
    let local = map.local_current_supports()?;
    let barrel = map.barrel_current_supports()?;
    let mut out = [0.; 6];
    let mut cursor = 0;
    for (i, owner) in map.owners.iter().enumerate() {
        if i % 256 == 0 && start.elapsed() >= wall {
            return Err(error("DEADLINE", "return boundary conductive integration"));
        }
        let (tetra, d) = match owner {
            Owner::Foil(f) => {
                let cell = &map.mesh().mesh.copper_tetrahedra[i];
                (
                    cell.nodes.map(|n| map.mesh().mesh.nodes_m[n]),
                    density(&planar[*f].current_density, re, im)?,
                )
            }
            Owner::Barrel(b) => {
                let cell = &map.mesh().mesh.copper_tetrahedra[i];
                (
                    cell.nodes.map(|n| map.mesh().mesh.nodes_m[n]),
                    density(&barrel[*b].current_density, re, im)?,
                )
            }
            Owner::Dropped => continue,
            Owner::Volume => {
                let s = &local[cursor];
                cursor += 1;
                (s.tetra, density(&s.densities, re, im)?)
            }
        };
        for axis in 0..3 {
            for side in 0..2 {
                out[axis * 2 + side] +=
                    area(tetra, bounds, axis, side) * d[0][axis].hypot(d[1][axis]);
            }
        }
    }
    if out.iter().any(|v| !v.is_finite()) {
        return Err(error("RETURN_BOUNDARY", "nonfinite conductive flux"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slice_and_coincident_ownership() {
        let t = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        let b = [[0., 0., 0.], [0.5, 1., 1.]];
        assert!((area(t, b, 0, 1) - 0.125).abs() < 1e-14);
        assert!((area(t, b, 0, 0) - 0.5).abs() < 1e-14);
        let outside = [[-1., 0., 0.], [0., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        assert_eq!(area(outside, b, 0, 0), 0.);
    }
}
