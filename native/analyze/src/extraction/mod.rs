//! CPU/F64 PEC quasi-TEM cross sections. Geometry, FEM and propagation are separate.
mod field;
pub mod polygon_section;
pub mod board_section;
mod geometry;
pub mod local;
pub mod losses;
pub mod propagation;
pub use field::{solve_field, Field};
pub use geometry::{Conductor, Dielectric, Geometry};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Instant;
pub const EPS0: f64 = 8.8541878128e-12;
pub const MU0: f64 = 1.25663706212e-6;
type Result<T> = std::result::Result<T, String>;
fn error(code: &str, message: impl std::fmt::Display) -> String {
    format!("{code}: {message}")
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MeshOptions {
    pub max_nodes: Option<usize>,
    pub max_triangles: Option<usize>,
    pub max_refinements: Option<usize>,
    pub max_edge_mm: Option<f64>,
    pub domain_scale: Option<f64>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub line: Value,
    pub frequencies: Vec<f64>,
    pub mesh: Option<MeshOptions>,
    pub budget: Option<crate::electrical::Budget>,
}
pub(super) struct Limits {
    nodes: usize,
    triangles: usize,
    memory: usize,
    seconds: f64,
    start: Instant,
}
impl Limits {
    fn check(&self, bytes: usize) -> Result<()> {
        if bytes > self.memory {
            return Err(error(
                "SECTION_MEMORY_LIMIT",
                format!("planned {bytes} bytes > {}", self.memory),
            ));
        }
        if self.start.elapsed().as_secs_f64() > self.seconds {
            return Err(error("SECTION_TIMEOUT", "native time budget exceeded"));
        }
        Ok(())
    }
}
fn change(a: &Value, b: &Value) -> f64 {
    let mut max: f64 = 0.;
    for key in [
        "capacitanceFPerM",
        "vacuumCapacitanceFPerM",
        "inductanceHPerM",
    ] {
        let aa = a[key].as_array().unwrap();
        let bb = b[key].as_array().unwrap();
        for i in 0..aa.len() {
            for j in 0..aa.len() {
                let x = aa[i][j].as_f64().unwrap();
                let y = bb[i][j].as_f64().unwrap();
                // Absolute floor = 1e-3 times the largest self term, declared for weak mutual entries.
                let floor = 1e-3
                    * aa.iter()
                        .enumerate()
                        .map(|(k, r)| r[k].as_f64().unwrap().abs())
                        .fold(0., f64::max);
                max = max.max((x - y).abs() / x.abs().max(floor));
            }
        }
    }
    for key in ["impedanceOhm", "delaySPerM"] {
        let aa = a["modes"].as_array().unwrap();
        let bb = b["modes"].as_array().unwrap();
        for (i, r) in aa.iter().enumerate() {
            let x = r[key].as_f64().unwrap();
            max = max.max((x - bb[i][key].as_f64().unwrap()).abs() / x.abs());
        }
    }
    max
}
pub fn run(request: Request) -> Value {
    if request.line["kind"]=="polygon-section" {return polygon_section::run_transport(request);}
    let start = Instant::now();
    let mut convergence = vec![];
    let result = (|| -> Result<Value> {
        let g = geometry::parse(&request.line)?;
        let loss_config = losses::Configuration::parse(&request.line, &g)?;
        let m = request.mesh.as_ref();
        let nodes = m.and_then(|x| x.max_nodes).unwrap_or(200000);
        let triangles = m.and_then(|x| x.max_triangles).unwrap_or(400000);
        let refinements = m.and_then(|x| x.max_refinements).unwrap_or(5);
        let scale = m.and_then(|x| x.domain_scale).unwrap_or(8.);
        let step = m
            .and_then(|x| x.max_edge_mm)
            .map(|x| x * 1e-3)
            .unwrap_or(g.feature() / 6.);
        let seconds = request
            .budget
            .as_ref()
            .and_then(|b| b.max_seconds)
            .unwrap_or(120.);
        let mib = request
            .budget
            .as_ref()
            .and_then(|b| b.max_memory_mi_b)
            .unwrap_or(768.);
        if nodes < 16
            || nodes > 300000
            || triangles < 16
            || triangles > 600000
            || refinements < 1
            || refinements > 5
            || !step.is_finite()
            || step <= 0.
            || step > g.scale()
            || !scale.is_finite()
            || !(4. ..=32.).contains(&scale)
            || !seconds.is_finite()
            || seconds <= 0.
            || !mib.is_finite()
            || !(1. ..=4096.).contains(&mib)
        {
            return Err(error("SECTION_BUDGET", "invalid mesh/budget caps"));
        }
        let limits = Limits {
            nodes,
            triangles,
            memory: (mib * 1048576.) as usize,
            seconds,
            start,
        };
        limits.check(65536)?;
        let n = g.conductors.len();
        let f = &request.frequencies;
        if f.is_empty()
            || f.len() > 8192
            || f.len() * (2 * n).pow(2) > 131072
            || f.iter()
                .enumerate()
                .any(|(i, x)| !x.is_finite() || *x < 0. || i > 0 && *x <= f[i - 1])
        {
            return Err(error(
                "SECTION_FREQUENCY",
                "finite ordered nonnegative frequency axis required; output cap 131072",
            ));
        }
        let max_frequency = 299792458. / (20. * g.scale() * g.max_epsilon().sqrt());
        if f.last().unwrap() > &max_frequency {
            return Err(error("SECTION_APPLICABILITY",format!("quasi-TEM geometric screening f <= {max_frequency} Hz; heuristic, not full-wave proof")));
        }
        if let Some(c) = &loss_config {
            c.screen(f)?;
        }
        if loss_config.is_some()
            && f.len()
                * ((2 * n).pow(2)
                    + n * n * (6 + n + g.reference_y_m.len() + 2 * g.dielectrics.len())
                    + n * 32)
                > 131072
        {
            return Err(error("SECTION_BUDGET","loss coefficient/mode/contribution arrays exceed bounded 131072 numeric output plan"));
        }
        let mut last: Option<Value> = None;
        let mut last_loss = None;
        let mut accepted = None;
        for level in 0..=refinements {
            let h = step / 2_f64.powi(level as i32);
            let geom = g.domain(scale);
            let field = field::solve(&geom, h, &limits)?;
            let p = propagation::parameters(&field.capacitance, &field.vacuum_capacitance)?;
            let loss_integrals = loss_config
                .as_ref()
                .map(|_| losses::integrals(&field))
                .transpose()?;
            let loss_change = loss_integrals
                .as_ref()
                .zip(last_loss.as_ref())
                .map(|(a, b)| losses::convergence(a, b, loss_config.as_ref().unwrap()));
            let delta = last.as_ref().map(|v| change(&p, v));
            convergence.push(json!({"kind":"mesh","level":level,"stepM":h,"domainScale":scale,"nodes":field.nodes.len(),"triangles":field.triangles.len(),"parameters":p,"maxRelativeChange":delta,"lossConvergence":loss_change,"lossIntegrals":loss_integrals.as_ref().map(|p|json!({"surfaces":p.surfaces.iter().map(|s|json!({"id":s.id,"gramPerM":s.gram_per_m,"integratedCurrentAPerA":s.integrated_current_a_per_a})).collect::<Vec<_>>(),"regions":p.regions}))}));
            if delta.is_some_and(|d| d <= 0.01)
                && (loss_config.is_none()
                    || loss_change.as_ref().is_some_and(|d| {
                        d["maxNormalizedChange"].as_f64().is_some_and(|x| x <= 0.02)
                    }))
            {
                accepted = Some((field, p, h, loss_integrals));
                break;
            }
            last = Some(p);
            last_loss = loss_integrals;
        }
        let (mut field, mut p, h, mut loss_integrals) = accepted.ok_or_else(|| {
            error(
                "SECTION_CONVERGENCE",
                "mesh did not meet 1% C/C0/L/Z/delay and active 2% loss-power criteria within caps",
            )
        })?;
        let mut domain_ok = false;
        for k in 1..=3 {
            let s = scale * 2_f64.powi(k);
            if s > 64. {
                break;
            }
            let wider = field::solve(&g.domain(s), h, &limits)?;
            let pp = propagation::parameters(&wider.capacitance, &wider.vacuum_capacitance)?;
            let wider_loss = loss_config
                .as_ref()
                .map(|_| losses::integrals(&wider))
                .transpose()?;
            let loss_change = wider_loss
                .as_ref()
                .zip(loss_integrals.as_ref())
                .map(|(a, b)| losses::convergence(a, b, loss_config.as_ref().unwrap()));
            let delta = change(&pp, &p);
            convergence.push(json!({"kind":"domain","domainScale":s,"stepM":h,"nodes":wider.nodes.len(),"triangles":wider.triangles.len(),"parameters":pp,"maxRelativeChange":delta,"lossConvergence":loss_change,"lossIntegrals":wider_loss.as_ref().map(|p|json!({"surfaces":p.surfaces.iter().map(|s|json!({"id":s.id,"gramPerM":s.gram_per_m,"integratedCurrentAPerA":s.integrated_current_a_per_a})).collect::<Vec<_>>(),"regions":p.regions}))}));
            field = wider;
            p = pp;
            loss_integrals = wider_loss;
            if delta <= 0.01
                && (loss_config.is_none()
                    || loss_change.as_ref().is_some_and(|d| {
                        d["maxNormalizedChange"].as_f64().is_some_and(|x| x <= 0.02)
                    }))
            {
                domain_ok = true;
                break;
            }
        }
        if !domain_ok {
            return Err(error(
                "SECTION_CONVERGENCE",
                "outer domain did not meet independent 1% criterion",
            ));
        }
        limits.check(field.nodes.len() * 512)?;
        let samples = loss_config
            .as_ref()
            .map(|c| losses::samples(&p, loss_integrals.as_ref().unwrap(), c, f, g.length))
            .transpose()?;
        let network = if let Some(length) = g.length {
            Some(if let Some(s) = &samples {
                propagation::lossy_network(s, length, 50.)?
            } else {
                propagation::network(&p, f, length, 50.)?
            })
        } else {
            None
        };
        if let Some(s) = samples {
            p["frequencyDependent"] = json!({"frequenciesHz":f,"samples":s,"conductorModel":if loss_config.as_ref().unwrap().conductivity.is_some(){"smooth good-conductor surface impedance perturbation"}else{"explicit PEC"},"dielectricModel":"regional electrostatic energy participation perturbation","delayDefinition":"phase delay beta*length/omega; not group delay"});
            // Static R/G are explicit ideal assumptions only on the legacy path.
            p.as_object_mut().unwrap().remove("resistanceOhmPerM");
            p.as_object_mut().unwrap().remove("conductanceSPerM");
        }
        let assumptions = if loss_config.is_some() {
            vec!["Weak-loss quasi-TEM perturbation using smooth good-conductor surface impedance and regional dielectric energy participation; conditional at fixed material parameters within requested band","Absent conductors explicitly retain PEC; no conductivity or reference thickness defaults","Infinite reference planes, no roughness, mask, radiation, material dispersion or thermal feedback; no DC/skin transition model","Surface currents are normalized 1 A basis functions, not PCB source currents; sharp corner peaks are mesh dependent","Phase delay is beta*length/omega; insertion loss includes mismatch and is not pure alpha","Mesh and outer-domain loss-power convergence <=2%; legacy C/C0/L/Z/delay <=1%; numerical convergence does not establish model applicability"]
        } else {
            vec!["PEC, zero internal L/conductor loss/roughness; R=0 and G=0 are explicit ideal assumptions, not real copper/dielectric losses","Isotropic positive nondispersive nonmagnetic dielectrics, lossTangent=0; no mask","Infinite ideal reference plane(s); finite artificial outer boundary has natural zero normal displacement, not a grounded screen","Mesh and outer-domain convergence do not prove quasi-TEM applicability"]
        };
        Ok(
            json!({"status":"complete","diagnostics":[],"input":request.line,"geometry":field.geometry,"parameters":p,"field":field,"lossIntegrals":loss_integrals,"convergence":convergence,"network":network,"frequenciesHz":f,
   "applicability":{"maxFrequencyHz":max_frequency,"rule":"max transverse physical dimension * sqrt(max epsilon) <= wavelength0/20; conservative heuristic only","fullWaveValidated":false},
   "assumptions":assumptions,
   "limits":{"maxNodes":nodes,"maxTriangles":triangles,"maxRefinements":refinements,"maxDomainScale":64,"maxOutputCoefficients":131072,"memoryPlanCapBytes":limits.memory,"memoryMetric":"conservative admission plan, not hard OS RSS cap; disposable process enforces deadline"},"convergenceTolerance":0.01,"mutualAbsoluteFloorRelativeToSelf":0.001}),
        )
    })();
    match result {
        Ok(mut v) => {
            v["elapsedMs"] = json!(start.elapsed().as_secs_f64() * 1000.);
            v
        }
        Err(e) => {
            let (code, _) = e.split_once(':').unwrap_or(("SECTION_REJECTED", &e));
            json!({"status":"rejected","diagnostics":[{"code":code,"message":e,"severity":"error"}],"convergence":convergence,"elapsedMs":start.elapsed().as_secs_f64()*1000.})
        }
    }
}
