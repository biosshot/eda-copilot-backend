//! Explicit tabulated multiports, common return, power waves, CPU/F64.
pub(super) mod math;
mod touchstone;
use super::{fail, Budget, Result};
use math::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeSet, time::Instant};
const TEXT_BYTES: usize = 8 * 1024 * 1024;
const TABLE_ELEMENTS: usize = 262144;
const OUTPUT_ELEMENTS: usize = 131072;
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Quality {
    max_residual: f64,
    max_condition: f64,
    solves: usize,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Resource {
    pub id: String,
    pub kind: String,
    pub ports: Vec<String>,
    pub data: Value,
    pub source: Option<String>,
    pub validity: Option<Value>,
    pub analyses: Option<Vec<String>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub model: Resource,
    /// Names in exact resource order, independent of object key enumeration.
    pub ports: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub instances: Vec<Instance>,
    pub connections: Vec<[usize; 2]>,
    pub ports: Vec<OutputPort>,
    pub frequencies: Vec<f64>,
    pub representation: String,
    pub reference_ohm: Option<Vec<f64>>,
    pub budget: Option<Budget>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OutputPort {
    pub name: String,
    pub index: usize,
}
#[derive(Deserialize)]
#[serde(tag = "format", deny_unknown_fields)]
enum Data {
    #[serde(rename = "network-parameters", rename_all = "camelCase")]
    Parameters {
        representation: String,
        frequencies_hz: Vec<f64>,
        parameters: Vec<Matrix>,
        reference_ohm: Option<Value>,
        reference_topology: String,
        nature: String,
        #[serde(default, rename = "temporal")]
        _temporal: Option<Value>,
    },
    #[serde(rename = "touchstone", rename_all = "camelCase")]
    Touchstone {
        content: String,
        reference_topology: String,
        nature: String,
        #[serde(default, rename = "temporal")]
        _temporal: Option<Value>,
    },
}
struct Model {
    band: [f64; 2],
    frequencies: Vec<f64>,
    parameters: Vec<Matrix>,
    refs: Vec<f64>,
    rep: String,
    nature: String,
    metadata: Value,
    conditional: bool,
    delayed: Option<super::delayed::Model>,
}
/// Producer applicability is independent of the mathematical resource class.
pub(super) fn declared_band(resource: &Resource) -> Result<[f64; 2]> {
    let Some(v) = resource
        .validity
        .as_ref()
        .and_then(|v| v["domain"].get("bandHz"))
    else {
        return Ok([0., f64::MAX]);
    };
    let band: Vec<f64> = serde_json::from_value(v.clone()).map_err(|e| fail("NETWORK_BAND", e))?;
    if band.len() != 2 || band.iter().any(|v| !v.is_finite() || *v < 0.) || band[1] < band[0] {
        return Err(fail("NETWORK_BAND", "Invalid producer applicability band"));
    }
    Ok([band[0], band[1]])
}
fn admit_table(n: usize, f: usize) -> Result<()> {
    if n == 0 || n > 16 || f == 0 || f > 100000 || f > TABLE_ELEMENTS / n / n {
        Err(fail(
            "NETWORK_BUDGET",
            "Resource limits: 1..16 ports, 1..100000 frequencies, F*N^2 <=262144",
        ))
    } else {
        Ok(())
    }
}
fn names(v: &[String]) -> bool {
    !v.is_empty()
        && v.iter().all(|s| !s.trim().is_empty())
        && v.iter().collect::<BTreeSet<_>>().len() == v.len()
}
pub(super) fn refs(v: &Value, ports: &[String]) -> Result<Vec<f64>> {
    let out = if let Some(n) = v.as_f64() {
        vec![n; ports.len()]
    } else if let Some(o) = v.as_object() {
        if o.len() != ports.len() || ports.iter().any(|p| !o.contains_key(p)) {
            return Err(fail(
                "NETWORK_INPUT",
                "referenceOhm keys must match ordered ports",
            ));
        }
        ports
            .iter()
            .map(|p| o[p].as_f64().unwrap_or(f64::NAN))
            .collect()
    } else {
        return Err(fail(
            "NETWORK_INPUT",
            "referenceOhm must be real scalar/map",
        ));
    };
    if out.iter().any(|n| !n.is_finite() || *n <= 0.) {
        return Err(fail(
            "NETWORK_INPUT",
            "References must be finite positive real",
        ));
    }
    Ok(out)
}
fn normalize(r: &Resource) -> Result<Model> {
    normalize_for(r, "networkParameters")
}
fn normalize_for(r: &Resource, operation: &str) -> Result<Model> {
    if r.kind != "network" || !names(&r.ports) {
        return Err(fail("NETWORK_INPUT", "Invalid resource kind/ports"));
    }
    admit_table(r.ports.len(), 1)?;
    if r.analyses
        .as_ref()
        .is_some_and(|a| !a.iter().any(|s| s == operation))
    {
        return Err(fail(
            "NETWORK_INPUT",
            format!("Resource does not support {operation}"),
        ));
    }
    let conditional = match r
        .validity
        .as_ref()
        .and_then(|v| v.get("status"))
        .and_then(Value::as_str)
    {
        None | Some("valid") => false,
        Some("conditional") => true,
        _ => {
            return Err(fail(
                "NETWORK_INPUT",
                "Resource validity unavailable/invalid",
            ))
        }
    };
    let band = declared_band(r)?;
    if r.data["format"] == "delayed-scattering" {
        let model = super::delayed::model(r)?;
        return Ok(Model {
            band,
            frequencies: vec![band[0], band[1]],
            parameters: vec![],
            refs: model.refs.clone(),
            rep: "S".into(),
            nature: "passive".into(),
            metadata: json!({"format":"delayed-scattering","delayS":model.delay,"interpolation":"exact analytic exp(-jωτ), no finite-band extrapolation"}),
            conditional,
            delayed: Some(model),
        });
    }
    // Check lengths before deserializing/copying any table or tokenizing text.
    if let Some(t) = r.data.get("content").and_then(Value::as_str) {
        if t.len() > TEXT_BYTES {
            return Err(fail("NETWORK_BUDGET", "Touchstone text exceeds 8 MiB"));
        }
    }
    if let Some(f) = r.data.get("frequenciesHz").and_then(Value::as_array) {
        admit_table(r.ports.len(), f.len())?;
    }
    if let Some(m) = r.data.get("parameters").and_then(Value::as_array) {
        admit_table(r.ports.len(), m.len())?;
        for rows in m {
            let rows = rows
                .as_array()
                .ok_or_else(|| fail("NETWORK_INPUT", "Matrix rows"))?;
            if rows.len() != r.ports.len()
                || rows
                    .iter()
                    .any(|row| row.as_array().is_none_or(|row| row.len() != r.ports.len()))
            {
                return Err(fail(
                    "NETWORK_INPUT",
                    "Matrix dimensions do not match ports",
                ));
            }
        }
    }
    let d: Data = serde_json::from_value(r.data.clone()).map_err(|e| fail("NETWORK_INPUT", e))?;
    let (f, p, refs, rep, nature, topology, metadata) = match d {
        Data::Parameters {
            representation,
            frequencies_hz,
            parameters,
            reference_ohm,
            reference_topology,
            nature,
            _temporal: _,
        } => {
            if !["S", "Z", "Y"].contains(&representation.as_str()) {
                return Err(fail("NETWORK_INPUT", "Invalid representation"));
            }
            let refs = if representation == "S" {
                refs(
                    &reference_ohm
                        .ok_or_else(|| fail("NETWORK_INPUT", "S requires referenceOhm"))?,
                    &r.ports,
                )?
            } else {
                if reference_ohm.is_some() {
                    return Err(fail(
                        "NETWORK_INPUT",
                        "Z/Y referenceOhm is ambiguous and forbidden",
                    ));
                }
                vec![50.; r.ports.len()]
            };
            (
                frequencies_hz,
                parameters,
                refs,
                representation,
                nature,
                reference_topology,
                json!({"format":"network-parameters","physicalUnits":"Z Ohm; Y Siemens; S dimensionless"}),
            )
        }
        Data::Touchstone {
            content,
            reference_topology,
            nature,
            _temporal: _,
        } => {
            let (f, p, refs, metadata) = touchstone::parse(&content, r.ports.len())?;
            (
                f,
                p,
                refs,
                "S".to_string(),
                nature,
                reference_topology,
                metadata,
            )
        }
    };
    admit_table(r.ports.len(), f.len())?;
    if topology != "common"
        || !["passive", "active", "unspecified"].contains(&nature.as_str())
        || p.len() != f.len()
        || f.iter()
            .enumerate()
            .any(|(i, v)| !v.is_finite() || *v < 0. || i > 0 && *v <= f[i - 1])
        || p.iter().any(|m| {
            m.len() != r.ports.len()
                || m.iter().any(|row| {
                    row.len() != r.ports.len()
                        || row
                            .iter()
                            .any(|v| !v.real.is_finite() || !v.imaginary.is_finite())
                })
        })
    {
        return Err(fail("NETWORK_INPUT","Invalid topology/nature/frequency axis/coefficient dimensions or nonfinite coefficient"));
    }
    let band = [band[0].max(f[0]), band[1].min(*f.last().unwrap())];
    if band[0] > band[1] {
        return Err(fail(
            "NETWORK_BAND",
            "Samples and producer applicability do not overlap",
        ));
    }
    Ok(Model {
        band,
        frequencies: f,
        parameters: p,
        refs,
        rep,
        nature,
        metadata,
        conditional,
        delayed: None,
    })
}
fn sampled(m: &Model, f: f64) -> Result<(Matrix, Option<[f64; 2]>)> {
    if f < m.band[0] || f > m.band[1] {
        return Err(fail(
            "NETWORK_BAND",
            "Frequency outside producer applicability",
        ));
    }
    if let Some(model) = &m.delayed {
        return Ok((super::delayed::sample(model, f)?, None));
    }
    if f < m.frequencies[0] || f > *m.frequencies.last().unwrap() {
        return Err(fail(
            "NETWORK_BAND",
            "No extrapolation or implicit DC; requested frequency outside model band",
        ));
    }
    match m
        .frequencies
        .binary_search_by(|v| v.partial_cmp(&f).unwrap())
    {
        Ok(k) => Ok((m.parameters[k].clone(), None)),
        Err(k) => {
            let lo = m.frequencies[k - 1];
            let hi = m.frequencies[k];
            let t = (f - lo) / (hi - lo);
            let mut out = m.parameters[k - 1].clone();
            for (i, row) in out.iter_mut().enumerate() {
                for (j, v) in row.iter_mut().enumerate() {
                    *v = (*v).scale(1. - t) + m.parameters[k][i][j].scale(t);
                }
            }
            Ok((out, Some([lo, hi])))
        }
    }
}
/// Same source parser/conversion/passivity guards for the explicitly requested
/// conditional temporal fit. Ordinary frequency calculation is unchanged.
pub(super) fn admittance_samples(
    r: &Resource,
) -> Result<(Vec<f64>, Vec<Matrix>, Vec<f64>, String)> {
    let m = normalize_for(r, "si")?;
    if m.nature != "passive" {
        return Err(fail(
            "TEMPORAL_PASSIVITY",
            "Sampled relaxation requires an authored passive resource",
        ));
    }
    let mut quality = Quality::default();
    let mut values = vec![];
    for matrix in &m.parameters {
        let s = to_s(matrix, &m.rep, &m.refs, &mut quality)?;
        if !passive(&s) {
            return Err(fail(
                "TEMPORAL_PASSIVITY",
                "Source samples violate passivity",
            ));
        }
        values.push(if m.rep == "Y" {
            matrix.clone()
        } else {
            from_s(&s, "Y", &m.refs, &mut quality)?
        });
    }
    Ok((m.frequencies, values, m.refs, m.rep))
}
pub fn run(r: Request) -> Value {
    let start = Instant::now();
    match calculate(&r, start) {
        Ok(mut v) => {
            v["elapsedMs"] = json!(start.elapsed().as_secs_f64() * 1000.);
            v
        }
        Err(e) => {
            json!({"status":"rejected","diagnostics":[{"code":e.split(':').next().unwrap_or("NETWORK_ERROR"),"severity":"error","message":e}],"elapsedMs":start.elapsed().as_secs_f64()*1000.})
        }
    }
}
fn calculate(r: &Request, start: Instant) -> Result<Value> {
    if r.instances
        .iter()
        .any(|i| i.model.data["format"] == "coupled-rlc")
    {
        return super::coupled::frequency_response::calculate(r);
    }
    let p = r
        .instances
        .iter()
        .map(|i| i.model.ports.len())
        .sum::<usize>();
    let n = r.ports.len();
    let f = r.frequencies.len();
    if r.instances.is_empty()
        || r.instances.len() > 32
        || p == 0
        || p > 128
        || n == 0
        || n > 16
        || f == 0
        || f > 100000
        || f > OUTPUT_ELEMENTS / n / n
        || f as f64 * (p as f64).powi(3) > 1e8
    {
        return Err(fail("NETWORK_BUDGET","Composition limits: 32 blocks,128 total ports,16 external ports,131072 output elements, F*P^3 <=1e8"));
    }
    let memory = f * r.instances.len() * 1024
        + p * p * 256
        + OUTPUT_ELEMENTS.min(f * n * n) * 256
        + r.instances
            .iter()
            .map(|i| {
                i.model
                    .data
                    .get("content")
                    .and_then(Value::as_str)
                    .map_or(0, |s| s.len() * 4)
                    + i.model
                        .data
                        .get("parameters")
                        .and_then(Value::as_array)
                        .map_or(0, |a| {
                            a.len()
                                .saturating_mul(i.ports.len())
                                .saturating_mul(i.ports.len())
                                .saturating_mul(192)
                        })
            })
            .sum::<usize>();
    let seconds = r.budget.as_ref().and_then(|b| b.max_seconds).unwrap_or(60.);
    let memory_limit = r
        .budget
        .as_ref()
        .and_then(|b| b.max_memory_mi_b)
        .unwrap_or(256.);
    if !seconds.is_finite()
        || seconds <= 0.
        || !memory_limit.is_finite()
        || memory_limit <= 0.
        || memory as f64 > memory_limit * 1048576.
    {
        return Err(fail(
            "NETWORK_BUDGET",
            "Invalid time/memory budget or planned model/output/workspace exceeds memory limit",
        ));
    }
    if !["S", "Z", "Y"].contains(&r.representation.as_str())
        || r.frequencies
            .iter()
            .enumerate()
            .any(|(i, x)| !x.is_finite() || *x < 0. || i > 0 && *x <= r.frequencies[i - 1])
    {
        return Err(fail(
            "NETWORK_INPUT",
            "Invalid output representation/frequencies",
        ));
    }
    let output_refs = if r.representation == "S" {
        let refs = r
            .reference_ohm
            .clone()
            .ok_or_else(|| fail("NETWORK_INPUT", "S output requires referenceOhm"))?;
        if refs.len() != n || refs.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return Err(fail("NETWORK_INPUT", "Invalid output references"));
        }
        refs
    } else {
        if r.reference_ohm.is_some() {
            return Err(fail(
                "NETWORK_INPUT",
                "Output Z/Y cannot specify referenceOhm",
            ));
        }
        vec![50.; n]
    };
    if r.instances
        .iter()
        .map(|i| &i.id)
        .collect::<BTreeSet<_>>()
        .len()
        != r.instances.len()
        || r.instances.iter().any(|i| {
            i.id.trim().is_empty() || i.ports.len() != i.model.ports.len() || !names(&i.ports)
        })
        || !names(&r.ports.iter().map(|p| p.name.clone()).collect::<Vec<_>>())
    {
        return Err(fail(
            "NETWORK_TOPOLOGY",
            "Duplicate/invalid instance or port mapping",
        ));
    }
    let mut assigned = vec![false; p];
    let mut partner = vec![None; p];
    for pair in &r.connections {
        let [a, b] = *pair;
        if a >= p || b >= p || a == b || assigned[a] || assigned[b] {
            return Err(fail(
                "NETWORK_TOPOLOGY",
                "Duplicate/invalid pair; no junction or self connection",
            ));
        }
        assigned[a] = true;
        assigned[b] = true;
        partner[a] = Some(b);
        partner[b] = Some(a);
    }
    for port in &r.ports {
        if port.index >= p || assigned[port.index] {
            return Err(fail(
                "NETWORK_TOPOLOGY",
                "Duplicate external alias or externally connected port",
            ));
        }
        assigned[port.index] = true;
    }
    if assigned.iter().any(|x| !*x) {
        return Err(fail(
            "NETWORK_TOPOLOGY",
            "Every port must be external or paired once; no implicit termination",
        ));
    }
    let mut offsets = Vec::new();
    let mut offset = 0;
    let mut owner = Vec::new();
    for (k, i) in r.instances.iter().enumerate() {
        offsets.push(offset);
        offset += i.ports.len();
        owner.extend(std::iter::repeat_n(k, i.ports.len()));
    }
    // Every output is advertised as common-return. Pairing joins returns;
    // unrelated external ports and equal wave references do not join them.
    let mut reachable = BTreeSet::from([owner[r.ports[0].index]]);
    loop {
        let old = reachable.len();
        for [a, b] in &r.connections {
            if reachable.contains(&owner[*a]) || reachable.contains(&owner[*b]) {
                reachable.insert(owner[*a]);
                reachable.insert(owner[*b]);
            }
        }
        if old == reachable.len() {
            break;
        }
    }
    if reachable.len() != r.instances.len() {
        return Err(fail(
            "NETWORK_TOPOLOGY",
            "Disconnected return groups/hidden blocks: one connected common-return network is required",
        ));
    }
    let models = r
        .instances
        .iter()
        .map(|i| {
            normalize(&i.model).map_err(|e| {
                format!(
                    "{}: resource {} (instance {}): {}",
                    e.split(':').next().unwrap(),
                    i.model.id,
                    i.id,
                    e
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let elements = models
        .iter()
        .map(|m| m.parameters.len() * m.refs.len() * m.refs.len())
        .sum::<usize>();
    if elements > 524288 {
        return Err(fail(
            "NETWORK_BUDGET",
            "Aggregate source coefficient limit 524288",
        ));
    }
    let band = [
        models.iter().map(|m| m.band[0]).fold(0., f64::max),
        models
            .iter()
            .map(|m| m.band[1])
            .fold(f64::INFINITY, f64::min),
    ];
    let mut quality = Quality::default();
    let mut parameters = Vec::new();
    let mut interpolation = Vec::new();
    let mut conditional = models.iter().any(|m| m.conditional);
    let mut provenance = Vec::new();
    for (i, m) in models.iter().enumerate() {
        provenance.push(json!({"instanceId":r.instances[i].id,"resource":r.instances[i].model,"mapping":r.instances[i].ports,"sourcePorts":r.instances[i].model.ports,"sourceRepresentation":m.rep,"sourceReferenceOhm":if m.rep=="S"{Some(&m.refs)}else{None},"bandHz":m.band,"nature":m.nature,"normalization":m.metadata,"normalized":{"frequenciesHz":m.frequencies,"parameters":m.parameters,"representation":m.rep,"referenceOhm":if m.rep=="S"{Some(&m.refs)}else{None}},"interpolation":"linear real/imag in source representation and source references, BEFORE conversion/composition"}));
    }
    for &frequency in &r.frequencies {
        let step=(||->Result<Matrix>{
   if start.elapsed().as_secs_f64()>seconds{return Err(fail("NETWORK_BUDGET","Native frequency deadline exceeded"));}
   let mut s=zero(p,p);
   for (k,m) in models.iter().enumerate(){let (matrix,interval)=sampled(m,frequency).map_err(|e|format!("{}: resource {}: {}",e.split(':').next().unwrap(),r.instances[k].model.id,e))?;
    if let Some(interval)=interval{conditional=true;interpolation.push(json!({"instanceId":r.instances[k].id,"resourceId":r.instances[k].model.id,"frequencyHz":frequency,"intervalHz":interval,"representation":m.rep,"referenceOhm":if m.rep=="S"{Some(&m.refs)}else{None}}));}
    let block=to_s(&matrix,&m.rep,&m.refs,&mut quality)?;if m.nature=="passive"&&!passive(&block){return Err(fail("NETWORK_PASSIVITY",format!("resource {}: max singular value exceeds 1+1e-9; coefficients retained, no repair",r.instances[k].model.id)));}
    for (i,row)in block.iter().enumerate(){for(j,z)in row.iter().enumerate(){s[offsets[k]+i][offsets[k]+j]=*z;}}
   }
   let mut lhs=eye(p);let mut rhs=zero(p,n);for i in 0..p{if let Some(j)=partner[i]{for k in 0..p{lhs[i][k]=lhs[i][k]-s[j][k];}}}
   for(j,port)in r.ports.iter().enumerate(){rhs[port.index][j].real=1.;}
   let incident=solve(&lhs,&rhs,&mut quality)?;let mut external=zero(n,n);for(i,port)in r.ports.iter().enumerate(){for j in 0..n{for k in 0..p{external[i][j]=external[i][j]+s[port.index][k]*incident[k][j];}}}
   from_s(&external,&r.representation,&output_refs,&mut quality)
  })().map_err(|e|format!("{}: frequency {frequency} Hz: {e}",e.split(':').next().unwrap()))?;
        if step
            .iter()
            .flatten()
            .any(|v| !v.real.is_finite() || !v.imaginary.is_finite())
        {
            return Err(fail(
                "NETWORK_NONFINITE",
                format!("frequency {frequency} Hz: recovered coefficient overflow"),
            ));
        }
        if start.elapsed().as_secs_f64() > seconds {
            return Err(fail("NETWORK_BUDGET", "Native frequency deadline exceeded"));
        }
        parameters.push(step);
    }
    let nature = if models.iter().any(|m| m.nature == "active") {
        "active"
    } else if models.iter().all(|m| m.nature == "passive") {
        "passive"
    } else {
        "unspecified"
    };
    Ok(
        json!({"status":"complete","diagnostics":[],"frequenciesHz":r.frequencies,"ports":r.ports,"representation":r.representation,"referenceOhm":if r.representation=="S"{Some(output_refs)}else{None},"parameters":parameters,"nature":nature,"conditional":conditional,"bandHz":band,"interpolation":interpolation,"provenance":provenance,"quality":quality,"topology":{"connections":r.connections,"totalPorts":p,"returnConvention":"Common within each resource; pair connections join positive and return terminals. Reference impedance creates no physical load."},"memoryPlanBytes":memory,"limits":{"resourcePorts":16,"resourceFrequencies":100000,"sourceElementsPerResource":TABLE_ELEMENTS,"aggregateSourceElements":524288,"totalPorts":128,"outputElements":OUTPUT_ELEMENTS,"passivityTolerance":1e-9,"residualTolerance":1e-8,"maxCondition":1e12},"validityLimitations":if models.iter().all(|m|m.delayed.is_some()) { "Exact passive causal real-K delay resources and their frequency composition; producer bands retained. These output samples alone are not a temporal realization; no PCB extraction." } else { "Passivity only at evaluated frequencies; computed samples do not provide causality/stability/temporal model or PCB extraction." }}),
    )
}
