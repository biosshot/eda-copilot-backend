//! Exact zero-frequency response on the authored finite local volume domain.
use super::{board_mesh, board_prepare, error, volume_current, volume_dc::ElectrodeDc, AllocationBudget, Result};
use faer::{prelude::Solve, Mat, Side};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub geometry: board_mesh::Request,
    #[serde(default)]
    pub drive: Vec<f64>,
    #[serde(default)]
    pub loads_ohm: Vec<Option<f64>>,
    #[serde(default)]
    pub observables_only: bool,
}

pub(super) struct Loaded {
    pub(super) volts: Vec<f64>,
    pub(super) components: Vec<usize>,
    pub(super) load_loss_w: f64,
    normalized_residual: f64,
}

pub(super) fn loaded(
    conductance: &[Vec<f64>],
    galvanic: &[usize],
    ports: &[[usize; 2]],
    drive: &[f64],
    loads: &[Option<f64>],
) -> Result<Loaded> {
    let mut components = galvanic.to_vec();
    let mut rhs = vec![0.; galvanic.len()];
    let mut matrix = conductance.to_vec();
    for ((&[a, b], &source), load) in ports.iter().zip(drive).zip(loads) {
        rhs[a] += source;
        rhs[b] -= source;
        if let Some(load) = load {
            let g = 1. / load;
            matrix[a][a] += g;
            matrix[b][b] += g;
            matrix[a][b] -= g;
            matrix[b][a] -= g;
            let from = components[b];
            let to = components[a];
            for component in &mut components {
                if *component == from {
                    *component = to;
                }
            }
        }
    }
    if rhs
        .iter()
        .chain(matrix.iter().flatten())
        .any(|v| !v.is_finite())
    {
        return Err(error("DC_RANGE", "loaded DC coefficients overflow"));
    }
    let mut seen = BTreeSet::new();
    let mut free = Vec::new();
    for (i, &component) in components.iter().enumerate() {
        if seen.insert(component) {
            let mut net = super::accurate::Sum::default();
            let mut magnitude = 0.;
            for ((&[a, b], &source), _) in ports.iter().zip(drive).zip(loads) {
                // Stamp cross-island sources directly to preserve a weak
                // unbalanced drive in the presence of a large internal drive.
                if (components[a] == component) != (components[b] == component) {
                    net = net.add(super::accurate::Sum::from(if components[a] == component {
                        source
                    } else {
                        -source
                    }));
                    magnitude += source.abs();
                }
            }
            if !magnitude.is_finite() || net.value().abs() > 1e-12 * magnitude {
                return Err(error(
                    "DC_INCOMPATIBLE",
                    "nonzero net DC injection on an isolated loaded conductor island",
                ));
            }
        } else {
            free.push(i);
        }
    }
    let mut volts = vec![0.; rhs.len()];
    if !free.is_empty() {
        let a = Mat::from_fn(free.len(), free.len(), |i, j| {
            let (i, j) = (free[i], free[j]);
            matrix[i][j] * 0.5 + matrix[j][i] * 0.5
        });
        let factor = a.llt(Side::Lower).map_err(|_| {
            error(
                "DC_FACTOR",
                "unresolved loaded conductance after numerical gauge selection",
            )
        })?;
        let x = factor.solve(Mat::from_fn(free.len(), 1, |i, _| rhs[free[i]]));
        for (i, &index) in free.iter().enumerate() {
            volts[index] = x[(i, 0)];
        }
    }
    let mut normalized_residual = 0_f64;
    for (i, row) in matrix.iter().enumerate() {
        let mut residual = super::accurate::Sum::from(rhs[i]);
        let mut scale = rhs[i].abs();
        for (&g, &v) in row.iter().zip(&volts) {
            residual = residual.sub(super::accurate::Sum::from(g).mul(v));
            scale += (g * v).abs();
        }
        normalized_residual =
            normalized_residual.max(residual.value().abs() / scale.max(f64::MIN_POSITIVE));
    }
    if volts.iter().any(|v| !v.is_finite()) || normalized_residual > 1e-8 {
        return Err(error(
            "DC_RESIDUAL",
            "loaded conductance fails full electrode KCL",
        ));
    }
    let load_loss_w = ports
        .iter()
        .zip(loads)
        .filter_map(|(&[a, b], load)| load.map(|r| ((volts[a] - volts[b]) / r.sqrt()).powi(2)))
        .sum();
    Ok(Loaded {
        volts,
        components,
        load_loss_w,
        normalized_residual,
    })
}

