use super::{accurate::Sum, board_mesh, error, Result};
use crate::electrical::{
    coupled::{composition, physical, Model, System},
    frequency::{declared_band, Instance, OutputPort, Resource},
    Budget,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{collections::BTreeSet, time::Instant};

const COPPER_ID: &str = "checked-board-copper";

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Attachment {
    pub id: String,
    pub electrodes: [String; 2],
    pub model: Resource,
}

pub(super) struct Validated {
    pub id: String,
    pub electrodes: [String; 2],
    resource: Resource,
    model: Model,
    r: f64,
    l: f64,
    c: f64,
}

fn input(detail: &str) -> String {
    error("BOARD_PACKAGE_INPUT", detail)
}

pub(super) fn validate(
    attachments: Vec<Attachment>,
    geometry: &board_mesh::Request,
    axis: &[f64],
) -> Result<Vec<Validated>> {
    if attachments.is_empty() {
        return Ok(Vec::new());
    }
    if attachments.len() > 8
        || geometry.ports.len() + attachments.len() > 16
        || geometry.electrodes.len() > 32
    {
        return Err(input(
            "at most 8 attachments, 16 external plus private ports, and 32 electrodes",
        ));
    }
    let available: BTreeSet<_> = geometry
        .electrodes
        .iter()
        .map(|e| e.name.as_str())
        .collect();
    if available.len() != geometry.electrodes.len() {
        return Err(input("distinct original electrode names required"));
    }
    let mut ids = BTreeSet::new();
    for a in &attachments {
        if a.id.trim().is_empty()
            || a.id.len() > 256
            || a.id == COPPER_ID
            || !ids.insert(&a.id)
            || a.electrodes[0] == a.electrodes[1]
            || a.electrodes
                .iter()
                .any(|name| !available.contains(name.as_str()))
        {
            return Err(input(
                "unique package identity and distinct existing physical pad faces required",
            ));
        }
    }
    attachments
        .into_iter()
        .map(|a| {
            let resource = a.model;
            if composition::source_plan(&resource) > 64 * 1024
                || resource.kind != "network"
                || resource.ports != ["terminals"]
                || resource
                    .analyses
                    .as_ref()
                    .is_some_and(|a| !a.iter().any(|s| s == "networkParameters"))
                || resource.validity.as_ref().is_some_and(|v| {
                    v.get("status")
                        .is_some_and(|s| s != "conditional" && s != "valid")
                })
            {
                return Err(input("bounded explicit passive package resource required"));
            }
            let [lo, hi] = declared_band(&resource)?;
            if axis.iter().any(|&f| f < lo || f > hi) {
                return Err(error(
                    "BOARD_PACKAGE_BAND",
                    "package authored band must include every requested frequency",
                ));
            }
            let model: Model = serde_json::from_value(resource.data.clone())
                .map_err(|_| input("explicit floating two-terminal R/L/C descriptor required"))?;
            let series = |entries: &[(usize, usize, f64)]| match entries {
                [] => Ok(0.),
                [(0, 0, x)] if x.is_finite() && *x > 0. => Ok(*x),
                _ => Err(input("positive single series R/L coefficient required")),
            };
            let r = series(&model.resistance_ohm)?;
            let l = series(&model.inductance_h)?;
            let current = r > 0. || l > 0.;
            let capacitor = !model.capacitance_f.is_empty();
            let internal = current && capacitor;
            let p = if internal { 2 } else { 0 };
            let c = if capacitor {
                let value = model.capacitance_f.first().map_or(0., |e| e.2);
                if !value.is_finite()
                    || value <= 0.
                    || model.capacitance_f
                        != [(p, p, value), (1, 1, value), (p, 1, -value), (1, p, -value)]
                {
                    return Err(input("explicit floating capacitance stamp required"));
                }
                value
            } else {
                0.
            };
            let names = if internal {
                vec!["positive", "negative", "internal"]
            } else {
                vec!["positive", "negative"]
            };
            let branch = if current {
                vec![(0, 0, 1.), (if internal { 2 } else { 1 }, 0, -1.)]
            } else {
                vec![]
            };
            let gauges = vec![vec![1.; names.len()]];
            let conserved = if internal {
                vec![vec![0., 1., 0.]]
            } else {
                vec![]
            };
            let terminals =
                json!([{"positive":{"node":"positive"},"negative":{"node":"negative"}}]);
            if (!current && !capacitor)
                || model.format != "coupled-rlc"
                || model.reference_topology != "infinity"
                || model.potential_nodes != names
                || model.current_dofs != if current { vec!["series"] } else { vec![] }
                || model.incidence != branch
                || model.port_incidence != [(0, 0, 1.), (1, 0, -1.)]
                || model.structural_gauge_modes != gauges
                || model.conserved_potential_modes != conserved
                || serde_json::to_value(&model.port_terminals).ok().as_ref() != Some(&terminals)
            {
                return Err(input(
                    "only the canonical explicit floating series R/L/C circuit is supported",
                ));
            }
            Ok(Validated {
                id: a.id,
                electrodes: a.electrodes,
                resource,
                model,
                r,
                l,
                c,
            })
        })
        .collect()
}

pub(super) fn output_reserve(packages: &[Validated], columns: usize) -> usize {
    packages.iter().fold(0, |n, p| {
        n + composition::source_plan(&p.resource) + (columns + 1) * (512 + 6 * p.id.len())
    })
}

struct Indices {
    values: Vec<usize>,
}

impl Indices {
    fn new(recovery: &physical::Recovery, id: &str, model: &Model, size: usize) -> Result<Self> {
        let mut values = Vec::with_capacity(model.potential_nodes.len() + model.current_dofs.len());
        for name in &model.potential_nodes {
            let node = recovery
                .potentials
                .iter()
                .find(|p| p.instance_id == id && p.node == *name)
                .and_then(|p| p.index)
                .filter(|&i| i < size)
                .ok_or_else(|| {
                    error(
                        "BOARD_PACKAGE_RECOVERY",
                        "ungrounded physical potential mapping required",
                    )
                })?;
            values.push(node);
        }
        for name in &model.current_dofs {
            let dof = recovery
                .currents
                .iter()
                .find(|p| p.instance_id == id && p.dof == *name)
                .map(|p| p.index)
                .filter(|&i| i < size)
                .ok_or_else(|| {
                    error(
                        "BOARD_PACKAGE_RECOVERY",
                        "physical current mapping required",
                    )
                })?;
            values.push(dof);
        }
        if values.iter().collect::<BTreeSet<_>>().len() != values.len() {
            return Err(error(
                "BOARD_PACKAGE_RECOVERY",
                "composition merged distinct local coordinates",
            ));
        }
        Ok(Self { values })
    }

    fn phase(&self, state: &[f64]) -> Vec<f64> {
        self.values.iter().map(|&i| state[i]).collect()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct PackageSample<'a> {
    id: &'a str,
    current_a: ComplexSample,
    pub loss_w: f64,
}

#[derive(Serialize)]
pub(super) struct ComplexSample {
    pub real: f64,
    pub imaginary: f64,
}

impl PackageSample<'_> {
    pub fn current(&self) -> [f64; 2] {
        [self.current_a.real, self.current_a.imaginary]
    }
}

pub(super) struct Recovery<'a> {
    copper: Indices,
    packages: Vec<(Indices, &'a Validated)>,
}

pub(super) struct Joint<'a> {
    pub system: System,
    pub recovery: Recovery<'a>,
    pub size: usize,
}

