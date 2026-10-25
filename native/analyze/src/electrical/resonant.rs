//! Authored one-series-RLC-section per constant real orthonormal mode.
//! Conditional identification, never recognition/proof of an unknown sampled class.
use super::frequency::math::{self, Matrix, C};
use super::*;
#[cfg(test)]
#[path = "resonant_tests.rs"]
mod tests;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct HighFrequency {
    inverse_inductance_per_h: Vec<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Temporal {
    method: String,
    normalization_ohm: f64,
    mode_transform: Vec<Vec<f64>>,
    dc_slope_f: Vec<f64>,
    high_frequency: HighFrequency,
}
#[derive(Clone)]
struct Section {
    r: f64,
    l: f64,
    c: f64,
    condition: f64,
    samples_per_linewidth: f64,
}
fn check(start: Instant, budget: Option<&Budget>) -> Result<()> {
    super::deadline(start, budget, "Authored resonant Y fit")
}
/// Three-column real QR with two orthogonalization passes; no normal equations.
/// Frobenius(R)*Frobenius(R^-1) bounds the column-scaled 2-norm condition.
fn qr(rows: &[[f64; 3]], rhs: &[f64]) -> Result<([f64; 3], f64)> {
    let mut q = vec![vec![0.; rows.len()]; 3];
    let mut r = [[0.; 3]; 3];
    let mut norms = [0.; 3];
    for j in 0..3 {
        norms[j] = rows.iter().map(|v| v[j] * v[j]).sum::<f64>().sqrt();
        if !norms[j].is_finite() || norms[j] == 0. {
            return Err(fail("RESONANT_CONDITION", "Zero/nonfinite design column"));
        }
        let mut v: Vec<f64> = rows.iter().map(|v| v[j] / norms[j]).collect();
        for _ in 0..2 {
            for i in 0..j {
                let dot = v.iter().zip(&q[i]).map(|(a, b)| a * b).sum::<f64>();
                r[i][j] += dot;
                for (a, b) in v.iter_mut().zip(&q[i]) {
                    *a -= dot * b;
                }
            }
        }
        r[j][j] = v.iter().map(|v| v * v).sum::<f64>().sqrt();
        if !r[j][j].is_finite() || r[j][j] < 1e-14 {
            return Err(fail(
                "RESONANT_CONDITION",
                "Rank deficient normalized design",
            ));
        }
        q[j] = v.iter().map(|v| v / r[j][j]).collect();
    }
    let solve = |target: [f64; 3]| {
        let mut x = [0.; 3];
        for i in (0..3).rev() {
            x[i] = (target[i] - ((i + 1)..3).map(|j| r[i][j] * x[j]).sum::<f64>()) / r[i][i];
        }
        x
    };
    let mut inv_norm_sq = 0.;
    for j in 0..3 {
        let mut unit = [0.; 3];
        unit[j] = 1.;
        inv_norm_sq += solve(unit).iter().map(|v| v * v).sum::<f64>();
    }
    let condition = (r.iter().flatten().map(|v| v * v).sum::<f64>() * inv_norm_sq).sqrt();
    if !condition.is_finite() || condition > 1e10 {
        return Err(fail(
            "RESONANT_CONDITION",
            format!("Normalized QR condition bound {condition}"),
        ));
    }
    let target = std::array::from_fn(|i| q[i].iter().zip(rhs).map(|(a, b)| a * b).sum());
    let result = solve(target);
    Ok((std::array::from_fn(|j| result[j] / norms[j]), condition))
}
fn prediction(f: f64, sections: &[Section], u: &[Vec<f64>]) -> Matrix {
    let n = u.len();
    let mut out = math::zero(n, n);
    if f == 0. {
        return out;
    }
    let w = 2. * std::f64::consts::PI * f;
    for (m, s) in sections.iter().enumerate() {
        let imaginary = w * s.l - 1. / (w * s.c);
        let mag = s.r.hypot(imaginary);
        let y = C::new(s.r / mag / mag, -imaginary / mag / mag);
        for i in 0..n {
            for j in 0..n {
                out[i][j] = out[i][j] + y.scale(u[i][m] * u[j][m]);
            }
        }
    }
    out
}
pub fn fit_started(
    resource: &frequency::Resource,
    budget: Option<&Budget>,
    start: Instant,
) -> Result<relaxation::Fit> {
    check(start, budget)?;
    coupled::composition::remaining_budget(budget, coupled::composition::source_plan(resource))?;
    let n = resource.ports.len();
    if !(1..=2).contains(&n)
        || resource.data["format"] != "network-parameters"
        || resource.data["representation"] != "Y"
    {
        return Err(fail(
            "RESONANT_CLASS",
            "Only authored 1..2 port network-parameters Y is supported",
        ));
    }
    if resource.data["frequenciesHz"]
        .as_array()
        .is_none_or(|a| !(24..=4096).contains(&a.len()))
    {
        return Err(fail(
            "RESONANT_BUDGET",
            "24..4096 source rows required before normalization",
        ));
    }
    let t: Temporal = serde_json::from_value(resource.data["temporal"].clone())
        .map_err(|e| fail("RESONANT_MODEL", e))?;
    let u = &t.mode_transform;
    let norm = t.normalization_ohm;
    if t.method != "passive-resonant-y"
        || !norm.is_finite()
        || norm <= 0.
        || u.len() != n
        || u.iter()
            .any(|v| v.len() != n || v.iter().any(|v| !v.is_finite()))
        || t.dc_slope_f.len() != n
        || t.high_frequency.inverse_inductance_per_h.len() != n
        || t.dc_slope_f
            .iter()
            .chain(&t.high_frequency.inverse_inductance_per_h)
            .any(|v| !v.is_finite() || *v <= 0.)
    {
        return Err(fail("RESONANT_MODEL", "Explicit finite U, positive scalar normalization and positive per-mode closures required"));
    }
    for i in 0..n {
        for j in 0..n {
            let dot = (0..n).map(|k| u[k][i] * u[k][j]).sum::<f64>();
            if (dot - if i == j { 1. } else { 0. }).abs() > 1e-12 {
                return Err(fail(
                    "RESONANT_MODAL",
                    "U must be real orthonormal; no repair",
                ));
            }
        }
    }
    let (frequencies, original, _, _) = frequency::admittance_samples(resource)?;
    if frequencies[0] != 0.
        || frequencies[1] <= 0.
        || frequencies[1..]
            .windows(2)
            .any(|v| (v[1] / v[0]).log10() > 0.100000000001)
    {
        return Err(fail(
            "RESONANT_BAND",
            "Real DC and positive band with <=0.1 decade gaps required",
        ));
    }
    let omega_ref =
        2. * std::f64::consts::PI * (frequencies[1].sqrt() * frequencies.last().unwrap().sqrt());
    if !omega_ref.is_finite() || omega_ref <= 0. {
        return Err(fail("RESONANT_BAND", "Unresolved frequency normalization"));
    }
    let mut modal = Vec::with_capacity(frequencies.len());
    for (index, h) in original.iter().enumerate() {
        if index % 32 == 0 {
            check(start, budget)?;
        }
        let mut m = math::zero(n, n);
        for i in 0..n {
            for j in 0..n {
                for a in 0..n {
                    for b in 0..n {
                        m[i][j] = m[i][j] + h[a][b].scale(norm * u[a][i] * u[b][j]);
                    }
                }
            }
        }
        let magnitude = h
            .iter()
            .flatten()
            .map(|z| z.abs() * norm)
            .fold(0., f64::max);
        for i in 0..n {
            for j in 0..n {
                if (h[i][j] - h[j][i]).abs() * norm > 1e-9 * magnitude + 1e-10
                    || i != j && m[i][j].abs() > 1e-9 * magnitude + 1e-10
                {
                    return Err(fail(
                        "RESONANT_MODAL",
                        "Nonreciprocal or frequency-dependent/noncommuting modes",
                    ));
                }
                if index == 0 && m[i][j].abs() > 1e-10 {
                    return Err(fail("RESONANT_CLOSURE", "DC must be zero"));
                }
            }
        }
        modal.push(m);
    }
    let training: Vec<usize> = (1..frequencies.len()).filter(|i| i % 2 == 1).collect();
    let mut sections = vec![];
    for m in 0..n {
        check(start, budget)?;
        let mut rows = vec![];
        let mut rhs = vec![];
        for &i in &training {
            let q = 2. * std::f64::consts::PI * frequencies[i] / omega_ref;
            let h = modal[i][m][m];
            let mag = h.abs();
            if !mag.is_finite() || mag == 0. {
                return Err(fail(
                    "RESONANT_FIT",
                    "Unresolved positive-frequency modal admittance",
                ));
            }
            rows.push([1., 0., 0.]);
            rhs.push(h.real / mag / mag);
            rows.push([0., q, -1. / q]);
            rhs.push(-h.imaginary / mag / mag);
        }
        let (x, condition) = qr(&rows, &rhs)?;
        if x.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err(fail(
                "RESONANT_PASSIVITY",
                "Nonpositive structural R/L/C; no clipping",
            ));
        }
        let mut section = Section {
            r: norm * x[0],
            l: norm * x[1] / omega_ref,
            c: 1. / (omega_ref * norm * x[2]),
            condition,
            samples_per_linewidth: 0.,
        };
        if [section.r, section.l, section.c]
            .iter()
            .any(|v| !v.is_finite() || *v <= 0.)
        {
            return Err(fail("RESONANT_FIT", "Unresolved dimensional conversion"));
        }
        if [
            1. / section.l,
            section.r / section.l,
            1. / section.l / section.c,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.)
        {
            return Err(fail(
                "RESONANT_FIT",
                "Unresolved positive dimensional k/a/b",
            ));
        }
        for (got, want) in [
            (section.c, t.dc_slope_f[m]),
            (1. / section.l, t.high_frequency.inverse_inductance_per_h[m]),
        ] {
            if (got / want - 1.).abs() > 0.0005 {
                return Err(fail(
                    "RESONANT_CLOSURE",
                    "DC slope or high-frequency inverse inductance mismatch",
                ));
            }
        }
        let f0 = 1. / (2. * std::f64::consts::PI * section.l.sqrt() * section.c.sqrt());
        let width = section.r / (2. * std::f64::consts::PI * section.l);
        let band: Vec<f64> = training.iter().map(|i| frequencies[*i]).collect();
        let j = band.partition_point(|f| *f < f0);
        if j == 0 || j == band.len() {
            return Err(fail(
                "RESONANT_BAND",
                "Resonance outside positive training band",
            ));
        }
        section.samples_per_linewidth = width / (band[j] - band[j - 1]);
        if !section.samples_per_linewidth.is_finite() || section.samples_per_linewidth < 8. {
            return Err(fail(
                "RESONANT_RESOLUTION",
                "At least eight training samples per fitted linewidth required",
            ));
        }
        sections.push(section);
    }
    let predicted: Vec<Matrix> = frequencies
        .iter()
        .map(|f| prediction(*f, &sections, u))
        .collect();
    let mut training_error: f64 = 0.;
    let mut holdout_error: f64 = 0.;
    for (index, (got, want)) in predicted.iter().zip(&original).enumerate() {
        for i in 0..n {
            for j in 0..n {
                let diff = (got[i][j] - want[i][j]).abs() * norm;
                let size = want[i][j].abs() * norm;
                if !diff.is_finite() || diff > 0.005 * size + 1e-8 {
                    return Err(fail(
                        "RESONANT_RESIDUAL",
                        "Full matrix training/holdout gate failed",
                    ));
                }
                let error = diff / (size + 2e-6);
                if index % 2 == 1 {
                    training_error = training_error.max(error);
                } else {
                    holdout_error = holdout_error.max(error);
                }
            }
        }
    }
    let mut model = coupled::Model {
        format: "coupled-rlc".into(),
        reference_topology: "grounded".into(),
        potential_nodes: resource
            .ports
            .iter()
            .map(|p| format!("port:{p}"))
            .chain((0..n).map(|i| format!("resonant-capacitor:{i}")))
            .collect(),
        current_dofs: (0..n).map(|i| format!("resonant-branch:{i}")).collect(),
        incidence: vec![],
        port_incidence: vec![],
        resistance_ohm: vec![],
        inductance_h: vec![],
        capacitance_f: vec![],
        structural_gauge_modes: vec![],
        conserved_potential_modes: vec![],
        port_terminals: None,
    };
    for m in 0..n {
        model.port_incidence.push((m, m, 1.));
        model.incidence.push((n + m, m, -1.));
        model.resistance_ohm.push((m, m, sections[m].r));
        model.inductance_h.push((m, m, sections[m].l));
        model.capacitance_f.push((n + m, n + m, sections[m].c));
        for i in 0..n {
            if u[i][m] != 0. {
                model.incidence.push((i, m, u[i][m]));
            }
        }
    }
    check(start, budget)?;
    Ok(relaxation::Fit {
        model,
        report: json!({"status":"complete","method":"passive-resonant-y","sourceResourceId":resource.id,
        "validity":{"status":"conditional","reason":"Authored one-series-RLC-section per constant mode plus finite samples/closure; unknown intersample/extraband resonances not excluded"},
        "frequenciesHz":frequencies,"sourceAdmittanceS":original,"fittedAdmittanceS":predicted,"normalizationOhm":norm,"omegaReferencePerS":omega_ref,"modeTransform":u,
        "trainingMaximum":training_error,"holdoutMaximum":holdout_error,"orderPerMode":2,
        "coefficients":sections.iter().map(|s|json!({"resistanceOhm":s.r,"inductanceH":s.l,"capacitanceF":s.c,"kPerH":1./s.l,"aPerS":s.r/s.l,"bPerS2":1./(s.l*s.c),"conditionUpperBound":s.condition,"trainingSamplesPerLinewidth":s.samples_per_linewidth})).collect::<Vec<_>>(),
        "passivity":"Positive physical R/L/C, power-preserving real orthonormal U; no pole reflection or data repair","elapsedMs":start.elapsed().as_secs_f64()*1000.}),
    })
}