pub fn run(board: &crate::model::Board, request: Request, seconds: f64) -> Result<Value> {
    let started = Instant::now();
    let max_seconds = board_prepare::verification_limit("P7_VERIFY_SECONDS", 300, 1, 3600)? as f64;
    if !seconds.is_finite() || seconds <= 0. || seconds > max_seconds {
        return Err(error("DEADLINE", "invalid remaining local DC wall budget"));
    }
    let remaining = || {
        Duration::from_secs_f64(seconds)
            .checked_sub(started.elapsed())
            .filter(|t| !t.is_zero())
            .ok_or_else(|| error("DEADLINE", "local DC extraction deadline"))
    };
    let budget = board_prepare::allocation_budget(board_prepare::verification_limit(
        "P7_VERIFY_MEMORY_MIB",
        2048,
        256,
        8192,
    )?);
    let Request {
        geometry,
        mut drive,
        mut loads_ohm,
        observables_only,
    } = request;
    let np = geometry.ports.len();
    if !(1..=16).contains(&np) || !(2..=32).contains(&geometry.electrodes.len()) {
        return Err(error(
            "DC_PORTS",
            "local DC requires 1..16 physical ports and 2..32 finite electrodes",
        ));
    }
    let names: Vec<_> = geometry.electrodes.iter().map(|e| e.name.clone()).collect();
    let ports = geometry
        .ports
        .iter()
        .map(|pair| {
            let [a, b] = pair
                .each_ref()
                .map(|name| names.iter().position(|n| n == name));
            match (a, b) {
                (Some(a), Some(b)) if a != b => Ok([a, b]),
                _ => Err(error(
                    "DC_PORTS",
                    "port must name two distinct authored electrodes",
                )),
            }
        })
        .collect::<Result<Vec<_>>>()?;
    if drive.is_empty() {
        drive.resize(np, 0.);
    }
    if loads_ohm.is_empty() {
        loads_ohm.resize(np, None);
    }
    if drive.len() != np
        || loads_ohm.len() != np
        || drive.iter().any(|v| !v.is_finite())
        || loads_ohm
            .iter()
            .flatten()
            .any(|&r| !r.is_finite() || r <= 0. || !r.recip().is_finite())
    {
        return Err(error(
            "DC_DRIVE",
            "finite Norton current and optional positive finite resistive load per port required",
        ));
    }
    let mesh = board_mesh::build(board, &geometry, budget, remaining()?)?;
    remaining()?;
    let electrode_nodes = geometry
        .electrodes
        .iter()
        .map(|e| mesh.electrode_nodes(e))
        .collect::<Result<Vec<_>>>()?;
    let space =
        volume_current::assemble(&mesh.mesh, &mesh.conductivity_s_m, &electrode_nodes, budget)?;
    let contacts = electrode_nodes
        .iter()
        .map(|nodes| space.node_potential[&nodes[0]])
        .collect::<Vec<_>>();
    let recovery_output = if observables_only {
        0
    } else {
        let numeric = space
            .current_count()
            .checked_mul(96)
            .and_then(|n| n.checked_add(mesh.mesh.nodes_m.len().checked_mul(192)?))
            .ok_or_else(|| error("OUTPUT_BUDGET", "DC recovery output plan overflow"))?;
        mesh.mesh
            .copper_tetrahedra
            .iter()
            .flat_map(|cell| &cell.source_ids)
            .try_fold(numeric, |n, id| {
                n.checked_add(96)?.checked_add(id.len().checked_mul(8)?)
            })
            .ok_or_else(|| error("OUTPUT_BUDGET", "DC source recovery output plan overflow"))?
    };
    if recovery_output > 64 * 1024 * 1024 {
        return Err(error(
            "OUTPUT_BUDGET",
            "DC recovery exceeds bounded output; request observables only",
        ));
    }
    let retained_report = mesh
        .source_mapping
        .iter()
        .try_fold(128 * 1024_usize, |n, (a, b, c, d)| {
            n.checked_add(512)?.checked_add(
                (a.len() + b.len() + c.as_ref().map_or(0, String::len) + d.len()).checked_mul(8)?,
            )
        })
        .and_then(|n| n.checked_add(recovery_output))
        .ok_or_else(|| error("OUTPUT_BUDGET", "DC report plan overflow"))?;
    let max_bytes = budget
        .max_bytes
        .checked_sub(retained_report)
        .ok_or_else(|| error("MEMORY_LIMIT", "DC report reserve exceeds budget"))?;
    let map = ElectrodeDc::new_with_deadline(
        &space,
        &contacts,
        AllocationBudget {
            max_bytes,
            ..budget
        },
        remaining()?,
    )?;
    remaining()?;
    let state = loaded(
        &map.conductance_s,
        &map.component_ids,
        &ports,
        &drive,
        &loads_ohm,
    )?;
    let (_, currents) = map.recover(&space, &state.volts)?;
    let losses = space.losses(&currents)?;
    let total_loss_w: f64 = losses.iter().sum();
    let port_volts = ports
        .iter()
        .map(|&[a, b]| {
            (state.components[a] == state.components[b]).then(|| state.volts[a] - state.volts[b])
        })
        .collect::<Vec<_>>();
    let floating = port_volts
        .iter()
        .enumerate()
        .filter_map(|(i, v)| v.is_none().then_some(i))
        .collect::<Vec<_>>();
    let input_power: f64 = ports
        .iter()
        .zip(&drive)
        .map(|(&[a, b], &i)| i * (state.volts[a] - state.volts[b]))
        .sum();
    let dissipated = total_loss_w + state.load_loss_w;
    if !input_power.is_finite()
        || !dissipated.is_finite()
        || (input_power - dissipated).abs()
            > 1e-8 * input_power.abs().max(dissipated).max(f64::MIN_POSITIVE)
    {
        return Err(error(
            "DC_POWER",
            "loaded DC source power and recovered Joule loss disagree",
        ));
    }
    remaining()?;
    let mut result = json!({
        "status":"complete","frequencyHz":0,"ports":geometry.ports,"driveA":drive,
        "loadsOhm":loads_ohm,"portRealV":port_volts,"floatingPortIndices":floating,
        "electrodeNames":names,"electrodeVoltagesV":state.volts,
        "electrodeGalvanicComponents":map.component_ids,"electrodeLoadedComponents":state.components,
        "dcConductanceS":map.conductance_s,"totalLossW":total_loss_w,"loadLossW":state.load_loss_w,"drivePowerW":input_power,
        "normalizedElectrodeResidual":state.normalized_residual,
        "roiMm":geometry.roi_mm,"domainPolygonMm":geometry.domain_polygon_mm,"allowInsulatedCopperCuts":geometry.allow_insulated_copper_cuts,
        "sourceMapping":mesh.source_mapping,
        "validity":{"status":"conditional","domain":{"frequencyHz":0},
            "reason":"Authored finite local copper domain and numerical mesh; DC endpoint only. Electrode potentials use independent numerical island gauges."}
    });
    if !observables_only {
        result["cellCurrentA"] = json!(currents);
        result["cellLossW"] = json!(losses);
        result["nodesM"] = json!(mesh.mesh.nodes_m);
        result["copperCells"] = json!(mesh
            .mesh
            .copper_tetrahedra
            .iter()
            .map(|c| c.nodes)
            .collect::<Vec<_>>());
        result["sourceIds"] = json!(mesh
            .mesh
            .copper_tetrahedra
            .iter()
            .map(|c| &c.source_ids)
            .collect::<Vec<_>>());
        result["originMm"] = json!(mesh.origin_mm);
    }
    remaining()?;
    Ok(result)
}