pub(super) fn compile<'a>(
    copper: &Model,
    packages: &'a [Validated],
    external: usize,
    budget: &Budget,
) -> Result<Joint<'a>> {
    let count = external + packages.len();
    let copper_ports: Vec<_> = (0..count).map(|i| format!("copper-port-{i}")).collect();
    let mut instances = vec![Instance {
        id: COPPER_ID.into(),
        ports: copper_ports.clone(),
        model: Resource {
            id: COPPER_ID.into(),
            kind: "network".into(),
            ports: copper_ports,
            data: serde_json::to_value(copper).map_err(|e| input(&e.to_string()))?,
            source: None,
            validity: None,
            analyses: Some(vec!["networkParameters".into()]),
        },
    }];
    instances.extend(packages.iter().map(|a| Instance {
        id: a.id.clone(),
        ports: a.resource.ports.clone(),
        model: a.resource.clone(),
    }));
    let topology = composition::Topology {
        instances,
        connections: (0..packages.len())
            .map(|i| [external + i, count + i])
            .collect(),
        connection_contacts: Some(vec![physical::Contacts::Both; packages.len()]),
        ports: (0..external)
            .map(|i| OutputPort {
                name: format!("external-{i}"),
                index: i,
            })
            .collect(),
        instance_inductance_h: Vec::new(),
    };
    let built = composition::build_network_parameters(&topology, Some(budget), Instant::now())?;
    drop(topology);
    if !built.recovery.mapped {
        return Err(error(
            "BOARD_PACKAGE_RECOVERY",
            "mapped finite two-contact composition required",
        ));
    }
    let size = built.model.potential_nodes.len() + built.model.current_dofs.len();
    let recovery = Recovery {
        copper: Indices::new(&built.recovery, COPPER_ID, copper, size)?,
        packages: packages
            .iter()
            .map(|p| Ok((Indices::new(&built.recovery, &p.id, &p.model, size)?, p)))
            .collect::<Result<_>>()?,
    };
    let system_budget = composition::remaining_budget(Some(budget), built.assembly_plan)?;
    let system = System::compile(&built.model, external, Some(&system_budget))?;
    Ok(Joint {
        system,
        recovery,
        size,
    })
}

