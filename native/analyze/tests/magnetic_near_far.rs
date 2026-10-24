//! Focused near/far coverage, physical absolute bounds and refusal tests.
use pcb_analyze::extraction::local::{
    far::Moments,
    near_far::{Budget, ExactCache, Operator, Plan},
    prism::Prism,
};
use std::{collections::BTreeSet, time::Duration};
fn budget() -> Budget {
    Budget {
        max_bytes: 512 * 1024 * 1024,
        max_nodes: 8192,
        max_blocks: 100000,
        max_pairs: 6000000,
        max_wall: Duration::from_secs(60),
    }
}
fn p(x: f64, w: f64) -> Prism {
    Prism::new("test".into(), [[x, 0.], [x + w, 0.], [x, w]], [0., 35e-6]).unwrap()
}
fn pairs(p: &Plan, far: bool) -> BTreeSet<(usize, usize)> {
    let mut s = BTreeSet::new();
    p.visit_pairs(far, |i, j| {
        assert!(s.insert((i.min(j), i.max(j))));
        Ok(())
    })
    .unwrap();
    s
}
#[test]
fn complete_partition_refinement_and_full_support_separation() {
    let ps = vec![
        p(0., 0.0005),
        p(0.0005, 0.0005),
        p(0.003, 0.0005),
        p(0.004, 0.0005),
    ];
    let mut previous = None;
    for theta in [0.7, 0.4, 0.2] {
        let plan = Plan::new(&ps, theta, 1, budget()).unwrap();
        let n = pairs(&plan, false);
        let f = pairs(&plan, true);
        assert!(n.is_disjoint(&f));
        assert_eq!(n.len() + f.len(), 10);
        for i in 0..4 {
            assert!(n.contains(&(i, i)));
        }
        for &(i, j) in &f {
            assert!(plan.moments[i].pair(&plan.moments[j]).unwrap().radius_ratio < 1.);
        }
        if let Some(old) = previous {
            assert!(f.is_subset(&old));
        }
        previous = Some(f);
    }
}
#[test]
fn geometric_movement_rotation_scale_and_slender_moments() {
    let a = p(0., 0.0005);
    let b = p(0.003, 0.0005);
    let base = Moments::new(&a)
        .unwrap()
        .pair(&Moments::new(&b).unwrap())
        .unwrap();
    let transform = |p: &Prism| {
        Prism::new(
            "rigid".into(),
            p.triangle_m.map(|v| [-2. * v[1] + 0.01, 2. * v[0] - 0.02]),
            p.z_m.map(|z| 2. * z + 0.003),
        )
        .unwrap()
    };
    let value = Moments::new(&transform(&a))
        .unwrap()
        .pair(&Moments::new(&transform(&b)).unwrap())
        .unwrap();
    assert!((value.integral_m5 / (32. * base.integral_m5) - 1.).abs() < 1e-12);
    assert!((value.absolute_remainder_m5 / (32. * base.absolute_remainder_m5) - 1.).abs() < 1e-11);
    let far = Plan::new(&[a.clone(), b], 0.4, 1, budget()).unwrap();
    let close = Plan::new(&[a.clone(), p(0.0005, 0.0005)], 0.4, 1, budget()).unwrap();
    assert!(far.far_pairs > close.far_pairs);
    assert!(Moments::new(&a)
        .unwrap()
        .pair(&Moments::new(&a).unwrap())
        .is_err());
    let slender = Prism::new(
        "slender".into(),
        [[0., 0.], [0.01, 0.], [0.01, 1e-9]],
        [0., 1e-5],
    )
    .unwrap();
    assert!(
        Plan::new(&[slender, p(0.05, 0.001)], 0.7, 1, budget())
            .unwrap()
            .far_pairs
            > 0
    );
}
#[test]
fn aggregate_absolute_bound_and_exact_null_mode() {
    let ps = vec![p(0., 0.0005), p(0.003, 0.0005)];
    let density = vec![
        vec![[1e7, 2e7, 0.], [1e7, 2e7, 0.]],
        vec![[-2e7, 1e7, 0.], [-2e7, 1e7, 0.]],
    ];
    let mut cache = ExactCache::new(ps, 12, budget()).unwrap();
    let exact = cache.dense().unwrap();
    let op = Operator::new(&mut cache, density.clone(), 0.4, 1).unwrap();
    assert_eq!(op.plan.far_pairs, 1);
    let null = op.apply(&[1., -1.]).unwrap();
    assert_eq!(null.energy_j, 0.);
    assert_eq!(null.absolute_truncation_energy_bound_j, 0.);
    for q in [[1., 0.], [-0.3, 0.7]] {
        let a = op.apply(&q).unwrap();
        let mut energy = 0.;
        for i in 0..2 {
            for j in 0..2 {
                let ji = density[i][0].map(|x| x * (q[0] + q[1]));
                let jj = density[j][0].map(|x| x * (q[0] + q[1]));
                energy += 0.5e-7 * exact[i][j] * ji.iter().zip(jj).map(|(a, b)| a * b).sum::<f64>();
            }
        }
        // Independent near quadrature error is not part of the analytic truncation.
        assert!(
            (a.energy_j - energy).abs()
                <= a.absolute_truncation_energy_bound_j + 1e-12 * energy.abs()
        );
    }
    assert!(op.apply(&[f64::NAN, 0.]).is_err());
    assert_eq!(cache.evaluations, 4);
}
#[test]
fn admission_and_input_negatives() {
    let ps = vec![p(0., 0.0005), p(0.003, 0.0005)];
    for theta in [0., 1., f64::NAN] {
        assert!(Plan::new(&ps, theta, 1, budget()).is_err());
    }
    assert!(Plan::new(&ps, 0.4, 0, budget()).is_err());
    for kind in 0..5 {
        let mut b = budget();
        match kind {
            0 => b.max_bytes = 1,
            1 => b.max_nodes = 0,
            2 => b.max_blocks = 0,
            3 => b.max_pairs = 0,
            _ => b.max_wall = Duration::ZERO,
        };
        assert!(Plan::new(&ps, 0.4, 1, b).is_err());
    }
    assert!(ExactCache::new(vec![ps[0].clone(); 65], 12, budget()).is_err());
    let mut cache = ExactCache::new(ps, 12, budget()).unwrap();
    assert!(Operator::new(&mut cache, vec![], 0.4, 1).is_err());
    assert!(cache.get(999, 0).is_err());
}
#[test]
fn independent_mixed_slender_far_reference() {
    use pcb_analyze::extraction::local::prism::{smooth_volume_integral, GaussRule};
    let a = Prism::new(
        "slender-reference".into(),
        [[0., 0.], [0.01, 0.], [0.01, 1e-9]],
        [0., 1e-5],
    )
    .unwrap();
    let transform = |p: &Prism| {
        Prism::new(
            "rotated-scaled".into(),
            p.triangle_m.map(|v| [-2. * v[1], 2. * v[0]]),
            p.z_m.map(|z| 2. * z),
        )
        .unwrap()
    };
    let mut previous_bound = f64::INFINITY;
    for x in [0.03, 0.05, 0.1] {
        let b = p(x, 0.0005);
        let f = Moments::new(&a)
            .unwrap()
            .pair(&Moments::new(&b).unwrap())
            .unwrap();
        let r6 = smooth_volume_integral(&a, &b, &GaussRule::new(6).unwrap()).unwrap();
        let r8 = smooth_volume_integral(&a, &b, &GaussRule::new(8).unwrap()).unwrap();
        let reference_change = (r8 - r6).abs();
        assert!(reference_change / r8 < 1e-7);
        assert!((f.integral_m5 - r8).abs() <= f.absolute_remainder_m5 + 2. * reference_change);
        assert!(f.absolute_remainder_m5 < previous_bound);
        previous_bound = f.absolute_remainder_m5;
        let rotated = Moments::new(&transform(&a))
            .unwrap()
            .pair(&Moments::new(&transform(&b)).unwrap())
            .unwrap();
        let rotated_reference =
            smooth_volume_integral(&transform(&a), &transform(&b), &GaussRule::new(8).unwrap())
                .unwrap();
        assert!((rotated_reference / (32. * r8) - 1.).abs() < 1e-12);
        assert!((rotated.integral_m5 / (32. * f.integral_m5) - 1.).abs() < 1e-12);
        println!(
            "{}",
            serde_json::json!({"xM":x,"momentM5":f.integral_m5,"direct6DM5":r8,"lastReferenceChangeM5":reference_change,"analyticRemainderM5":f.absolute_remainder_m5,"radiusRatio":f.radius_ratio,"rotationScaleReferenceM5":rotated_reference})
        );
    }
    // Bounding boxes separate, but moment balls do not: mandatory far refusal.
    let close = p(0.011, 0.0005);
    assert!(smooth_volume_integral(&a, &close, &GaussRule::new(8).unwrap()).is_ok());
    assert!(Moments::new(&a)
        .unwrap()
        .pair(&Moments::new(&close).unwrap())
        .is_err());
    assert_eq!(
        Plan::new(&[a, close], 0.7, 1, budget()).unwrap().far_pairs,
        0
    );
}
