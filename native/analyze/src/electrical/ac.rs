//! Explicit peak phasors. Independent impedance test right-hand sides stay separate.
use super::*;
use std::collections::{BTreeMap, BTreeSet};

pub fn validate(r: &Request) -> Result<()> {
    if r.ac_sources.is_empty() {
        return Ok(());
    }
    if r.mode != "frequency"
        || r.ac_sources.len() > r.circuit.elements.len()
        || r.frequencies.len().saturating_mul(r.ac_sources.len()) > 100_000
    {
        return Err(fail(
            "NETWORK_AC_SOURCE",
            "AC spectra require frequency mode and bounded complete axes",
        ));
    }
    let mut ids = BTreeSet::new();
    for source in &r.ac_sources {
        if !ids.insert(&source.element_id)
            || source.values.len() != r.frequencies.len()
            || source
                .values
                .iter()
                .any(|v| !v.real.is_finite() || !v.imaginary.is_finite())
            || !r.circuit.elements.iter().any(|e| {
                e.id() == source.element_id
                    && matches!(
                        e,
                        Element::VoltageSource { .. } | Element::CurrentSource { .. }
                    )
            })
        {
            return Err(fail(
                "NETWORK_AC_SOURCE",
                "Duplicate/unknown source, nonfinite phasor or incomplete spectrum",
            ));
        }
    }
    Ok(())
}

pub fn response(
    n: &mut network::Network,
    r: &Request,
    index: usize,
    stats: &mut Counters,
) -> Result<(
    BTreeMap<String, Value>,
    BTreeMap<String, Value>,
    f64,
    f64,
    f64,
)> {
    let sources: BTreeMap<_, _> = r
        .ac_sources
        .iter()
        .map(|s| (s.element_id.as_str(), &s.values[index]))
        .collect();
    let mut rhs = vec![0.; 2 * n.n];
    for (i, e) in n.circuit.elements.iter().enumerate() {
        let Some(value) = sources.get(e.id()) else {
            continue;
        };
        for (part, value) in [value.real, value.imaginary].into_iter().enumerate() {
            let offset = part * n.n;
            match e {
                Element::VoltageSource { .. } => rhs[offset + n.branch[i].unwrap()] += value,
                Element::CurrentSource { nodes, .. } => {
                    if let Some(a) = n.nodes[&nodes[0]] {
                        rhs[offset + a] -= value;
                    }
                    if let Some(b) = n.nodes[&nodes[1]] {
                        rhs[offset + b] += value;
                    }
                }
                _ => unreachable!(),
            }
        }
    }
    let (x, residual, kcl) = n.solve(
        n.matrix(0., Some(2. * std::f64::consts::PI * r.frequencies[index])),
        rhs,
        stats,
    )?;
    let phasor = |a: f64, b: f64| json!({"real":a,"imaginary":b});
    let nodes = n
        .nodes
        .keys()
        .map(|id| (id.clone(), phasor(n.v(&x, id), n.v(&x[n.n..], id))))
        .collect();
    let mut currents = BTreeMap::new();
    let (mut real_power, mut imaginary_power, mut scale) = (0., 0., 0.);
    for (i, e) in n.circuit.elements.iter().enumerate() {
        let vr = n.v(&x, &e.nodes()[0]) - n.v(&x, &e.nodes()[1]);
        let vi = n.v(&x[n.n..], &e.nodes()[0]) - n.v(&x[n.n..], &e.nodes()[1]);
        let (ir, ii) = if let Some(branch) = n.branch[i] {
            (x[branch], x[n.n + branch])
        } else if let Element::Resistor { r, .. } = e {
            (vr / r, vi / r)
        } else {
            sources
                .get(e.id())
                .map_or((0., 0.), |v| (v.real, v.imaginary))
        };
        currents.insert(e.id().to_owned(), phasor(ir, ii));
        let pr = vr * ir + vi * ii;
        let pi = vi * ir - vr * ii;
        real_power += pr;
        imaginary_power += pi;
        scale += pr.abs() + pi.abs();
    }
    let error = real_power.hypot(imaginary_power);
    if error > 1e-12 + 1e-8 * scale {
        return Err(fail(
            "NETWORK_POWER_BALANCE",
            format!("Explicit AC complex power imbalance {error}"),
        ));
    }
    Ok((nodes, currents, residual, kcl, error / scale.max(1e-12)))
}