impl<'a> Recovery<'a> {
    pub fn copper_state(&self, re: &[f64], im: &[f64]) -> [Vec<f64>; 2] {
        [self.copper.phase(re), self.copper.phase(im)]
    }
    pub fn package_states(&self, w: f64, re: &[f64], im: &[f64]) -> Result<Vec<PackageSample<'a>>> {
        self.packages
            .iter()
            .map(|(map, p)| p.checked_state(w, [&map.phase(re), &map.phase(im)]))
            .collect()
    }
}

impl Validated {
    fn checked_state(&self, w: f64, state: [&[f64]; 2]) -> Result<PackageSample<'_>> {
        let nv = self.model.potential_nodes.len();
        let mut in_pos = [0.; 2];
        let current = state.map(|s| s.get(nv).copied().unwrap_or(0.));
        for k in 0..2 {
            let v = state[k];
            let other = state[1 - k];
            let sign = if k == 0 { -1. } else { 1. };
            let c_from = if nv == 3 { 2 } else { 0 };
            let ic = sign * w * self.c * (other[c_from] - other[1]);
            let has_current = !self.model.current_dofs.is_empty();
            in_pos[k] = if has_current { current[k] } else { ic };
            if nv == 3 && !balanced(current[k] - ic, current[k].abs() + ic.abs()) {
                return Err(error(
                    "BOARD_PACKAGE_KCL",
                    "series package internal KCL failed",
                ));
            }
            if has_current {
                let r = self.r * current[k];
                let l = sign * w * self.l * current[1 - k];
                let drop = v[0] - v[if nv == 3 { 2 } else { 1 }];
                if !balanced(r + l - drop, r.abs() + l.abs() + drop.abs()) {
                    return Err(error(
                        "BOARD_PACKAGE_KVL",
                        "series package branch KVL failed",
                    ));
                }
            }
        }
        let loss = self.r * (current[0].powi(2) + current[1].powi(2));
        let power = (0..2)
            .fold(Sum::default(), |s, k| {
                s.add(
                    Sum::from(state[k][0])
                        .sub(Sum::from(state[k][1]))
                        .mul(in_pos[k]),
                )
            })
            .value();
        if !loss.is_finite()
            || !power.is_finite()
            || (power - loss).abs() > 1e-12 + 1e-8 * (power.abs() + loss)
        {
            return Err(error(
                "BOARD_PACKAGE_POWER",
                "package terminal power disagrees with its Joule loss",
            ));
        }
        Ok(PackageSample {
            id: &self.id,
            current_a: ComplexSample {
                real: in_pos[0],
                imaginary: in_pos[1],
            },
            loss_w: loss,
        })
    }
}

fn balanced(residual: f64, scale: f64) -> bool {
    residual.is_finite()
        && scale.is_finite()
        && residual.abs() <= 1e-8 * scale.max(f64::MIN_POSITIVE)
}

pub(super) fn copper_loss(model: &Model, state: &[Vec<f64>; 2]) -> f64 {
    let offset = model.potential_nodes.len();
    state
        .iter()
        .flat_map(|x| {
            model
                .resistance_ohm
                .iter()
                .map(move |&(i, j, r)| Sum::from(x[offset + i]).mul(x[offset + j]).mul(r))
        })
        .fold(Sum::default(), Sum::add)
        .value()
}

