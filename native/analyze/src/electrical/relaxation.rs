//! Conditional sampled-to-time fit in a deliberately restricted positive-real class.
//! Stable poles and nonnegative physical RC strengths are constraints, not repairs.
use super::frequency::math::{self, Matrix, C};
use super::*;
use std::collections::BTreeMap;
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HighFrequency {
    conductance_s: Vec<f64>,
    capacitance_f: Vec<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Temporal {
    method: String,
    mode_transform: Option<Vec<Vec<f64>>>,
    normalization_ohm: Option<Value>,
    high_frequency: HighFrequency,
}
pub struct Fit {
    pub model: coupled::Model,
    pub report: Value,
}
struct Mode {
    g0: f64,
    c: f64,
    total: f64,
    poles: Vec<f64>,
    weights: Vec<f64>,
    kkt: f64,
    iterations: usize,
}
fn checkpoint(start: Instant, budget: Option<&Budget>) -> Result<()> {
    super::deadline(start, budget, "Passive relaxation fit")
}
fn basis(w: f64, p: f64) -> C {
    // Dimensionless ratio avoids squaring large dimensional frequencies.
    let q = w / p;
    if q <= 1. {
        C::new(q * q / (1. + q * q), q / (1. + q * q))
    } else {
        let r = p / w;
        C::new(1. / (1. + r * r), r / (1. + r * r))
    }
}
fn predict(mode: &Mode, f: f64) -> C {
    let w = 2. * std::f64::consts::PI * f;
    mode.poles
        .iter()
        .zip(&mode.weights)
        .fold(C::new(mode.g0, w * mode.c), |a, (p, h)| {
            a + basis(w, *p).scale(mode.total * h)
        })
}
/// Equality-constrained active-set NNLS on sum(x)=1. Every accepted iterate is
/// feasible; small KKT systems use the existing checked sparse complex LU.
fn simplex_fit(
    gram: &[Vec<f64>],
    target: &[f64],
    start: Instant,
    budget: Option<&Budget>,
) -> Result<(Vec<f64>, f64, usize)> {
    let n = target.len();
    let best = (0..n)
        .min_by(|&a, &b| (0.5 * gram[a][a] - target[a]).total_cmp(&(0.5 * gram[b][b] - target[b])))
        .unwrap();
    let mut x = vec![0.; n];
    x[best] = 1.;
    let mut active = vec![best];
    let mut quality = frequency::Quality::default();
    let scale = 1. + target.iter().map(|v| v.abs()).fold(0., f64::max);
    for iteration in 0..5000 {
        if iteration % 32 == 0 {
            checkpoint(start, budget)?;
        }
        let gradient: Vec<f64> = gram
            .iter()
            .zip(target)
            .map(|(row, c)| row.iter().zip(&x).map(|(g, v)| g * v).sum::<f64>() - c)
            .collect();
        let lambda = gradient.iter().zip(&x).map(|(g, v)| g * v).sum::<f64>();
        let kkt = gradient
            .iter()
            .zip(&x)
            .map(|(g, v)| {
                if *v > 1e-10 {
                    (g - lambda).abs()
                } else {
                    (lambda - g).max(0.)
                }
            })
            .fold(0., f64::max)
            / scale;
        if kkt <= 1e-9 {
            return Ok((x, kkt, iteration));
        }
        let next = (0..n)
            .filter(|i| !active.contains(i))
            .min_by(|&a, &b| gradient[a].total_cmp(&gradient[b]));
        if let Some(next) = next {
            active.push(next);
        } else {
            return Err(fail("TEMPORAL_FIT", "No feasible improving NNLS direction"));
        }
        let mut feasible = false;
        for _ in 0..2 * n {
            let k = active.len();
            let mut a = math::zero(k + 1, k + 1);
            let mut b = math::zero(k + 1, 1);
            for i in 0..k {
                b[i][0].real = target[active[i]];
                for j in 0..k {
                    a[i][j].real = gram[active[i]][active[j]];
                }
                a[i][k].real = 1.;
                a[k][i].real = 1.;
            }
            b[k][0].real = 1.;
            let solution = math::solve(&a, &b, &mut quality)
                .map_err(|e| fail("TEMPORAL_FIT", format!("Unresolved constrained fit: {e}")))?;
            let mut z = vec![0.; n];
            for (i, index) in active.iter().enumerate() {
                z[*index] = solution[i][0].real;
            }
            if active.iter().all(|i| z[*i] > 0.) {
                x = z;
                feasible = true;
                break;
            }
            let alpha = active
                .iter()
                .filter(|i| z[**i] <= 0.)
                .map(|i| x[*i] / (x[*i] - z[*i]))
                .fold(1., f64::min);
            if !alpha.is_finite() {
                return Err(fail("TEMPORAL_FIT", "Nonfinite NNLS feasibility step"));
            }
            for i in 0..n {
                x[i] += alpha * (z[i] - x[i]);
                if x[i].abs() < 1e-14 {
                    x[i] = 0.;
                }
            }
            active.retain(|i| x[*i] > 0.);
            if active.is_empty() {
                return Err(fail("TEMPORAL_FIT", "Lost constrained simplex feasibility"));
            }
        }
        if !feasible {
            return Err(fail("TEMPORAL_FIT", "NNLS active-set inner budget"));
        }
    }
    Err(fail("TEMPORAL_FIT", "NNLS iteration budget"))
}
fn fit_mode(
    poles: Vec<f64>,
    frequencies: &[f64],
    values: &[C],
    g0: f64,
    c: f64,
    total: f64,
    start: Instant,
    budget: Option<&Budget>,
) -> Result<Mode> {
    let n = poles.len();
    let mut gram = vec![vec![0.; n]; n];
    let mut target = vec![0.; n];
    if total == 0. {
        return Ok(Mode {
            g0,
            c,
            total,
            poles,
            weights: vec![0.; n],
            kkt: 0.,
            iterations: 0,
        });
    }
    for (sample, (&f, value)) in frequencies.iter().zip(values).enumerate() {
        if sample % 2 != 0 {
            continue;
        } // Interlaced holdout fixed before fitting.
        if sample % 64 == 0 {
            checkpoint(start, budget)?;
        }
        let w = 2. * std::f64::consts::PI * f;
        let y = (*value - C::new(g0, w * c)).scale(1. / total);
        let a: Vec<_> = poles.iter().map(|p| basis(w, *p)).collect();
        for i in 0..n {
            target[i] += a[i].real * y.real + a[i].imaginary * y.imaginary;
            for j in 0..n {
                gram[i][j] += a[i].real * a[j].real + a[i].imaginary * a[j].imaginary;
            }
        }
    }
    let (weights, kkt, iterations) = simplex_fit(&gram, &target, start, budget)?;
    if weights.iter().any(|v| !v.is_finite() || *v < 0.)
        || (weights.iter().sum::<f64>() - 1.).abs() > 1e-10
    {
        return Err(fail(
            "TEMPORAL_FIT",
            "Unverified passive/equality constraints",
        ));
    }
    Ok(Mode {
        g0,
        c,
        total,
        poles,
        weights,
        kkt,
        iterations,
    })
}
fn refine(mode: &Mode) -> Vec<f64> {
    let mut intervals: Vec<_> = mode
        .poles
        .windows(2)
        .enumerate()
        .map(|(i, p)| {
            let gap = (p[1] / p[0]).ln();
            (
                (mode.weights[i] + mode.weights[i + 1]) * gap * gap,
                gap,
                p[0] * (p[1] / p[0]).sqrt(),
            )
        })
        .collect();
    intervals.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.total_cmp(&a.1)));
    let mut poles = mode.poles.clone();
    poles.extend(intervals.iter().take(4).map(|v| v.2));
    poles.sort_by(f64::total_cmp);
    poles
}
fn matrices(modes: &[Mode], u: &[Vec<f64>], frequencies: &[f64]) -> Vec<Matrix> {
    let n = modes.len();
    frequencies
        .iter()
        .map(|f| {
            let diagonal: Vec<_> = modes.iter().map(|m| predict(m, *f)).collect();
            (0..n)
                .map(|i| {
                    (0..n)
                        .map(|j| {
                            (0..n).fold(C::default(), |z, k| {
                                z + diagonal[k].scale(u[i][k] * u[j][k])
                            })
                        })
                        .collect()
                })
                .collect()
        })
        .collect()
}
fn normalized_error(a: &Matrix, b: &Matrix, refs: &[f64]) -> f64 {
    a.iter()
        .zip(b)
        .enumerate()
        .flat_map(|(i, (a, b))| {
            a.iter()
                .zip(b)
                .enumerate()
                .map(move |(j, (a, b))| (*a - *b).abs() * (refs[i] * refs[j]).sqrt())
        })
        .fold(0., f64::max)
}
fn model(modes: &[Mode], u: &[Vec<f64>], ports: &[String]) -> coupled::Model {
    let n = ports.len();
    let mut potential_nodes: Vec<_> = ports.iter().map(|p| format!("port:{p}")).collect();
    let mut current_dofs = vec![];
    let mut incidence = vec![];
    let mut resistance_ohm = vec![];
    let mut capacitance: BTreeMap<(usize, usize), f64> = BTreeMap::new();
    let mut candidates = vec![];
    for (m, mode) in modes.iter().enumerate() {
        for i in 0..n {
            for j in 0..n {
                *capacitance.entry((i, j)).or_default() += mode.c * u[i][m] * u[j][m];
            }
        }
        if mode.g0 > 0. {
            let branch = current_dofs.len();
            current_dofs.push(format!("mode:{m}:g0"));
            resistance_ohm.push((branch, branch, 1. / mode.g0));
            for (i, row) in u.iter().enumerate() {
                if row[m] != 0. {
                    incidence.push((i, branch, row[m]));
                }
            }
        }
        for (k, (&pole, &weight)) in mode.poles.iter().zip(&mode.weights).enumerate() {
            let h = mode.total * weight;
            if h == 0. {
                continue;
            }
            let node = potential_nodes.len();
            potential_nodes.push(format!("mode:{m}:cap:{k}"));
            let branch = current_dofs.len();
            current_dofs.push(format!("mode:{m}:rc:{k}"));
            resistance_ohm.push((branch, branch, 1. / h));
            capacitance.insert((node, node), h / pole);
            for (i, row) in u.iter().enumerate() {
                if row[m] != 0. {
                    incidence.push((i, branch, row[m]));
                }
            }
            incidence.push((node, branch, -1.));
        }
    }
    for (m, mode) in modes.iter().enumerate() {
        if mode.g0 == 0. {
            let mut v = vec![0.; potential_nodes.len()];
            for i in 0..n {
                v[i] = u[i][m];
            }
            for (i, name) in potential_nodes.iter().enumerate().skip(n) {
                if name.starts_with(&format!("mode:{m}:cap:")) {
                    v[i] = 1.;
                }
            }
            candidates.push(v);
        }
    }
    coupled::Model {
        format: "coupled-rlc".into(),
        reference_topology: "grounded".into(),
        potential_nodes,
        current_dofs,
        incidence,
        port_incidence: (0..n).map(|i| (i, i, 1.)).collect(),
        resistance_ohm,
        inductance_h: vec![],
        capacitance_f: capacitance
            .into_iter()
            .map(|((i, j), v)| (i, j, v))
            .collect(),
        conserved_potential_modes: candidates,
        port_terminals: None,
    }
}
pub fn fit(resource: &frequency::Resource, budget: Option<&Budget>) -> Result<Fit> {
    fit_started(resource, budget, Instant::now())
}
pub(super) fn fit_started(
    resource: &frequency::Resource,
    budget: Option<&Budget>,
    start: Instant,
) -> Result<Fit> {
    checkpoint(start, budget)?;
    coupled::composition::remaining_budget(budget, coupled::composition::source_plan(resource))?;
    let n = resource.ports.len();
    if n == 0
        || n > 4
        || resource
            .data
            .get("frequenciesHz")
            .and_then(Value::as_array)
            .is_some_and(|f| f.len() > 4096)
    {
        return Err(fail(
            "TEMPORAL_BUDGET",
            "Relaxation fit permits 1..4 ports and <=4096 frequencies",
        ));
    }
    let temporal: Temporal =
        serde_json::from_value(resource.data.get("temporal").cloned().ok_or_else(|| {
            fail(
                "TEMPORAL_MODEL_REQUIRED",
                "Sampled time needs explicit relaxation/DC/high-frequency closure",
            )
        })?)
        .map_err(|e| fail("TEMPORAL_MODEL", e))?;
    if temporal.method != "passive-relaxation"
        || temporal.high_frequency.conductance_s.len() != n
        || temporal.high_frequency.capacitance_f.len() != n
        || temporal
            .high_frequency
            .conductance_s
            .iter()
            .chain(&temporal.high_frequency.capacitance_f)
            .any(|v| !v.is_finite() || *v < 0.)
    {
        return Err(fail(
            "TEMPORAL_MODEL",
            "Invalid passive high-frequency conducting remainder/capacitance",
        ));
    }
    let u = temporal.mode_transform.unwrap_or_else(|| {
        (0..n)
            .map(|i| (0..n).map(|j| if i == j { 1. } else { 0. }).collect())
            .collect()
    });
    if u.len() != n
        || u.iter()
            .any(|v| v.len() != n || v.iter().any(|x| !x.is_finite()))
        || (0..n).any(|i| {
            (0..n).any(|j| {
                ((0..n).map(|k| u[k][i] * u[k][j]).sum::<f64>() - if i == j { 1. } else { 0. })
                    .abs()
                    > 1e-10
            })
        })
    {
        return Err(fail(
            "TEMPORAL_MODAL",
            "Constant real orthonormal power-preserving U required",
        ));
    }
    let (frequencies, original, source_refs, representation) =
        frequency::admittance_samples(resource)?;
    checkpoint(start, budget)?;
    if frequencies.len() < 24
        || frequencies.len() > 4096
        || frequencies[0] != 0.
        || frequencies[1] <= 0.
        || frequencies[1..]
            .windows(2)
            .any(|v| (v[1] / v[0]).log10() > 0.100000000001)
    {
        return Err(fail(
            "TEMPORAL_BAND",
            "Real DC, sufficient dense positive band and <=0.1-decade gaps required",
        ));
    }
    let refs = if temporal.normalization_ohm.is_none() && representation == "S" {
        source_refs
    } else {
        let value = temporal.normalization_ohm.ok_or_else(|| {
            fail(
                "TEMPORAL_REFERENCE",
                "Y/Z fit needs authored normalizationOhm",
            )
        })?;
        if let Some(v) = value.as_f64() {
            vec![v; n]
        } else if let Some(values) = value.as_object() {
            if values.len() != n {
                return Err(fail(
                    "TEMPORAL_REFERENCE",
                    "Normalization keys must match source ports",
                ));
            }
            resource
                .ports
                .iter()
                .map(|p| values.get(p).and_then(Value::as_f64).unwrap_or(f64::NAN))
                .collect()
        } else {
            return Err(fail("TEMPORAL_REFERENCE", "Invalid normalizationOhm"));
        }
    };
    if refs.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err(fail(
            "TEMPORAL_REFERENCE",
            "Positive finite references required",
        ));
    }
    let modal: Vec<Matrix> = original
        .iter()
        .map(|matrix| {
            (0..n)
                .map(|i| {
                    (0..n)
                        .map(|j| {
                            (0..n).fold(C::default(), |sum, a| {
                                (0..n)
                                    .fold(sum, |sum, b| sum + matrix[a][b].scale(u[a][i] * u[b][j]))
                            })
                        })
                        .collect()
                })
                .collect()
        })
        .collect();
    for (matrix, source) in modal.iter().zip(&original) {
        let mut rebuilt = math::zero(n, n);
        for i in 0..n {
            for j in 0..n {
                for k in 0..n {
                    rebuilt[i][j] = rebuilt[i][j] + matrix[k][k].scale(u[i][k] * u[j][k]);
                }
            }
        }
        if normalized_error(&rebuilt, source, &refs) > 1e-9 {
            return Err(fail(
                "TEMPORAL_MODAL",
                "Frequency-dependent/noncommuting/nonreciprocal modes outside relaxation class",
            ));
        }
    }
    let low = 10. * 2. * std::f64::consts::PI * frequencies[1];
    let high = 2. * std::f64::consts::PI * frequencies.last().unwrap() / 10.;
    if !low.is_finite() || !high.is_finite() || low >= high || !(high / low).is_finite() {
        return Err(fail(
            "TEMPORAL_BAND",
            "Band cannot bracket passive relaxation poles with required margin",
        ));
    }
    let mut modes = vec![];
    for m in 0..n {
        let g0 = modal[0][m][m].real;
        let c = temporal.high_frequency.capacitance_f[m];
        let total = temporal.high_frequency.conductance_s[m] - g0;
        if g0 < 0.
            || total < 0.
            || modal[0][m][m].imaginary.abs() * refs.iter().copied().fold(0., f64::max) > 1e-12
        {
            return Err(fail(
                "TEMPORAL_DC",
                "Incompatible known DC or high-frequency conducting remainder",
            ));
        }
        modes.push(Mode {
            g0,
            c,
            total,
            poles: (0..8)
                .map(|i| low * (high / low).powf(i as f64 / 7.))
                .collect(),
            weights: vec![0.; 8],
            kkt: 0.,
            iterations: 0,
        });
    }
    let mut history = vec![];
    let mut previous: Option<Vec<Matrix>> = None;
    let mut accepted = None;
    for order in (8..=64).step_by(4) {
        checkpoint(start, budget)?;
        for m in 0..n {
            let values: Vec<_> = modal.iter().map(|matrix| matrix[m][m]).collect();
            let old = &modes[m];
            modes[m] = fit_mode(
                old.poles.clone(),
                &frequencies,
                &values,
                old.g0,
                old.c,
                old.total,
                start,
                budget,
            )?;
        }
        if [8, 16, 32, 64].contains(&order) {
            let predicted = matrices(&modes, &u, &frequencies);
            let mut training: f64 = 0.;
            let mut holdout: f64 = 0.;
            for (i, (a, b)) in predicted.iter().zip(&original).enumerate() {
                let error = normalized_error(a, b, &refs);
                if i % 2 == 0 {
                    training = training.max(error);
                } else {
                    holdout = holdout.max(error);
                }
            }
            let change = previous.as_ref().map(|p| {
                p.iter()
                    .zip(&predicted)
                    .map(|(a, b)| normalized_error(a, b, &refs))
                    .fold(0., f64::max)
            });
            history.push(json!({"dictionaryOrder":order,"trainingMaximum":training,"holdoutMaximum":holdout,"orderMaximumChange":change,"kkt":modes.iter().map(|m|m.kkt).collect::<Vec<_>>(),"iterations":modes.iter().map(|m|m.iterations).collect::<Vec<_>>() }));
            if training <= 0.005 + 1e-9
                && holdout <= 0.005 + 1e-9
                && change.is_some_and(|e| e <= 0.001)
            {
                accepted = Some(predicted);
                break;
            }
            previous = Some(predicted);
        }
        if order < 64 {
            for mode in &mut modes {
                mode.poles = refine(mode);
            }
        }
    }
    let predicted = accepted.ok_or_else(|| {
        fail(
            "TEMPORAL_FIT",
            format!(
                "Model/holdout/order gate failed within 64-pole cap: {}",
                json!(history)
            ),
        )
    })?;
    let model = model(&modes, &u, &resource.ports);
    checkpoint(start, budget)?;
    Ok(Fit {
        report: json!({"status":"complete","validity":{"status":"conditional","reason":"Finite sampled data plus authored passive relaxation/high-frequency closure; intersample/extraband original behavior is not proven"},"sourceResourceId":resource.id,"frequenciesHz":frequencies,"sourceAdmittanceS":original,"fittedAdmittanceS":predicted,"normalizationOhm":refs,"modeTransform":u,"refinement":history,"coefficients":modes.iter().map(|m|json!({"dcConductanceS":m.g0,"highFrequencyCapacitanceF":m.c,"highFrequencyConductingRemainderS":m.g0+m.total,"polesPerS":m.poles,"conductanceStrengthsS":m.weights.iter().map(|w|w*m.total).collect::<Vec<_>>()})).collect::<Vec<_>>(),"passivity":"Structural nonnegative series-RC branches + nonnegative shunt G/C; not pointwise-only certification","causality":"Strict negative real poles; no time advance or inferred delay","sourceLimitations":"Conditional between samples; unseen narrow resonances and unknown out-of-band behavior are not excluded by a regular grid","elapsedMs":start.elapsed().as_secs_f64()*1000.}),
        model,
    })
}
