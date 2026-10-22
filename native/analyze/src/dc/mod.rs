//! CPU/F64 steady DC. Existing conduction mesh; finite sheet/barrel conductance.
//! No net-name merging, gmin, material defaults, refill or GPU.
mod system;
use crate::conduction::{self, ConductionOptions, ConductionReport};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Conductor {
    pub length_mm: f64,
    pub width_mm: f64,
    pub thickness_mm: f64,
    pub conductivity_s_m: f64,
    pub state: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Terminal {
    pub id: String,
    pub pad_id: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementKind {
    Voltage,
    Current,
    Resistance,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Element {
    pub id: String,
    pub kind: ElementKind,
    pub positive: String,
    pub negative: String,
    /// Voltage V / current A (injected into positive) / resistance Ohm.
    pub value: f64,
    pub role: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub max_memory_mib: f64,
    pub max_unknowns: usize,
    pub max_seconds: f64,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            max_memory_mib: 1024.,
            max_unknowns: 20_000,
            max_seconds: 120.,
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub context_id: String,
    pub board_json: Option<String>,
    pub conductor: Option<Conductor>,
    #[serde(default)]
    pub conduction: ConductionOptions,
    #[serde(default)]
    pub snapshot: bool,
    #[serde(default)]
    pub terminals: Vec<Terminal>,
    pub elements: Vec<Element>,
    #[serde(default)]
    pub limits: Limits,
}
#[derive(Debug, Clone, Serialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}
pub(super) fn error(code: &str, message: impl Into<String>) -> Error {
    Error {
        code: code.into(),
        message: message.into(),
    }
}
type Result<T> = std::result::Result<T, Error>;
fn positive(x: f64) -> bool {
    x.is_finite() && x > 0.
}

/// Owned by one scenario/worker. At most one prepared board and one matrix.
#[derive(Default)]
pub struct Session {
    preparation_key: String,
    mesh: Option<ConductionReport>,
    geometry: Option<Value>,
    matrix: Option<system::System>,
}
impl Session {
    pub fn solve(&mut self, r: Request) -> Value {
        let start = Instant::now();
        let reused = self.mesh.is_some()
            && self.preparation_key
                == serde_json::to_string(&(&r.context_id, &r.board_json, &r.conduction)).unwrap();
        let result = self.run(&r, start);
        let mut out = match result {
            Ok(v) => v,
            Err(e) => json!({"status":"rejected","diagnostics":[e],"conditional":false}),
        };
        out["operation"] = json!("dc");
        out["method"] = json!(if r.conductor.is_some() {
            "CPU/F64 analytic uniform rectangular conductor"
        } else {
            "CPU/F64 linear triangle FEM; faer sparse QR"
        });
        out["elapsed_ms"] = json!(start.elapsed().as_secs_f64() * 1000.);
        out["reused_preparation"] = json!(reused);
        // Retain original readiness; snapshot acceptance is a separate decision.
        if r.board_json.is_some()
            && self.preparation_key
                == serde_json::to_string(&(&r.context_id, &r.board_json, &r.conduction)).unwrap()
        {
            out["mesh"] = json!(self.mesh);
            out["geometry"] = json!(self.geometry);
        }
        out
    }
    fn run(&mut self, r: &Request, start: Instant) -> Result<Value> {
        if r.context_id.is_empty()
            || !positive(r.limits.max_memory_mib)
            || !(1. ..=4096.).contains(&r.limits.max_memory_mib)
            || !(2..=100_000).contains(&r.limits.max_unknowns)
            || !positive(r.limits.max_seconds)
        {
            return Err(error(
                "INVALID_DC_REQUEST",
                "Invalid context or resource limits",
            ));
        }
        if r.board_json.is_some() == r.conductor.is_some() {
            return Err(error("INVALID_DC_MODE", "Choose board or conductor"));
        }
        if r.elements.is_empty()
            || r.elements.iter().any(|e| {
                !e.value.is_finite()
                    || e.id.is_empty()
                    || !["source", "load"].contains(&e.role.as_str())
                    || matches!(e.kind, ElementKind::Resistance) && e.value <= 0.
            })
        {
            return Err(error(
                "INVALID_DC_ELEMENT",
                "Finite sources/loads with positive resistance and stable IDs required",
            ));
        }
        if let Some(c) = &r.conductor {
            return standalone(c, r);
        }
        let key = serde_json::to_string(&(&r.context_id, &r.board_json, &r.conduction)).unwrap();
        let reused_preparation = key == self.preparation_key && self.mesh.is_some();
        if !reused_preparation {
            self.matrix = None;
            self.mesh = None;
            self.geometry = None;
            self.preparation_key.clear();
            let input = r.board_json.as_ref().unwrap();
            // Conservative working-set planning, not a claim of exact allocator RSS.
            let planned = input.len() as f64 * 16.
                + r.conduction.geometry.max_generated_vertices as f64 * 192.
                + r.conduction.mesh.max_nodes as f64 * 320.
                + r.conduction.mesh.max_triangles as f64 * 192.;
            if planned > r.limits.max_memory_mib * 1048576. {
                return Err(error(
                    "DC_MEMORY_LIMIT",
                    format!("Preparation working-set plan {planned} bytes exceeds budget"),
                ));
            }
            let mut imported = crate::import_json(input);
            let board = imported.board.take().ok_or_else(|| {
                error(
                    "DC_INVALID_BOARD",
                    format!("Import rejected: {:?}", imported.diagnostics),
                )
            })?;
            let g = crate::geometry::prepare(&board, r.conduction.geometry);
            let mut mesh = conduction::prepare_geometry(&board, &g, r.conduction.clone());
            mesh.import_diagnostics = imported.diagnostics;
            // Minimal physical outline/union/bore view for maps, including
            // unmeshed regions. No second per-source region/contact/island graph.
            self.geometry = Some(json!({"outline":g.outline,"layers":g.layers,"holes":g.holes}));
            self.mesh = Some(mesh);
            self.preparation_key = key;
        }
        if start.elapsed().as_secs_f64() > r.limits.max_seconds {
            return Err(error("DC_DEADLINE", "DC preparation exceeded budget"));
        }
        let mesh = self.mesh.as_ref().unwrap();
        if mesh.status == "invalid"
            || mesh
                .import_diagnostics
                .iter()
                .any(|d| matches!(d.severity, crate::Severity::Error))
        {
            return Err(error(
                "DC_INVALID_BOARD",
                "Board import/conduction preparation is invalid",
            ));
        }
        let (mut out, system) = system::solve(mesh, r, self.matrix.take(), start)?;
        self.matrix = Some(system);
        out["reused_preparation"] = json!(reused_preparation);
        Ok(out)
    }
}
fn standalone(c: &Conductor, r: &Request) -> Result<Value> {
    if ![c.length_mm, c.width_mm, c.thickness_mm, c.conductivity_s_m]
        .iter()
        .all(|v| positive(*v))
        || c.state.trim().is_empty()
    {
        return Err(error(
            "MISSING_CONDUCTOR_PHYSICS",
            "Positive dimensions, conductivity and explicit material state required",
        ));
    }
    if r.elements.len() != 1 || matches!(r.elements[0].kind, ElementKind::Resistance) {
        return Err(error(
            "UNSUPPORTED_DC_MODEL",
            "Standalone conductor needs one current or voltage excitation",
        ));
    }
    let resistance = conduction::profile::metres(c.length_mm)
        / (c.conductivity_s_m * conduction::profile::square_metres(c.width_mm * c.thickness_mm));
    let e = &r.elements[0];
    let (voltage, current) = if matches!(e.kind, ElementKind::Current) {
        (resistance * e.value, e.value)
    } else {
        (e.value, e.value / resistance)
    };
    let losses = current * current * resistance;
    if ![resistance, voltage, current, losses]
        .iter()
        .all(|x| x.is_finite())
    {
        return Err(error(
            "DC_NONFINITE",
            "Standalone coefficients/results overflow",
        ));
    }
    Ok(
        json!({"status":"complete","diagnostics":[],"conditional":false,"standalone":{
        "resistance_ohm":resistance,"voltage_v":voltage,"current_a":current,"loss_w":losses,
        "current_density_a_mm2":current/(c.width_mm*c.thickness_mm),"conductor":c},
        "quality":{"relative_residual":0.,"current_balance_a":0.,"power_balance_w":0.},"reused_preparation":false,"reused_factorization":false}),
    )
}

pub(super) struct Union {
    parent: Vec<usize>,
}
impl Union {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn root(&mut self, mut i: usize) -> usize {
        while self.parent[i] != i {
            self.parent[i] = self.parent[self.parent[i]];
            i = self.parent[i];
        }
        i
    }
    fn join(&mut self, a: usize, b: usize) {
        let a = self.root(a);
        let b = self.root(b);
        self.parent[a.max(b)] = a.min(b);
    }
}
pub(super) fn unique_ids<'a>(mut ids: impl Iterator<Item = &'a str>) -> bool {
    let mut seen = BTreeMap::new();
    ids.all(|id| !id.is_empty() && seen.insert(id, ()).is_none())
}
