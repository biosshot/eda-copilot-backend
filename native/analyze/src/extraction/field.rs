//! Conforming graded rectilinear triangles. Every metal edge and dielectric
//! interface is an exact mesh constraint; cells inside PEC are excluded.
use super::*;
use faer::{
    dyn_stack::{MemBuffer, MemStack},
    sparse::{linalg::cholesky, SparseColMat, Triplet},
    Conj, Mat, Par, Side,
};
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub geometry: Geometry,
    pub nodes: Vec<[f64; 2]>,
    pub triangles: Vec<[usize; 3]>,
    pub epsilon_r: Vec<f64>,
    pub boundary: Vec<i32>,
    pub potentials_v: Vec<Vec<f64>>,
    pub electric_fields_v_per_m: Vec<Vec<[f64; 2]>>,
    pub capacitance: Vec<Vec<f64>>,
    pub vacuum_capacitance: Vec<Vec<f64>>,
    pub quality: Value,
    #[serde(skip)]
    pub vacuum_reaction_c_per_m: Vec<Vec<f64>>,
}
fn axis(mut constraints: Vec<f64>, focus: &[f64], step: f64, feature: f64) -> Result<Vec<f64>> {
    constraints.sort_by(f64::total_cmp);
    constraints.dedup_by(|a, b| (*a - *b).abs() < 1e-16);
    let mut out = vec![constraints[0]];
    fn split(
        a: f64,
        b: f64,
        focus: &[f64],
        h: f64,
        f: f64,
        out: &mut Vec<f64>,
        depth: usize,
    ) -> Result<()> {
        let mid = (a + b) / 2.;
        let distance = focus
            .iter()
            .map(|x| {
                if *x < a {
                    a - x
                } else if *x > b {
                    x - b
                } else {
                    0.
                }
            })
            .fold(f64::INFINITY, f64::min);
        if b - a > h * (1. + distance / (f * 0.15)).powf(0.85) * (1. + 1e-12) {
            if depth > 30 || out.len() > 2000 {
                return Err(error("SECTION_MESH_LIMIT", "axis refinement cap"));
            }
            split(a, mid, focus, h, f, out, depth + 1)?;
            split(mid, b, focus, h, f, out, depth + 1)?;
        } else {
            out.push(b);
        }
        Ok(())
    }
    for p in constraints.windows(2) {
        split(p[0], p[1], focus, step, feature, &mut out, 0)?;
    }
    Ok(out)
}
fn mesh(
    g: &Geometry,
    h: f64,
    limits: &Limits,
) -> Result<(Vec<[f64; 2]>, Vec<[usize; 3]>, Vec<f64>, Vec<i32>)> {
    let b = g.bounds_m;
    let mut xx = vec![b[0], b[2], 0.];
    let mut xf = vec![];
    let mut yy = vec![b[1], b[3]];
    let mut yf = vec![];
    for c in &g.conductors {
        for x in [c.bounds_m[0], c.bounds_m[2]] {
            xx.push(x);
            xf.push(x);
        }
        for y in [c.bounds_m[1], c.bounds_m[3]] {
            yy.push(y);
            yf.push(y);
        }
    }
    for d in &g.dielectrics {
        yy.extend([d.y_min_m, d.y_max_m]);
    }
    let xs = axis(xx, &xf, h, g.feature())?;
    let ys = axis(yy, &yf, h, g.feature())?;
    let nx = xs.len();
    let ny = ys.len();
    if nx * ny > limits.nodes || 2 * (nx - 1) * (ny - 1) > limits.triangles {
        return Err(error(
            "SECTION_MESH_LIMIT",
            format!(
                "grid {} nodes / {} triangle bound",
                nx * ny,
                2 * (nx - 1) * (ny - 1)
            ),
        ));
    }
    // Upper bound includes triplets, factors, all bases/fields, JSON and previous
    // accepted mesh during domain comparison. Not a strict allocator/RSS limit.
    limits.check(nx * ny * 2048 + 2 * (nx - 1) * (ny - 1) * 512 + 65536)?;
    let mut nodes = vec![];
    let mut boundary = vec![];
    let mut remap = vec![usize::MAX; nx * ny];
    for (j, &y) in ys.iter().enumerate() {
        for (i, &x) in xs.iter().enumerate() {
            let mut tag = 0;
            for (k, c) in g.conductors.iter().enumerate() {
                let a = c.bounds_m;
                if x > a[0] + 1e-16 && x < a[2] - 1e-16 && y > a[1] + 1e-16 && y < a[3] - 1e-16 {
                    tag = i32::MIN;
                    break;
                }
                if x >= a[0] - 1e-16 && x <= a[2] + 1e-16 && y >= a[1] - 1e-16 && y <= a[3] + 1e-16
                {
                    tag = k as i32 + 1;
                }
            }
            if tag == i32::MIN {
                continue;
            }
            if g.reference_y_m.iter().any(|v| (y - v).abs() < 1e-16) {
                tag = -1;
            }
            remap[j * nx + i] = nodes.len();
            nodes.push([x, y]);
            boundary.push(tag);
        }
    }
    let mut triangles = vec![];
    let mut eps = vec![];
    for j in 0..ny - 1 {
        for i in 0..nx - 1 {
            let x = (xs[i] + xs[i + 1]) / 2.;
            let y = (ys[j] + ys[j + 1]) / 2.;
            if g.conductors.iter().any(|c| {
                x > c.bounds_m[0] && x < c.bounds_m[2] && y > c.bounds_m[1] && y < c.bounds_m[3]
            }) {
                continue;
            }
            let er = g
                .dielectrics
                .iter()
                .find(|d| y > d.y_min_m && y < d.y_max_m)
                .ok_or_else(|| error("SECTION_MESH", "cell has no material"))?
                .relative_permittivity;
            let [a, b, c, d] = [
                remap[j * nx + i],
                remap[j * nx + i + 1],
                remap[(j + 1) * nx + i],
                remap[(j + 1) * nx + i + 1],
            ];
            if [a, b, c, d].contains(&usize::MAX) {
                return Err(error("SECTION_MESH", "metal interior leaked into element"));
            }
            triangles.extend([[a, b, d], [a, d, c]]);
            eps.extend([er, er]);
        }
    }
    Ok((nodes, triangles, eps, boundary))
}
fn element(nodes: &[[f64; 2]], t: [usize; 3]) -> Result<(f64, [[f64; 2]; 3])> {
    let [a, b, c] = t.map(|i| nodes[i]);
    let det = (b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1]);
    if !det.is_finite() || det <= 0. {
        return Err(error("SECTION_MESH", "degenerate/inverted triangle"));
    }
    Ok((
        det / 2.,
        [
            [(b[1] - c[1]) / det, (c[0] - b[0]) / det],
            [(c[1] - a[1]) / det, (a[0] - c[0]) / det],
            [(a[1] - b[1]) / det, (b[0] - a[0]) / det],
        ],
    ))
}
struct Solution {
    c: Vec<Vec<f64>>,
    phi: Vec<Vec<f64>>,
    e: Vec<Vec<[f64; 2]>>,
    quality: Value,
    reaction: Vec<Vec<f64>>,
}
fn solve_basis(
    nodes: &[[f64; 2]],
    ts: &[[usize; 3]],
    eps: &[f64],
    bc: &[i32],
    n: usize,
    limits: &Limits,
) -> Result<Solution> {
    let mut map = vec![usize::MAX; nodes.len()];
    let mut free = 0;
    for (i, b) in bc.iter().enumerate() {
        if *b == 0 {
            map[i] = free;
            free += 1;
        }
    }
    if free == 0 {
        return Err(error("SECTION_MESH", "no dielectric unknowns"));
    }
    let mut rhs = Mat::zeros(free, n);
    let mut terms = Vec::with_capacity(ts.len() * 9);
    let mut triplets = Vec::with_capacity(ts.len() * 9);
    for (t, er) in ts.iter().zip(eps) {
        let (area, grad) = element(nodes, *t)?;
        for i in 0..3 {
            for j in 0..3 {
                let v = area * er * (grad[i][0] * grad[j][0] + grad[i][1] * grad[j][1]);
                let a = t[i];
                let b = t[j];
                terms.push((a, b, v));
                if bc[a] == 0 {
                    if bc[b] == 0 {
                        triplets.push(Triplet::new(map[a], map[b], v));
                    } else if bc[b] > 0 {
                        rhs[(map[a], bc[b] as usize - 1)] -= v;
                    }
                }
            }
        }
    }
    limits.check(nodes.len() * 2048 + ts.len() * 512 + 65536)?;
    let matrix = SparseColMat::try_new_from_triplets(free, free, &triplets)
        .map_err(|e| error("SECTION_MATRIX", e))?;
    drop(triplets);
    let symbolic = cholesky::factorize_symbolic_cholesky(
        matrix.symbolic(),
        Side::Lower,
        Default::default(),
        Default::default(),
    )
    .map_err(|e| error("SECTION_MATRIX", e))?;
    let factor_scratch =
        symbolic.factorize_numeric_llt_scratch::<f64>(Par::Seq, Default::default());
    let solve_scratch = symbolic.solve_in_place_scratch::<f64>(n, Par::Seq);
    let memory_plan = nodes.len() * 2048
        + ts.len() * 512
        + 65536
        + symbolic.len_val() * 8
        + factor_scratch
            .unaligned_bytes_required()
            .max(solve_scratch.unaligned_bytes_required());
    limits.check(memory_plan)?;
    let mut values = vec![0.; symbolic.len_val()];
    let mut workspace = MemBuffer::try_new(factor_scratch)
        .map_err(|e| error("SECTION_MEMORY_LIMIT", format!("factor scratch {e:?}")))?;
    let factor = symbolic
        .factorize_numeric_llt(
            &mut values,
            matrix.as_ref(),
            Side::Lower,
            Default::default(),
            Par::Seq,
            MemStack::new(&mut workspace),
            Default::default(),
        )
        .map_err(|e| error("SECTION_ENERGY", e))?;
    drop(workspace);
    limits.check(0)?;
    let mut workspace = MemBuffer::try_new(solve_scratch)
        .map_err(|e| error("SECTION_MEMORY_LIMIT", format!("solve scratch {e:?}")))?;
    let mut x = rhs;
    factor.solve_in_place_with_conj(
        Conj::No,
        x.as_mut(),
        Par::Seq,
        MemStack::new(&mut workspace),
    );
    drop(workspace);
    let mut phi = vec![vec![0.; nodes.len()]; n];
    for k in 0..n {
        for (i, b) in bc.iter().enumerate() {
            phi[k][i] = if *b == 0 {
                x[(map[i], k)]
            } else if *b == k as i32 + 1 {
                1.
            } else {
                0.
            };
        }
    }
    if phi
        .iter()
        .flatten()
        .any(|v| !v.is_finite() || *v < -1e-8 || *v > 1. + 1e-8)
    {
        return Err(error(
            "SECTION_ENERGY",
            "nonfinite potential or discrete maximum principle violation",
        ));
    }
    let mut reaction = vec![vec![0.; nodes.len()]; n];
    let mut scale = vec![vec![0.; nodes.len()]; n];
    for (a, b, v) in &terms {
        for k in 0..n {
            reaction[k][*a] += v * phi[k][*b];
            scale[k][*a] += (v * phi[k][*b]).abs();
        }
    }
    let mut residual: f64 = 0.;
    let mut c = vec![vec![0.; n]; n];
    let mut balance: f64 = 0.;
    let mut refs = vec![0.; n];
    for k in 0..n {
        let mut sum = 0.;
        let mut norm = 0.;
        for (i, b) in bc.iter().enumerate() {
            let q = reaction[k][i];
            if *b == 0 {
                residual = residual.max(q.abs() / scale[k][i].max(1e-30));
            } else {
                sum += q;
                norm += q.abs();
                if *b > 0 {
                    c[*b as usize - 1][k] += q * EPS0;
                } else {
                    refs[k] += q * EPS0;
                }
            }
        }
        balance = balance.max(sum.abs() / norm.max(1e-30));
    }
    if phi
        .iter()
        .flatten()
        .chain(c.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err(error(
            "SECTION_NONFINITE",
            "nonfinite potential/charge coefficient",
        ));
    }
    let mut symmetry: f64 = 0.;
    let mut energy_error: f64 = 0.;
    let mut energy = vec![0.; n];
    for i in 0..n {
        if !c[i][i].is_finite() || c[i][i] <= 0. {
            return Err(error("SECTION_ENERGY", "nonpositive diagonal C"));
        }
        for j in 0..n {
            symmetry = symmetry.max((c[i][j] - c[j][i]).abs() / c[i][i]);
            if i != j && c[i][j] > 1e-10 * c[i][i] {
                return Err(error("SECTION_ENERGY", "positive mutual Maxwell C"));
            }
            let integral = terms
                .iter()
                .map(|(a, b, v)| phi[i][*a] * v * phi[j][*b] * EPS0)
                .sum::<f64>();
            if !integral.is_finite() {
                return Err(error("SECTION_NONFINITE", "nonfinite FEM energy integral"));
            }
            energy_error = energy_error.max((integral - c[i][j]).abs() / c[i][i]);
            if i == j {
                energy[i] = 0.5 * integral;
            }
        }
    }
    if n == 2 && c[0][0] * c[1][1] <= c[0][1] * c[1][0] {
        return Err(error("SECTION_ENERGY", "C not positive definite"));
    }
    if !residual.is_finite()
        || residual > 1e-8
        || symmetry > 1e-8
        || balance > 1e-8
        || energy_error > 1e-8
    {
        return Err(error(
            "SECTION_RESIDUAL",
            format!(
                "residual {residual}, symmetry {symmetry}, charge {balance}, energy {energy_error}"
            ),
        ));
    }
    let mut fields = vec![];
    for p in &phi {
        let mut e = vec![];
        for t in ts {
            let (_, grad) = element(nodes, *t)?;
            let mut v = [0., 0.];
            for i in 0..3 {
                for j in 0..2 {
                    v[j] -= p[t[i]] * grad[i][j];
                }
            }
            e.push(v);
        }
        if e.iter().flatten().any(|v| !v.is_finite()) {
            return Err(error("SECTION_NONFINITE", "nonfinite electric field"));
        }
        fields.push(e);
    }
    limits.check(0)?;
    Ok(Solution {
        c,
        phi,
        e: fields,
        reaction: reaction
            .into_iter()
            .map(|r| r.into_iter().map(|v| v * EPS0).collect())
            .collect(),
        quality: json!({"normalizedResidual":residual,"normalizedSymmetry":symmetry,"normalizedChargeBalance":balance,"normalizedEnergyReactionError":energy_error,"energyJPerM":energy,"referenceChargeCPerM":refs,"factorizations":1,"rhs":n,"memoryPlanBytes":memory_plan,"factorValues":symbolic.len_val(),"solver":"faer sparse LLT CPU/F64; epsilon0-scaled stiffness; natural outer BC"}),
    })
}
pub(super) fn solve(g: &Geometry, h: f64, limits: &Limits) -> Result<Field> {
    let (nodes, triangles, epsilon_r, boundary) = mesh(g, h, limits)?;
    let n = g.conductors.len();
    let mut max_aspect: f64 = 0.;
    let mut min_angle: f64 = 90.;
    for t in &triangles {
        let p = t.map(|i| nodes[i]);
        let dx = (p[1][0] - p[0][0]).abs().max((p[2][0] - p[0][0]).abs());
        let dy = (p[1][1] - p[0][1]).abs().max((p[2][1] - p[0][1]).abs());
        let aspect = dx.max(dy) / dx.min(dy);
        max_aspect = max_aspect.max(aspect);
        min_angle = min_angle.min((1. / aspect).atan().to_degrees());
    }
    if !max_aspect.is_finite() || max_aspect > 1e9 {
        return Err(error(
            "SECTION_MESH_QUALITY",
            "triangle aspect exceeds 1e9 (very thin edge-aligned features)",
        ));
    }
    let actual = solve_basis(&nodes, &triangles, &epsilon_r, &boundary, n, limits)?;
    let vacuum = solve_basis(
        &nodes,
        &triangles,
        &vec![1.; triangles.len()],
        &boundary,
        n,
        limits,
    )?;
    Ok(Field {
        geometry: g.clone(),
        nodes,
        triangles,
        epsilon_r,
        boundary,
        potentials_v: actual.phi,
        electric_fields_v_per_m: actual.e,
        capacitance: actual.c,
        vacuum_capacitance: vacuum.c,
        vacuum_reaction_c_per_m: vacuum.reaction,
        quality: json!({"actual":actual.quality,"vacuum":vacuum.quality,"mesh":{"minimumAngleDeg":min_angle,"maximumAspect":max_aspect,"constraints":"axis-aligned P1 right triangles; material/PEC edges exact; anisotropy reported, maximumAspect<=1e9"}}),
    })
}
/// Internal analytic field coupon uses the identical mesh/assembly/basis path.
/// Left/right natural BC, bottom reference, upper PEC spanning full width.
pub fn solve_field(g: &Geometry, step_m: f64) -> Result<Field> {
    let limits = Limits {
        nodes: 200000,
        triangles: 400000,
        memory: 512 * 1048576,
        seconds: 120.,
        start: Instant::now(),
    };
    solve(g, step_m, &limits)
}
