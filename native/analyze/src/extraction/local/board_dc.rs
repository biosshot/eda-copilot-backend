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
    pub external_port_count: Option<usize>,
}

pub(super) struct Loaded {
    pub(super) volts: Vec<f64>,
    pub(super) components: Vec<usize>,
    pub(super) load_loss_w: f64,
    normalized_residual: f64,
}

// Each inter-island port is a directed edge. A spanning forest yields one
// balanced drive per cycle and a unit drive per same-island port.
fn balanced_drives(components: &[usize], ports: &[[usize; 2]]) -> Vec<Vec<f64>> {
    let mut labels = std::collections::BTreeMap::new();
    let index: Vec<_> = components.iter().map(|&label| {
        let next = labels.len();
        *labels.entry(label).or_insert(next)
    }).collect();
    let mut tree = vec![Vec::<(usize, usize, f64)>::new(); labels.len()];
    let mut basis = Vec::new();
    for (edge, &[a, b]) in ports.iter().enumerate() {
        let (a, b) = (index[a], index[b]);
        let mut parent = vec![None; tree.len()];
        let mut pending = vec![b];
        parent[b] = Some((b, edge, 0.));
        while let Some(node) = pending.pop() {
            for &(next, index, sign) in &tree[node] {
                if parent[next].is_none() {
                    parent[next] = Some((node, index, sign));
                    pending.push(next);
                }
            }
        }
        if parent[a].is_none() {
            tree[a].push((b, edge, 1.));
            tree[b].push((a, edge, -1.));
            continue;
        }
        let mut drive = vec![0.; ports.len()];
        drive[edge] = 1.;
        let mut node = a;
        while node != b {
            let (previous, index, sign) = parent[node].unwrap();
            drive[index] = sign;
            node = previous;
        }
        basis.push(drive);
    }
    basis
}

pub(super) fn loaded(
    conductance: &[Vec<f64>],
    galvanic: &[usize],
    ports: &[[usize; 2]],
    drive: &[f64],
    loads: &[Option<f64>],
) -> Result<Loaded> {
    Ok(loaded_with_modes(conductance, galvanic, ports, drive, loads, 0)?.0)
}

