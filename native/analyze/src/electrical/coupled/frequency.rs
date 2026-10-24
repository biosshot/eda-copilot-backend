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
