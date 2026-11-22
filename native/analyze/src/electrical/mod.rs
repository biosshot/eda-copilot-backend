//! Explicit linear networks, CPU/F64. No PCB extraction or implicit parasitics.
mod ac;
pub mod coupled;
mod delayed;
pub mod frequency;
mod network;
pub mod relaxation;
pub mod resonant;
pub mod signal;
mod solve;
pub mod vector_fit;
mod waveform;
pub use network::{Circuit, Element, Port};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;
pub use waveform::Waveform;
type Result<T> = std::result::Result<T, String>;
fn fail(code: &str, message: impl std::fmt::Display) -> String {
    format!("{code}: {message}")
}
/// Only trusted Rust callers can construct this budget (JSON has no infinities).
impl Budget {
    pub(crate) fn externally_supervised() -> Self {
        Self { max_seconds: Some(f64::INFINITY), max_memory_mi_b: Some(f64::INFINITY) }
    }

    pub(crate) fn uses_external_limits(&self) -> bool {
        self.max_seconds == Some(f64::INFINITY) && self.max_memory_mi_b == Some(f64::INFINITY)
    }
}

/// A single monotonic deadline is passed through fitting, assembly and solving.
pub(super) fn deadline(start: Instant, budget: Option<&Budget>, stage: &str) -> Result<()> {
    if budget.is_some_and(Budget::uses_external_limits) { return Ok(()); }
    let seconds = budget.and_then(|b| b.max_seconds).unwrap_or(120.);
    if !seconds.is_finite() || seconds <= 0. {
        return Err(fail("NETWORK_BUDGET", "Invalid request deadline"));
    }
    if start.elapsed().as_secs_f64() > seconds {
        return Err(fail(
            "NETWORK_DEADLINE",
            format!("{stage}: shared request deadline exceeded"),
        ));
    }
    Ok(())
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Numerics {
    pub relative_tolerance: Option<f64>,
    pub absolute_tolerance: Option<f64>,
    pub max_iterations: Option<usize>,
    pub max_step_s: Option<f64>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Time {
    pub stop_s: f64,
    pub sample_step_s: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Budget {
    pub max_seconds: Option<f64>,
    pub max_memory_mi_b: Option<f64>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Phasor {
    pub real: f64,
    pub imaginary: f64,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct AcSource {
    pub element_id: String,
    pub values: Vec<Phasor>,
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub context_id: String,
    pub circuit: Circuit,
    pub mode: String,
    #[serde(default)]
    pub frequencies: Vec<f64>,
    #[serde(default)]
    pub ac_sources: Vec<AcSource>,
    pub time: Option<Time>,
    pub initial: Option<Value>,
    pub numerics: Option<Numerics>,
    pub budget: Option<Budget>,
}
#[derive(Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Counters {
    pub compilations: usize,
    pub symbolic_factorizations: usize,
    pub numerical_factorizations: usize,
    pub solves: usize,
    pub reused_factors: usize,
    pub accepted_steps: usize,
    pub rejected_steps: usize,
}
/// One bounded session in the existing disposable scenario process.
#[derive(Default)]
pub struct Session {
    compiled: Option<network::Network>,
}
impl Session {
    pub fn solve(&mut self, request: Request) -> Value {
        let start = Instant::now();
        let result = (|| -> Result<Value> {
            ac::validate(&request)?;
            let mut stats = Counters::default();
            let mut identity = request.circuit.clone();
            identity.ports.clear();
            let key = serde_json::to_string(&identity).map_err(|e| fail("NETWORK_INPUT", e))?;
            for e in &mut identity.elements {
                match e {
                    Element::VoltageSource {
                        value, waveform, ..
                    }
                    | Element::CurrentSource {
                        value, waveform, ..
                    } => {
                        *value = 0.;
                        *waveform = None;
                    }
                    _ => {}
                }
            }
            let matrix_key = format!(
                "{}:{}",
                request.context_id,
                serde_json::to_string(&identity).map_err(|e| fail("NETWORK_INPUT", e))?
            );
            if self
                .compiled
                .as_ref()
                .is_none_or(|n| n.matrix_identity != matrix_key)
            {
                self.compiled = None;
                self.compiled = Some(network::Network::compile(
                    &request.circuit,
                    key.clone(),
                    &request,
                )?);
                stats.compilations = 1;
            }
            let n = self.compiled.as_mut().unwrap();
            if request.circuit.ports.len() > 128
                || request.circuit.ports.iter().any(|p| {
                    p.id.is_empty()
                        || p.positive == p.negative
                        || !n.nodes.contains_key(&p.positive)
                        || !n.nodes.contains_key(&p.negative)
                })
                || request
                    .circuit
                    .ports
                    .iter()
                    .map(|p| &p.id)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != request.circuit.ports.len()
            {
                return Err(fail("NETWORK_INPUT", "Invalid/duplicate port"));
            }
            for e in &request.circuit.elements {
                if !e.source(None).is_finite() {
                    return Err(fail("NETWORK_INPUT", "Nonfinite source"));
                }
                if let Some(w) = e.waveform() {
                    w.validate()?;
                }
            }
            n.matrix_identity = matrix_key;
            n.identity = key;
            n.circuit = request.circuit.clone();
            n.admit(&request)?;
            let mut report = solve::run(n, &request, &mut stats, start)?;
            fn finite(v: &Value) -> bool {
                match v {
                    Value::Null => false,
                    Value::Number(n) => n.as_f64().is_some_and(f64::is_finite),
                    Value::Array(a) => a.iter().all(finite),
                    Value::Object(o) => o.values().all(finite),
                    _ => true,
                }
            }
            if !finite(&report) {
                return Err(fail(
                    "NETWORK_NONFINITE",
                    "Nonfinite recovered quantity/quality/state",
                ));
            }
            report["counters"] = json!(stats);
            report["system"] = json!({"unknowns":n.n,"nodalUnknowns":n.nv,"gNonzeros":n.g.len(),"eNonzeros":n.e.len(),"solver":"faer sparse LU; real-block complex AC; no dense solve"});
            report["elapsedMs"] = json!(start.elapsed().as_secs_f64() * 1000.);
            report["request"] = json!(request);
            if let Some(state) = report.get("finalState") {
                n.final_state = Some(state.clone());
            }
            Ok(report)
        })();
        match result {
            Ok(v) => v,
            Err(e) => {
                self.compiled = None;
                json!({"status":"rejected","diagnostics":[{"code":e.split(':').next().unwrap_or("NETWORK_ERROR"),"message":e,"severity":"error"}],"elapsedMs":start.elapsed().as_secs_f64()*1000.})
            }
        }
    }
}

#[cfg(test)]
mod deadline_tests {
    use super::*;
    #[test]
    fn retained_sources_share_one_memory_allowance_before_fitting() {
        let resource: frequency::Resource = serde_json::from_value(json!({
            "id":"retained","kind":"network","ports":["p"],
            "data":{"unvalidatedSource":vec![1.;1024]}
        }))
        .unwrap();
        let bytes = coupled::composition::source_plan(&resource);
        let budget = Budget {
            max_seconds: Some(120.),
            max_memory_mi_b: Some(bytes as f64 * 1.5 / 1048576.),
        };
        assert!(coupled::composition::remaining_budget(Some(&budget), bytes).is_ok());
        let topology = coupled::composition::Topology {
            instances: (0..2)
                .map(|i| frequency::Instance {
                    id: format!("b{i}"),
                    model: resource.clone(),
                    ports: vec!["p".into()],
                })
                .collect(),
            connection_contacts: None,
            connections: vec![],
            instance_inductance_h: vec![],
            ports: vec![frequency::OutputPort {
                name: "p".into(),
                index: 0,
            }],
        };
        assert!(
            coupled::composition::build(&topology, Some(&budget), Instant::now())
                .err()
                .unwrap()
                .starts_with("COUPLED_BUDGET")
        );
        let tiny = Budget {
            max_seconds: Some(120.),
            max_memory_mi_b: Some(bytes as f64 / 2. / 1048576.),
        };
        assert!(relaxation::fit(&resource, Some(&tiny))
            .err()
            .unwrap()
            .starts_with("COUPLED_BUDGET"));
    }
    #[test]
    fn composed_fits_and_assembly_share_the_original_expired_start() {
        let start = Instant::now() - std::time::Duration::from_secs(1);
        let budget = Budget {
            max_seconds: Some(0.1),
            max_memory_mi_b: None,
        };
        let resource: frequency::Resource = serde_json::from_value(
            json!({"id":"expired","kind":"network","ports":["p"],"data":{}}),
        )
        .unwrap();
        let topology = coupled::composition::Topology {
            instances: vec![],
            connection_contacts: None,
            connections: vec![],
            instance_inductance_h: vec![],
            ports: vec![],
        };
        for error in [
            relaxation::fit_started(&resource, Some(&budget), start)
                .err()
                .unwrap(),
            coupled::composition::resource(&resource, Some(&budget), start)
                .err()
                .unwrap(),
            coupled::composition::build(&topology, Some(&budget), start)
                .err()
                .unwrap(),
        ] {
            assert!(
                error.starts_with("NETWORK_DEADLINE"),
                "A stage must not start a fresh budget: {error}"
            );
        }
    }
}