fn loaded_with_modes(
    conductance: &[Vec<f64>],
    galvanic: &[usize],
    ports: &[[usize; 2]],
    drive: &[f64],
    loads: &[Option<f64>],
    external_port_count: usize,
) -> Result<(Loaded, Value)> {
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
    let external = &ports[..external_port_count];
    let basis = balanced_drives(&components, external);
    let mut right_sides = vec![rhs];
    for mode in &basis {
        let mut column = vec![0.; galvanic.len()];
        for (&[a, b], &current) in external.iter().zip(mode) {
            column[a] += current;
            column[b] -= current;
        }
        right_sides.push(column);
    }
    let mut voltages = vec![vec![0.; galvanic.len()]; right_sides.len()];
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
        let x = factor.solve(Mat::from_fn(free.len(), right_sides.len(), |i, j| {
            right_sides[j][free[i]]
        }));
        for (j, column) in voltages.iter_mut().enumerate() {
            for (i, &index) in free.iter().enumerate() {
                column[index] = x[(i, j)];
            }
        }
    }
    let mut residuals = Vec::with_capacity(right_sides.len());
    for (rhs, volts) in right_sides.iter().zip(&voltages) {
        let mut normalized_residual = 0_f64;
        for (i, row) in matrix.iter().enumerate() {
            let mut residual = super::accurate::Sum::from(rhs[i]);
            let mut scale = rhs[i].abs();
            for (&g, &v) in row.iter().zip(volts) {
                residual = residual.sub(super::accurate::Sum::from(g).mul(v));
                scale += (g * v).abs();
            }
            normalized_residual =
                normalized_residual.max(residual.value().abs() / scale.max(f64::MIN_POSITIVE));
        }
        if volts.iter().any(|v| !v.is_finite()) || normalized_residual > 1e-8 {
            return Err(error("DC_RESIDUAL", "loaded conductance fails full electrode KCL"));
        }
        residuals.push(normalized_residual);
    }
    let modal_impedance: Vec<Vec<f64>> = basis.iter().map(|mode| {
        voltages[1..].iter().map(|volts| {
            external.iter().zip(mode).fold(super::accurate::Sum::default(), |sum, (&[a, b], &weight)| {
                sum.add(super::accurate::Sum::from(volts[a]).sub(super::accurate::Sum::from(volts[b])).mul(weight))
            }).value()
        }).collect()
    }).collect();
    if modal_impedance.iter().flatten().any(|x| !x.is_finite()) {
        return Err(error("DC_RANGE", "balanced DC modal impedance overflow"));
    }
    let port_voltages: Vec<Vec<Option<f64>>> = voltages[1..].iter().map(|volts| {
        external.iter().map(|&[a, b]| {
            (components[a] == components[b]).then(|| volts[a] - volts[b])
        }).collect()
    }).collect();
    let mut power_by_drive = Vec::with_capacity(basis.len());
    for (index, volts) in voltages[1..].iter().enumerate() {
        let mut copper = super::accurate::Sum::default();
        // Use the original copper Schur conductance, before package stamps.
        // Differences on each island remove its arbitrary potential origin.
        for (i, row) in conductance.iter().enumerate() {
            for (j, &g) in row.iter().enumerate().skip(i + 1) {
                let voltage = volts[i] - volts[j];
                copper = copper.sub(super::accurate::Sum::from(g).mul(voltage).mul(voltage));
            }
        }
        let copper_w = copper.value();
        let (mut external_w, mut package_w) = (0., 0.);
        for (port, (&[a, b], load)) in ports.iter().zip(loads).enumerate() {
            if let Some(r) = load {
                let loss = ((volts[a] - volts[b]) / r.sqrt()).powi(2);
                if port < external_port_count {
                    external_w += loss;
                } else {
                    package_w += loss;
                }
            }
        }
        let total_w = copper_w + external_w + package_w;
        let input_w = modal_impedance[index][index];
        if [copper_w, external_w, package_w, total_w, input_w].iter().any(|v| !v.is_finite() || *v < 0.)
            || (input_w - total_w).abs()
                > 1e-8 * input_w.abs().max(total_w).max(f64::MIN_POSITIVE)
        {
            return Err(error("DC_POWER", "balanced modal source power and Schur losses disagree"));
        }
        power_by_drive.push(json!({
            "terminalInputW": input_w, "copperSchurLossW": copper_w,
            "externalLoadLossW": external_w, "packageLossW": package_w, "totalLossW": total_w,
        }));
    }
    let response = json!({
        "basisDrivesA": basis,
        "powerByDrive": power_by_drive,
        "powerScope": "Terminal copper Schur, external loads and private package losses; no modal cell recovery",
        "modalImpedanceOhm": modal_impedance,
        "portVoltagesVByDrive": port_voltages,
        "normalizedElectrodeResidualByDrive": residuals[1..],
        "independentPortDrives": basis.len() == external.len(),
    });
    let volts = voltages.swap_remove(0);
    let normalized_residual = residuals[0];
    let load_loss_w = ports
        .iter()
        .zip(loads)
        .filter_map(|(&[a, b], load)| load.map(|r| ((volts[a] - volts[b]) / r.sqrt()).powi(2)))
        .sum();
    Ok((Loaded {
        volts,
        components,
        load_loss_w,
        normalized_residual,
    }, response))
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
        external_port_count,
    } = request;
    let np = geometry.ports.len();
    if !(1..=16).contains(&np) || !(2..=32).contains(&geometry.electrodes.len()) {
        return Err(error(
            "DC_PORTS",
            "local DC requires 1..16 physical ports and 2..32 finite electrodes",
        ));
    }
    if external_port_count.is_some_and(|count| count == 0 || count > np) {
        return Err(error("DC_PORTS", "invalid external DC port count"));
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
    let (state, balanced_response) = loaded_with_modes(
        &map.conductance_s,
        &map.component_ids,
        &ports,
        &drive,
        &loads_ohm,
        external_port_count.unwrap_or(0),
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
    if external_port_count.is_some() {
        result["balancedPortResponse"] = balanced_response;
    }
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

#[cfg(test)]
mod balanced_port_tests {
    use super::*;

    #[test]
    fn balanced_port_modes_preserve_floating_voltages_and_package_loads() {
        let conductance = vec![
            vec![2., -2., 0., 0.], vec![-2., 2., 0., 0.],
            vec![0., 0., 4., -4.], vec![0., 0., -4., 4.],
        ];
        let ports = [[0, 2], [1, 3], [0, 1], [0, 2]];
        let run = |external, package| loaded_with_modes(
            &conductance, &[42, 42, 73, 73], &ports, &[0.; 4],
            &[external, None, None, package], 3,
        ).unwrap().1;
        let open = run(None, None);
        assert_eq!(open["basisDrivesA"], json!([[-1., 1., 0.], [0., 0., 1.]]));
        assert_eq!(open["portVoltagesVByDrive"][0][0], Value::Null);
        assert_eq!(open["independentPortDrives"], false);
        assert!((open["modalImpedanceOhm"][0][0].as_f64().unwrap() - 0.75).abs() < 1e-12);
        let modal = |i: usize, j: usize| open["modalImpedanceOhm"][i][j].as_f64().unwrap();
        assert!((modal(0, 1) + 0.5).abs() < 1e-12);
        assert!((modal(0, 1) - modal(1, 0)).abs() < 1e-12);
        assert!((open["powerByDrive"][0]["copperSchurLossW"].as_f64().unwrap() - 0.75).abs() < 1e-12);
        let permuted = loaded_with_modes(
            &conductance, &[73, 73, 42, 42], &[[1, 3], [0, 2], [1, 0], [1, 3]], &[0.; 4], &[None; 4], 3,
        ).unwrap().1;
        for i in 0..2 {
            for j in 0..2 {
                let value = permuted["modalImpedanceOhm"][i][j].as_f64().unwrap();
                assert!((value - modal(i, j)).abs() < 1e-12);
            }
        }
        let loaded = run(None, Some(2.));
        assert_eq!(loaded["independentPortDrives"], true);
        assert!((loaded["modalImpedanceOhm"][0][0].as_f64().unwrap() - 2.).abs() < 1e-12);
        assert!((loaded["powerByDrive"][0]["packageLossW"].as_f64().unwrap() - 2.).abs() < 1e-12);
        let terminated = run(Some(4.), Some(2.));
        let power = &terminated["powerByDrive"][0];
        let check_loss = |key: &str, expected: f64| {
            assert!((power[key].as_f64().unwrap() - expected).abs() < 1e-12);
        };
        check_loss("externalLoadLossW", 4. / 9.);
        check_loss("packageLossW", 8. / 9.);
        check_loss("totalLossW", 4. / 3.);
        assert!(balanced_drives(&[0, 1], &[[0, 1]]).is_empty());
    }
}
