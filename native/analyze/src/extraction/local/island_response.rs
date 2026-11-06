//! Bounded full-source island response; no dispatch or original-field admission claim.
#![allow(dead_code)]
use super::{
    accurate::Sum, error, volume_current::Space, volume_field::ElectricApply, AllocationBudget,
    Result,
};
use faer::{prelude::Solve, Mat};
use std::time::{Duration, Instant};

pub(super) struct IslandResponse<'a> {
    space: &'a Space,
    field: &'a dyn ElectricApply,
    columns: Vec<Vec<f64>>,
    jacobian: Mat<f64>,
    planned: usize,
    retained: usize,
    scratch: usize,
}
pub(super) struct Refined {
    pub real_constants: Vec<Sum>,
    pub imaginary_constants: Vec<Sum>,
    pub real_charge: Vec<Sum>,
    pub imaginary_charge: Vec<Sum>,
    pub iterations: usize,
    pub worst_ratio: f64,
}
fn check(start: Instant, limit: Duration) -> Result<()> {
    if start.elapsed() >= limit {
        Err(error("DEADLINE", "island response deadline"))
    } else {
        Ok(())
    }
}
impl<'a> IslandResponse<'a> {
    pub(super) fn new(
        space: &'a Space,
        field: &'a dyn ElectricApply,
        budget: AllocationBudget,
        start: Instant,
        limit: Duration,
    ) -> Result<Self> {
        check(start, limit)?;
        let n = space.potential_count();
        let k = space.components.len();
        if n == 0 || k == 0 || n > budget.max_nodes || k > budget.max_nodes {
            return Err(error(
                "ISLAND_INPUT",
                "nonempty bounded component partition required",
            ));
        }
        let retained = n
            .checked_mul(k)
            .and_then(|x| x.checked_mul(8))
            .and_then(|x| x.checked_add(k.checked_mul(k)?.checked_mul(8)?))
            .and_then(|x| x.checked_add(k.checked_mul(64)?))
            .ok_or_else(|| error("MEMORY_LIMIT", "island retained plan overflow"))?;
        let scratch = n
            .checked_mul(k)
            .and_then(|x| x.checked_mul(8))
            .and_then(|x| x.checked_add(k.checked_mul(k)?.checked_mul(256)?))
            .and_then(|x| x.checked_add(n.checked_mul(256)?))
            .and_then(|x| x.checked_add(k.checked_mul(256)?))
            .and_then(|x| x.checked_add(field.scratch_bytes()))
            .ok_or_else(|| error("MEMORY_LIMIT", "island scratch plan overflow"))?;
        let plan = retained
            .checked_add(scratch)
            .and_then(|x| x.checked_add(space.planned_bytes))
            .and_then(|x| x.checked_add(field.planned_bytes()))
            .ok_or_else(|| error("MEMORY_LIMIT", "island response plan overflow"))?;
        budget.check(plan)?;
        let mut seen = vec![false; n];
        for nodes in &space.components {
            check(start, limit)?;
            if nodes.is_empty() {
                return Err(error("ISLAND_INPUT", "empty component"));
            }
            for &i in nodes {
                if i >= n || seen[i] {
                    return Err(error(
                        "ISLAND_INPUT",
                        "invalid or duplicate component coordinate",
                    ));
                }
                seen[i] = true;
            }
        }
        if seen.iter().any(|v| !*v) {
            return Err(error("ISLAND_INPUT", "incomplete component partition"));
        }
        let mut columns = Vec::with_capacity(k);
        for nodes in &space.components {
            check(start, limit)?;
            let mut indicator = vec![0.; n];
            for &i in nodes {
                indicator[i] = 1.;
            }
            let q = field.apply(&indicator)?;
            check(start, limit)?;
            if q.len() != n || q.iter().any(|v| !v.is_finite()) {
                return Err(error("ISLAND_INPUT", "invalid field response"));
            }
            columns.push(q);
        }
        let jacobian = Mat::from_fn(k, k, |i, j| {
            space.components[i]
                .iter()
                .fold(Sum::default(), |sum, &p| sum.add(Sum::from(columns[j][p])))
                .value()
        });
        if (0..k).any(|i| (0..k).any(|j| !jacobian[(i, j)].is_finite())) {
            return Err(error("ISLAND_INPUT", "nonfinite island Jacobian"));
        }
        check(start, limit)?;
        Ok(Self {
            space,
            field,
            columns,
            jacobian,
            planned: plan,
            retained,
            scratch,
        })
    }
    pub(super) fn planned_bytes(&self) -> usize {
        self.planned
    }
    pub(super) fn retained_bytes(&self) -> usize {
        self.retained
    }
    pub(super) fn scratch_bytes(&self) -> usize {
        self.scratch
    }
    pub(super) fn compose(
        &self,
        base: &[f64],
        delta: &[Sum],
        start: Instant,
        limit: Duration,
    ) -> Result<Vec<Sum>> {
        check(start, limit)?;
        let n = self.space.potential_count();
        let k = self.columns.len();
        if base.len() != n
            || delta.len() != k
            || base.iter().any(|x| !x.is_finite())
            || delta.iter().any(|x| !x.hi.is_finite() || !x.lo.is_finite())
        {
            return Err(error("ISLAND_INPUT", "invalid response composition"));
        }
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            check(start, limit)?;
            let q = (0..k).fold(Sum::from(base[i]), |v, j| {
                v.add(delta[j].mul(self.columns[j][i]))
            });
            if !q.hi.is_finite() || !q.lo.is_finite() {
                return Err(error("ISLAND_RANGE", "nonfinite composed charge"));
            }
            out.push(q);
        }
        Ok(out)
    }
    pub(super) fn refine(
        &self,
        vr: &[f64],
        vi: &[f64],
        ir: &[f64],
        ii: &[f64],
        omega: f64,
        budget: AllocationBudget,
        start: Instant,
        limit: Duration,
    ) -> Result<Refined> {
        check(start, limit)?;
        budget.check(self.planned)?;
        let n = self.space.potential_count();
        let k = self.columns.len();
        if !omega.is_finite()
            || omega <= 0.
            || [vr, vi, ir, ii]
                .iter()
                .any(|v| v.len() != n || v.iter().any(|x| !x.is_finite()))
        {
            return Err(error("ISLAND_INPUT", "invalid island refinement state"));
        }
        let br = self.field.apply(vr)?;
        check(start, limit)?;
        let bi = self.field.apply(vi)?;
        check(start, limit)?;
        if br.len() != n || bi.len() != n || br.iter().chain(&bi).any(|x| !x.is_finite()) {
            return Err(error("ISLAND_INPUT", "invalid base field action"));
        }
        let mut factor = None;
        let mut zr = vec![Sum::default(); k];
        let mut zi = zr.clone();
        let mut previous = f64::INFINITY;
        for iteration in 0..8 {
            check(start, limit)?;
            let qr = self.compose(&br, &zr, start, limit)?;
            let qi = self.compose(&bi, &zi, start, limit)?;
            let mut rr = vec![0.; k];
            let mut ri = vec![0.; k];
            let mut worst = 0f64;
            for (c, nodes) in self.space.components.iter().enumerate() {
                check(start, limit)?;
                let sr = nodes
                    .iter()
                    .fold(Sum::default(), |a, &i| a.add(Sum::from(ir[i])))
                    .value();
                let si = nodes
                    .iter()
                    .fold(Sum::default(), |a, &i| a.add(Sum::from(ii[i])))
                    .value();
                rr[c] = -nodes
                    .iter()
                    .fold(Sum::default(), |a, &i| a.add(qr[i]))
                    .sub(Sum::from(si / omega))
                    .value();
                ri[c] = -nodes
                    .iter()
                    .fold(Sum::default(), |a, &i| a.add(qi[i]))
                    .add(Sum::from(sr / omega))
                    .value();
                let scale = nodes
                    .iter()
                    .map(|&i| qr[i].value().abs() + qi[i].value().abs())
                    .sum::<f64>()
                    + (sr.abs() + si.abs()) / omega;
                if !sr.is_finite()
                    || !si.is_finite()
                    || !rr[c].is_finite()
                    || !ri[c].is_finite()
                    || !scale.is_finite()
                {
                    return Err(error("ISLAND_RANGE", "nonfinite charge scale or residual"));
                }
                let ratio = rr[c].hypot(ri[c]) / scale.max(f64::MIN_POSITIVE);
                if !ratio.is_finite() {
                    return Err(error("ISLAND_RANGE", "nonfinite charge residual"));
                }
                worst = worst.max(ratio);
            }
            if worst <= 1e-8 {
                return Ok(Refined {
                    real_constants: zr,
                    imaginary_constants: zi,
                    real_charge: qr,
                    imaginary_charge: qi,
                    iterations: iteration,
                    worst_ratio: worst,
                });
            }
            if worst >= previous {
                return Err(error("ISLAND_UNRESOLVED", "island refinement stagnated"));
            }
            previous = worst;
            if factor.is_none() {
                let f = self.jacobian.full_piv_lu();
                check(start, limit)?;
                if (0..k).any(|i| !f.U()[(i, i)].is_finite() || f.U()[(i, i)] == 0.) {
                    return Err(error("ISLAND_UNRESOLVED", "singular island Jacobian"));
                }
                factor = Some(f);
            }
            let rhs = Mat::from_fn(k, 2, |i, j| if j == 0 { rr[i] } else { ri[i] });
            let solution = factor.as_ref().unwrap().solve(&rhs);
            check(start, limit)?;
            for i in 0..k {
                for part in 0..2 {
                    let target = if part == 0 { rr[i] } else { ri[i] };
                    let mut sum = Sum::default();
                    let mut scale = target.abs();
                    for j in 0..k {
                        let v = solution[(j, part)];
                        if !v.is_finite() {
                            return Err(error("ISLAND_UNRESOLVED", "singular island Jacobian"));
                        }
                        sum = sum.add(Sum::from(self.jacobian[(i, j)]).mul(v));
                        scale += (self.jacobian[(i, j)] * v).abs();
                    }
                    if !scale.is_finite() || !sum.hi.is_finite() || !sum.lo.is_finite() {
                        return Err(error("ISLAND_RANGE", "nonfinite Jacobian residual scale"));
                    }
                    if sum.sub(Sum::from(target)).value().abs()
                        > 1e-8 * scale.max(f64::MIN_POSITIVE)
                    {
                        return Err(error("ISLAND_UNRESOLVED", "island Jacobian solve residual"));
                    }
                }
            }
            for i in 0..k {
                zr[i] = zr[i].add(Sum::from(solution[(i, 0)]));
                zi[i] = zi[i].add(Sum::from(solution[(i, 1)]));
            }
        }
        Err(error(
            "ISLAND_UNRESOLVED",
            "island refinement iteration limit",
        ))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct TestField(bool);
    impl ElectricApply for TestField {
        fn apply(&self, v: &[f64]) -> Result<Vec<f64>> {
            Ok(v.iter().map(|x| if self.0 { *x } else { 0. }).collect())
        }
        fn energy_gram(&self, _: &[Vec<f64>]) -> Result<Vec<(usize, usize, f64)>> {
            Ok(vec![])
        }
        fn planned_bytes(&self) -> usize {
            0
        }
    }
    fn space(components: Vec<Vec<usize>>) -> Space {
        Space {
            mesh_nodes: vec![vec![]; 3],
            node_potential: Default::default(),
            cells: vec![],
            components,
            planned_bytes: 0,
        }
    }
    #[test]
    fn full_partition_and_refusals() {
        let b = AllocationBudget {
            max_bytes: 1_000_000,
            max_nodes: 10,
            max_tetrahedra: 10,
        };
        let s = space(vec![vec![0], vec![1], vec![2]]);
        let field = TestField(true);
        let r = IslandResponse::new(&s, &field, b, Instant::now(), Duration::from_secs(1)).unwrap();
        assert!(r.retained_bytes() > 0 && r.scratch_bytes() > 0);
        let x = r
            .refine(
                &[1., 2., 3.],
                &[0.; 3],
                &[0.; 3],
                &[0.; 3],
                1.,
                b,
                Instant::now(),
                Duration::from_secs(1),
            )
            .unwrap();
        assert!(x.worst_ratio <= 1e-8 && x.real_charge.iter().all(|v| v.value() == 0.));
        assert!(r
            .refine(
                &[f64::MAX; 3],
                &[f64::MAX; 3],
                &[0.; 3],
                &[0.; 3],
                1.,
                b,
                Instant::now(),
                Duration::from_secs(1)
            )
            .is_err());
        let zero = TestField(false);
        let singular =
            IslandResponse::new(&s, &zero, b, Instant::now(), Duration::from_secs(1)).unwrap();
        let inert = singular
            .refine(
                &[1.; 3],
                &[0.; 3],
                &[0.; 3],
                &[0.; 3],
                1.,
                b,
                Instant::now(),
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(inert.iterations, 0);
        assert!(singular
            .refine(
                &[1.; 3],
                &[0.; 3],
                &[1., 0., 0.],
                &[0.; 3],
                1.,
                b,
                Instant::now(),
                Duration::from_secs(1)
            )
            .is_err());
        let complex = r
            .refine(
                &[0.; 3],
                &[0.; 3],
                &[2., -4., 6.],
                &[4., 6., -2.],
                2.,
                b,
                Instant::now(),
                Duration::from_secs(1),
            )
            .unwrap();
        for i in 0..3 {
            assert_eq!(complex.real_charge[i].value(), [2., 3., -1.][i]);
            assert_eq!(complex.imaginary_charge[i].value(), [-1., 2., -3.][i]);
        }
        let grouped = space(vec![vec![0, 1], vec![2]]);
        let rr = IslandResponse::new(&grouped, &field, b, Instant::now(), Duration::from_secs(1))
            .unwrap();
        let state = rr
            .refine(
                &[1., -1., 2.],
                &[0.; 3],
                &[0.; 3],
                &[0.; 3],
                1.,
                b,
                Instant::now(),
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(state.real_charge[0].value(), 1.);
        assert_eq!(state.real_charge[1].value(), -1.);
        assert_eq!(state.real_charge[2].value(), 0.);
        for bad in [
            vec![vec![0, 0], vec![1, 2]],
            vec![vec![0], vec![1]],
            vec![vec![0], vec![1, 3]],
        ] {
            let s = space(bad);
            assert!(
                IslandResponse::new(&s, &field, b, Instant::now(), Duration::from_secs(1)).is_err()
            );
        }
        assert!(IslandResponse::new(&s, &field, b, Instant::now(), Duration::ZERO).is_err());
        assert!(IslandResponse::new(
            &s,
            &field,
            AllocationBudget { max_bytes: 1, ..b },
            Instant::now(),
            Duration::from_secs(1)
        )
        .is_err());
    }
}