pub(super) fn check_copper_power(
    model: &Model,
    state: &[Vec<f64>; 2],
    injections: [&[f64]; 2],
    copper_loss: f64,
) -> Result<()> {
    let mut absorbed = Sum::default();
    for (phase, injection) in state.iter().zip(injections) {
        for &(node, port, coefficient) in &model.port_incidence {
            absorbed = absorbed.add(Sum::from(phase[node]).mul(injection[port]).mul(coefficient));
        }
    }
    let power = absorbed.value();
    if !power.is_finite()
        || !copper_loss.is_finite()
        || (power - copper_loss).abs() > 1e-12 + 1e-8 * (power.abs() + copper_loss.abs())
    {
        return Err(error(
            "BOARD_PACKAGE_COPPER_POWER",
            "original copper terminal power disagrees with its Joule loss",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electrical::Counters;
    use std::f64::consts::TAU;

    fn circuit(r: f64, l: f64, c: f64) -> serde_json::Value {
        let series = r > 0. || l > 0.;
        let internal = series && c > 0.;
        let cap = if internal { 2 } else { 0 };
        json!({
            "format": "coupled-rlc", "referenceTopology": "infinity",
            "potentialNodes": if internal { vec!["positive","negative","internal"] } else { vec!["positive","negative"] },
            "currentDofs": if series { vec!["series"] } else { vec![] },
            "incidence": if series { vec![(0,0,1.),(if internal {2} else {1},0,-1.)] } else { vec![] },
            "portIncidence": [(0,0,1.),(1,0,-1.)],
            "resistanceOhm": if r > 0. { vec![(0,0,r)] } else { vec![] },
            "inductanceH": if l > 0. { vec![(0,0,l)] } else { vec![] },
            "capacitanceF": if c > 0. { vec![(cap,cap,c),(1,1,c),(cap,1,-c),(1,cap,-c)] } else { vec![] },
            "structuralGaugeModes": [vec![1.; if internal {3} else {2}]],
            "conservedPotentialModes": if internal { vec![vec![0.,1.,0.]] } else { vec![] },
            "portTerminals": [{"positive":{"node":"positive"},"negative":{"node":"negative"}}],
        })
    }

    fn attachment(data: serde_json::Value) -> Attachment {
        Attachment {
            id: "P1".into(),
            electrodes: ["terminal-8".into(), "terminal-9".into()],
            model: Resource {
                id: "explicit-package".into(),
                kind: "network".into(),
                ports: vec!["terminals".into()],
                data,
                source: None,
                validity: Some(json!({"status":"conditional","domain":{"bandHz":[900,1100]}})),
                analyses: Some(vec!["networkParameters".into()]),
            },
        }
    }

    #[test]
    fn floating_rlc_attachment_recovers_copper_and_package_against_analytic_admittance() {
        let mut geometry: board_mesh::Request = serde_json::from_str(include_str!(
            "../../../../../tests/fixtures/analysis-local-extraction/board-integration/via-contact-request-d1-m1.json"
        )).unwrap();
        geometry.ports.truncate(1);
        let mut copper: Model = serde_json::from_value(circuit(100., 0., 0.)).unwrap();
        copper.port_incidence.extend([(0, 1, 1.), (1, 1, -1.)]);
        let terminal = copper.port_terminals.as_ref().unwrap()[0].clone();
        copper.port_terminals.as_mut().unwrap().push(terminal);
        let budget = Budget {
            max_memory_mi_b: Some(64.),
            max_seconds: Some(10.),
        };
        let w = TAU * 1000.;
        for (r, l, c) in [(25., 1e-3, 1e-5), (0., 0., 1e-5)] {
            let resource = attachment(circuit(r, l, c));
            let package = validate(vec![resource], &geometry, &[1000.]).unwrap();
            let mut joint = compile(&copper, &package, 1, &budget).unwrap();
            joint.system.add_port_admittance(0, 1. / 50., 0.).unwrap();
            let mut rhs = joint.system.port_rhs(&[1.]);
            rhs.resize(2 * joint.size, 0.);
            let state = joint
                .system
                .solve(w, rhs, &vec![0.; joint.size], &mut Counters::default())
                .unwrap();
            let (real, imaginary) = state.split_at(joint.size);
            let cu = joint.recovery.copper_state(real, imaginary);
            let sample = joint.recovery.package_states(w, real, imaginary).unwrap();
            let [ir, ii] = sample[0].current();
            let x = w * l - 1. / (w * c);
            let (yr, yi) = (r / (r * r + x * x), -x / (r * r + x * x));
            let g = 1. / 50. + 1. / 100. + yr;
            let (vr, vi) = (g / (g * g + yi * yi), -yi / (g * g + yi * yi));
            assert!((cu[0][0] - cu[0][1] - vr).abs() < 1e-10);
            assert!((cu[1][0] - cu[1][1] - vi).abs() < 1e-10);
            assert!(
                (ir - (vr * yr - vi * yi)).abs() < 1e-10
                    && (ii - (vr * yi + vi * yr)).abs() < 1e-10
            );
            let cu_loss = copper_loss(&copper, &cu);
            check_copper_power(
                &copper,
                &cu,
                [&[1. - vr / 50., -ir], &[-vi / 50., -ii]],
                cu_loss,
            )
            .unwrap();
            let total_loss = cu_loss + sample[0].loss_w;
            assert!((cu_loss - (vr * vr + vi * vi) / 100.).abs() < 1e-10);
            assert!((sample[0].loss_w - r * (ir * ir + ii * ii)).abs() < 1e-10);
            assert!((total_loss - (vr - (vr * vr + vi * vi) / 50.)).abs() < 1e-10);
        }
        assert!(validate(
            vec![attachment(circuit(25., 0., 0.))],
            &geometry,
            &[1001., 2000.]
        )
        .is_err());
        let mut grounded = circuit(0., 0., 1e-5);
        grounded["capacitanceF"] = json!([[0, 0, 1e-5]]);
        assert!(validate(vec![attachment(grounded)], &geometry, &[1000.]).is_err());
    }
}
