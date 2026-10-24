//! Direct descriptor evaluation keeps differential terminal semantics. A generic
//! infinity block is not silently advertised as a common-return tabulated block.
use super::*;
use crate::electrical::frequency::{math, Request};
pub fn calculate(request: &Request) -> Result<Value> {
    if request.instances.len() != 1 || !request.connections.is_empty() {
        return Err(fail("COUPLED_TOPOLOGY","Descriptor frequency composition requires explicit physical terminal mapping; use one complete block"));
    }
    let instance = &request.instances[0];
    let resource = &instance.model;
    let ports = resource.ports.len();
    if resource.kind != "network"
        || resource.id.trim().is_empty()
        || ports == 0
        || ports > 16
        || resource.ports.iter().any(|v| v.trim().is_empty())
        || resource.ports.iter().collect::<BTreeSet<_>>().len() != ports
        || instance.ports.len() != ports
        || request.ports.len() != ports
        || request
            .ports
            .iter()
            .enumerate()
            .any(|(i, p)| p.index != i || p.name.is_empty())
        || request
            .ports
            .iter()
            .map(|p| &p.name)
            .collect::<BTreeSet<_>>()
            .len()
            != ports
        || resource
            .analyses
            .as_ref()
            .is_some_and(|a| !a.iter().any(|v| v == "networkParameters"))
    {
        return Err(fail(
            "COUPLED_PORT",
            "Direct descriptor requires a complete ordered port mapping and authorized Resource",
        ));
    }
    let conditional = match resource
        .validity
        .as_ref()
        .and_then(|v| v["status"].as_str())
    {
        None | Some("valid") => false,
        Some("conditional") => true,
        _ => return Err(fail("COUPLED_VALIDITY", "Unavailable or invalid Resource")),
    };
    let band = if let Some(domain) = resource
        .validity
        .as_ref()
        .and_then(|v| v["domain"].get("bandHz"))
    {
        let band: Vec<f64> =
            serde_json::from_value(domain.clone()).map_err(|e| fail("COUPLED_BAND", e))?;
        if band.len() != 2
            || !band[0].is_finite()
            || !band[1].is_finite()
            || band[0] < 0.
            || band[1] < band[0]
            || request
                .frequencies
                .iter()
                .any(|f| *f < band[0] || *f > band[1])
        {
            return Err(fail(
                "COUPLED_BAND",
                "Requested frequency outside declared model validity band",
            ));
        }
        band
    } else {
        vec![0., f64::MAX]
    };
    let model: Model =
        serde_json::from_value(resource.data.clone()).map_err(|e| fail("COUPLED_INPUT", e))?;
    let refs = if request.representation == "S" {
        let refs = request.reference_ohm.as_ref().ok_or_else(|| {
            fail(
                "COUPLED_REFERENCE",
                "S output requires explicit positive real references",
            )
        })?;
        if refs.len() != ports || refs.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err(fail("COUPLED_REFERENCE", "Invalid power-wave references"));
        }
        refs.clone()
    } else {
        if request.reference_ohm.is_some() || !["Z", "Y"].contains(&request.representation.as_str())
        {
            return Err(fail(
                "COUPLED_REFERENCE",
                "Z/Y forbid wave references; unsupported representation",
            ));
        }
        vec![]
    };
    let mut result = impedance(&model, ports, &request.frequencies, request.budget.as_ref())?;
    let mut quality = crate::electrical::frequency::Quality::default();
    if request.representation != "Z" {
        let z: Vec<math::Matrix> = serde_json::from_value(result["parameters"].clone())
            .map_err(|e| fail("COUPLED_NONFINITE", e))?;
        let converted = z
            .iter()
            .map(|z| {
                if request.representation == "Y" {
                    math::solve(z, &math::eye(ports), &mut quality)
                } else {
                    math::to_s(z, "Z", &refs, &mut quality)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        result["parameters"] = json!(converted);
    }
    result["representation"] = json!(request.representation);
    result["referenceOhm"] = if refs.is_empty() {
        Value::Null
    } else {
        json!(refs)
    };
    result["diagnostics"] = json!([]);
    result["ports"] = json!(request.ports);
    result["nature"] = json!("passive");
    result["conditional"] = json!(conditional);
    result["bandHz"] = json!(band);
    result["interpolation"] = json!([]);
    result["provenance"] = json!([{"instanceId":instance.id,"resource":resource,"mapping":instance.ports,"sourcePorts":resource.ports,"sourceRepresentation":"coupled-rlc","sourceReferenceOhm":null,"bandHz":band,"nature":"passive","normalization":{"physicalDescriptor":true,"referenceTopology":model.reference_topology}}]);
    result["conversionQuality"] = json!(quality);
    result["validityLimitations"]=json!("Exact evaluation of supplied full RLC model inside declared validity band; no PCB extraction accuracy inferred. Physical terminal mapping retained, no implicit common return.");
    Ok(result)
}
/// Convert only explicit grounded canonical descriptors at the requested points.
/// Their reference loads define S, not additional physical loads in composition.
/// All subsequent pair joins use the existing passive wave composition backend.
pub(crate) fn common_tables(request: &Request, start: Instant) -> Result<(Request, usize, f64)> {
    let p = request
        .instances
        .iter()
        .map(|i| i.ports.len())
        .sum::<usize>();
    let f = request.frequencies.len();
    let n = request.ports.len();
    if request.instances.is_empty()
        || request.instances.len() > 32
        || p > 128
        || n == 0
        || n > 16
        || f == 0
        || f > 100000
        || f.saturating_mul(n * n) > 131072
        || f.saturating_mul(p * p * p) > 100000000
        || request.instances.iter().any(|i| {
            i.model.ports.is_empty()
                || i.model.ports.len() > 16
                || f.saturating_mul(i.model.ports.len().pow(2)) > 262144
        })
        || request
            .frequencies
            .iter()
            .enumerate()
            .any(|(i, x)| !x.is_finite() || *x < 0. || i > 0 && *x <= request.frequencies[i - 1])
    {
        return Err(fail(
            "NETWORK_BUDGET",
            "Mixed analytic frequency output/work limits",
        ));
    }
    let elements = request
        .instances
        .iter()
        .map(|i| f * i.model.ports.len().pow(2))
        .sum::<usize>();
    if elements > 524288 {
        return Err(fail(
            "NETWORK_BUDGET",
            "Mixed frequency aggregate source elements",
        ));
    }
    let reserved = request
        .instances
        .iter()
        .map(|i| composition::source_plan(&i.model))
        .sum::<usize>()
        .saturating_add(elements.saturating_mul(576));
    let mut budget = composition::remaining_budget(request.budget.as_ref(), reserved)?;
    budget.max_seconds = Some(
        request
            .budget
            .as_ref()
            .and_then(|b| b.max_seconds)
            .unwrap_or(60.),
    );
    let mut normalized = request.clone();
    normalized.budget = Some(budget.clone());
    let mut residual = 0_f64;
    for instance in &mut normalized.instances {
        super::super::deadline(start, Some(&budget), "Mixed descriptor frequency admission")?;
        let resource = &mut instance.model;
        if resource.data["format"] != "coupled-rlc" {
            continue;
        }
        if resource.kind != "network"
            || resource
                .analyses
                .as_ref()
                .is_some_and(|a| !a.iter().any(|s| s == "networkParameters"))
        {
            return Err(fail(
                "COUPLED_RESOURCE",
                "Descriptor does not authorize frequency calculation",
            ));
        }
        match resource
            .validity
            .as_ref()
            .and_then(|v| v["status"].as_str())
        {
            None | Some("valid") | Some("conditional") => {}
            _ => return Err(fail("COUPLED_VALIDITY", "Unavailable descriptor")),
        }
        let band = crate::electrical::frequency::declared_band(resource)?;
        if request
            .frequencies
            .iter()
            .any(|f| *f < band[0] || *f > band[1])
        {
            return Err(fail(
                "COUPLED_BAND",
                "Frequency outside producer applicability",
            ));
        }
        let model: Model =
            serde_json::from_value(resource.data.clone()).map_err(|e| fail("COUPLED_INPUT", e))?;
        if model.reference_topology != "grounded" {
            return Err(fail(
                "COUPLED_TERMINAL_MAP",
                "Mixed infinity descriptor joins require explicit physical terminal mapping",
            ));
        }
        let ports = resource.ports.len();
        let mut system = System::compile(&model, ports, Some(&budget))?;
        for port in 0..ports {
            let entries: Vec<_> = model
                .port_incidence
                .iter()
                .filter(|(_, j, v)| *j == port && *v != 0.)
                .collect();
            if entries.len() != 1 || entries[0].2 != 1. {
                return Err(fail(
                    "COUPLED_TERMINAL_MAP",
                    "Mixed frequency requires canonical positive-potential ports",
                ));
            }
            system.add_port_admittance(port, 1. / 50., 0.)?;
        }
        let states = system.linear.n;
        let mut parameters = Vec::with_capacity(f);
        let mut counters = Counters::default();
        for &frequency in &request.frequencies {
            super::super::deadline(
                start,
                Some(&budget),
                "Mixed descriptor frequency evaluation",
            )?;
            let complex = frequency > 0.;
            let mut matrix = vec![vec![math::C::default(); ports]; ports];
            for input in 0..ports {
                let mut incident = vec![0.; ports];
                incident[input] = 2. / 50_f64.sqrt();
                let mut rhs = system.port_rhs(&incident);
                if complex {
                    rhs.resize(states * 2, 0.);
                }
                let x = system.solve(
                    2. * std::f64::consts::PI * frequency,
                    rhs,
                    &vec![0.; states],
                    &mut counters,
                )?;
                let re = system.port_voltages(&x[..states], ports);
                let im = if complex {
                    system.port_voltages(&x[states..], ports)
                } else {
                    vec![0.; ports]
                };
                for output in 0..ports {
                    matrix[output][input] = math::C::new(
                        re[output] / 50_f64.sqrt() - if output == input { 1. } else { 0. },
                        im[output] / 50_f64.sqrt(),
                    );
                }
            }
            parameters.push(matrix);
        }
        residual = residual.max(system.residual);
        resource.data = json!({"format":"network-parameters","representation":"S","referenceTopology":"common","nature":"passive","referenceOhm":50.,"frequenciesHz":request.frequencies,"parameters":parameters});
    }
    super::super::deadline(
        start,
        Some(&budget),
        "Mixed descriptor frequency normalization",
    )?;
    Ok((normalized, reserved, residual))
}
