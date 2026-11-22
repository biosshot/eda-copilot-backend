//! Ignored actual-board magnetic action benchmark. No C/field or AC response.
use super::*;
use crate::extraction::local::{
    board_mesh, board_prepare, mixed_conductor, volume_integral_magnetic,
};
use std::{env, fs, path::PathBuf};

struct DcAudit {
    copper: Vec<f64>,
    correction_rhs: Vec<f64>,
    control: serde_json::Value,
    score: f64,
}

/// Evaluate the original conservative Cu operator on compensated F64 voltages.
/// Keeping the low word avoids discarding refinements under a large common mode.
fn audit_dc(
    space: &crate::extraction::local::volume_current::Space,
    ports: &[[usize; 2]],
    voltage: &[Sum],
    injection: &[f64],
    drive: usize,
    reference_ohm: f64,
) -> Result<DcAudit> {
    let n = space.potential_count();
    let mut copper = Vec::with_capacity(space.current_count());
    let mut native_gradient = Vec::with_capacity(space.current_count());
    for cell in &space.cells {
        for axis in 0..3 {
            let g = (1..4)
                .fold(Sum::default(), |g, k| {
                    g.add(
                        voltage[cell.potentials[k]]
                            .sub(voltage[cell.potentials[0]])
                            .mul(cell.incidence[k][axis]),
                    )
                })
                .value();
            native_gradient.push(g);
            copper.push(g / cell.resistance_ohm);
        }
    }
    let mut reaction = space.divergence(&copper)?;
    let port_voltages: Vec<_> = ports
        .iter()
        .map(|&[p, m]| voltage[p].sub(voltage[m]).value())
        .collect();
    let load_currents: Vec<_> = port_voltages.iter().map(|v| v / reference_ohm).collect();
    let mut row_scale = vec![0.; n];
    for (i, cell) in space.cells.iter().enumerate() {
        for k in 0..4 {
            for axis in 0..3 {
                row_scale[cell.potentials[k]] +=
                    (cell.incidence[k][axis] * copper[3 * i + axis]).abs();
            }
        }
    }
    for (&[p, m], &q) in ports.iter().zip(&load_currents) {
        reaction[p] = Sum::from(reaction[p]).add(Sum::from(q)).value();
        reaction[m] = Sum::from(reaction[m]).sub(Sum::from(q)).value();
        row_scale[p] += q.abs();
        row_scale[m] += q.abs();
    }
    let correction_rhs: Vec<_> = injection
        .iter()
        .zip(&reaction)
        .map(|(&b, &a)| Sum::from(b).sub(Sum::from(a)).value())
        .collect();
    let residual = correction_rhs.iter().map(|x| x.abs()).fold(0., f64::max);
    let scale = row_scale
        .iter()
        .zip(injection)
        .map(|(&a, &b)| a + b.abs())
        .fold(0., f64::max);
    let kcl_relative = residual / scale.max(f64::MIN_POSITIVE);
    let mut kvl_error = 0_f64;
    let mut kvl_scale = 0_f64;
    for (i, (&q, &g)) in copper.iter().zip(&native_gradient).enumerate() {
        let rq = space.cells[i / 3].resistance_ohm * q;
        kvl_error = kvl_error.max((rq - g).abs());
        kvl_scale = kvl_scale.max(rq.abs() + g.abs());
    }
    let dc_kvl_relative = kvl_error / kvl_scale.max(f64::MIN_POSITIVE);
    let copper_loss = space
        .losses(&copper)?
        .into_iter()
        .fold(Sum::default(), |s, x| s.add(Sum::from(x)))
        .value();
    let load_loss = load_currents
        .iter()
        .fold(Sum::default(), |s, &q| {
            s.add(Sum::from(q).mul(q).mul(reference_ohm))
        })
        .value();
    let source_power = port_voltages[drive];
    let power_relative = Sum::from(source_power)
        .sub(Sum::from(copper_loss))
        .sub(Sum::from(load_loss))
        .value()
        .abs()
        / source_power
            .abs()
            .max((copper_loss + load_loss).abs())
            .max(f64::MIN_POSITIVE);
    let incident_power = reference_ohm / 4.;
    let outgoing = port_voltages
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            let s = 2. * v / reference_ohm - if i == drive { 1. } else { 0. };
            incident_power * s * s
        })
        .sum::<f64>();
    let wave_power_relative = Sum::from(incident_power)
        .sub(Sum::from(outgoing))
        .sub(Sum::from(copper_loss))
        .value()
        .abs()
        / incident_power;
    let metrics = [
        kcl_relative,
        dc_kvl_relative,
        power_relative,
        wave_power_relative,
    ];
    if metrics.iter().any(|x| !x.is_finite())
        || copper.iter().chain(&correction_rhs).any(|x| !x.is_finite())
    {
        return Err(error(
            "BENCH_DC_RESIDUAL",
            "nonfinite original DC source or residual",
        ));
    }
    let score = metrics.into_iter().fold(0., f64::max);
    let mut d0 = 0_f64;
    for (i, &q) in copper.iter().enumerate() {
        let (_, lower, _) = volume_integral_magnetic::hierarchical_magnetic::kvl_cell(
            space.cells[i / 3].resistance_ohm,
            q,
            &[],
            &[],
            0.,
            0.,
            0.,
        )?;
        d0 = d0.max(lower);
    }
    let active = copper
        .chunks_exact(3)
        .filter(|q| q.iter().any(|v| *v != 0.))
        .count();
    Ok(DcAudit {
        copper,
        correction_rhs,
        score,
        control: serde_json::json!({"drivePort":drive,"nortonCurrentA":1.,"referenceOhm":reference_ohm,"kclRelative":kcl_relative,"dcKvlRelative":dc_kvl_relative,"sourcePowerW":source_power,"copperLossW":copper_loss,"externalLoadLossW":load_loss,"powerRelative":power_relative,"matchedWavePowerRelative":wave_power_relative,"portVoltagesV":port_voltages,"activeOriginalCopperCells":active,"normalizationLowerV":d0}),
    })
}

fn bytes_add(a: usize, b: usize) -> Result<usize> {
    a.checked_add(b)
        .ok_or_else(|| error("MEMORY_LIMIT", "actual magnetic benchmark byte overflow"))
}

fn original_space(
    mesh: &board_mesh::BoardMesh,
    request: &board_mesh::Request,
    frequency: f64,
    budget: AllocationBudget,
    wall: Duration,
) -> Result<(
    crate::extraction::local::volume_current::Space,
    Vec<usize>,
    usize,
)> {
    let map = mixed_conductor::prepare(mesh, &request.electrodes, frequency, budget, wall)?;
    let originals = map.original_representatives();
    let electrodes = map.electrodes.iter().map(|&i| originals[i]).collect();
    // All remaining temporary mixed-map fields drop at this function boundary.
    Ok((map.full, electrodes, map.planned_bytes))
}

fn run_actual(path: &str, started: Instant, wall: Duration) -> Result<serde_json::Value> {
    let raw = fs::read_to_string(path).map_err(|e| error("BENCH_INPUT", &e.to_string()))?;
    let problem: serde_json::Value =
        serde_json::from_str(&raw).map_err(|e| error("BENCH_INPUT", &e.to_string()))?;
    let imported = crate::import_json(
        problem["boardJson"]
            .as_str()
            .ok_or_else(|| error("BENCH_INPUT", "original boardJson required"))?,
    );
    let board = imported
        .board
        .ok_or_else(|| error("BENCH_INPUT", "saved original board failed import"))?;
    let request: board_mesh::Request = serde_json::from_value(problem["geometry"].clone())
        .map_err(|e| error("BENCH_INPUT", &e.to_string()))?;
    let frequencies = problem["request"]["frequenciesHz"]
        .as_array()
        .ok_or_else(|| error("BENCH_INPUT", "original authored frequency required"))?;
    if frequencies.len() != 1
        || frequencies[0].as_f64() != Some(10_000.)
        || request.ports.len() != 2
        || problem["request"]["referenceOhm"]
            .as_array()
            .is_none_or(|references| {
                references.len() != 2 || references.iter().any(|r| r.as_f64() != Some(50.))
            })
    {
        return Err(error(
            "BENCH_INPUT",
            "benchmark requires the saved two-port authored singleton 10 kHz and 50 ohm references",
        ));
    }
    let frequency_hz = 10_000.;
    let omega = 2. * std::f64::consts::PI * frequency_hz;
    let reference_ohm = 50.;
    let input_bytes = bytes_add(
        raw.len()
            .checked_mul(8)
            .ok_or_else(|| error("MEMORY_LIMIT", "decoded benchmark input overflow"))?,
        256 * 1024,
    )?;
    let mut budget = board_prepare::allocation_budget(3072);
    budget.max_bytes = budget
        .max_bytes
        .checked_sub(input_bytes)
        .ok_or_else(|| error("MEMORY_LIMIT", "benchmark input exceeds total budget"))?;
    let geometry_started = Instant::now();
    eprintln!("P7_ACTUAL_MAGNETIC_STAGE geometry");
    let mesh = board_mesh::build(
        &board,
        &request,
        budget,
        wall.saturating_sub(started.elapsed()),
    )?;
    check_time(started, wall)?;
    let geometry_seconds = geometry_started.elapsed().as_secs_f64();
    let ownership_started = Instant::now();
    eprintln!("P7_ACTUAL_MAGNETIC_STAGE originalSpace geometrySeconds={geometry_seconds:.6}");
    let (space, electrode_dofs, map_peak) = original_space(
        &mesh,
        &request,
        frequency_hz,
        budget,
        wall.saturating_sub(started.elapsed()),
    )?;
    let names: BTreeMap<_, _> = request
        .electrodes
        .iter()
        .enumerate()
        .map(|(i, e)| (e.name.as_str(), i))
        .collect();
    if names.len() != request.electrodes.len() {
        return Err(error(
            "BENCH_INPUT",
            "distinct original electrode names required",
        ));
    }
    let mut ports = Vec::with_capacity(request.ports.len());
    for pair in &request.ports {
        let p = *names
            .get(pair[0].as_str())
            .ok_or_else(|| error("BENCH_INPUT", "original positive electrode missing"))?;
        let m = *names
            .get(pair[1].as_str())
            .ok_or_else(|| error("BENCH_INPUT", "original negative electrode missing"))?;
        if electrode_dofs[p] == electrode_dofs[m] {
            return Err(error("BENCH_INPUT", "external physical port collapsed"));
        }
        ports.push([electrode_dofs[p], electrode_dofs[m]]);
    }
    let ownership_seconds = ownership_started.elapsed().as_secs_f64();
    let n = space.potential_count();
    let ni = space.current_count();
    let dc_started = Instant::now();
    eprintln!("P7_ACTUAL_MAGNETIC_STAGE dcFactor potentials={n} currents={ni} ownershipSeconds={ownership_seconds:.6}");
    // The existing n*1024 / current-vector reservation covers the retained
    // compensated potential (two F64 words), true residual and correction RHS.
    let dc_plan = native_sizes(n, ni + ports.len(), 0, space.planned_bytes)?;
    budget.check(dc_plan)?;
    let mut cells = Vec::with_capacity(ni + ports.len());
    for cell in &space.cells {
        check_time(started, wall)?;
        for axis in 0..3 {
            cells.push(Cell::new(
                (0..4)
                    .map(|k| (cell.potentials[k], cell.incidence[k][axis]))
                    .collect(),
                cell.resistance_ohm,
            ));
        }
    }
    for &[p, m] in &ports {
        cells.push(Cell::new(vec![(p, 1.), (m, -1.)], reference_ohm));
    }
    // Finite loads connect circuit components, with one numerical gauge each.
    // No conductor is welded and every gauge reaction is checked below.
    let mut physical_component = vec![usize::MAX; n];
    let mut loaded_parent: Vec<_> = (0..space.components.len()).collect();
    for (component, nodes) in space.components.iter().enumerate() {
        for &node in nodes {
            physical_component[node] = component;
        }
    }
    for &[p, m] in &ports {
        let a = root(&loaded_parent, physical_component[p]);
        let b = root(&loaded_parent, physical_component[m]);
        loaded_parent[b] = a;
    }
    let mut anchors = BTreeMap::<usize, usize>::new();
    for (component, nodes) in space.components.iter().enumerate() {
        let key = root(&loaded_parent, component);
        anchors
            .entry(key)
            .and_modify(|node| *node = (*node).min(nodes[0]))
            .or_insert(nodes[0]);
    }
    let gauges: BTreeSet<_> = anchors.into_values().collect();
    let loaded_component_count = gauges.len();
    let (factor_peak, dc) = correction(&cells, n, &gauges, dc_plan, budget, started, wall, None)?;
    let factor_seconds = dc_started.elapsed().as_secs_f64();
    let mut columns = Vec::with_capacity(ports.len());
    let mut controls = Vec::with_capacity(ports.len());
    for (drive, &[positive, negative]) in ports.iter().enumerate() {
        check_time(started, wall)?;
        let mut injection = vec![0.; n];
        injection[positive] = 1.;
        injection[negative] = -1.;
        let mut voltage: Vec<_> = dc
            .solve(&cells, &injection)?
            .into_iter()
            .map(Sum::from)
            .collect();
        let mut history = Vec::with_capacity(9);
        let mut previous_score = f64::INFINITY;
        let mut stagnant = 0usize;
        let mut accepted = None;
        for iteration in 0..=8 {
            check_time(started, wall)?;
            let mut audit = audit_dc(&space, &ports, &voltage, &injection, drive, reference_ohm)?;
            let mut step = audit.control.clone();
            step["iteration"] = serde_json::json!(iteration);
            eprintln!("P7_ACTUAL_DC_REFINEMENT {step}");
            history.push(step);
            if audit.score <= 1e-8 {
                audit.control["dcRefinementIterations"] = serde_json::json!(iteration);
                audit.control["dcRefinementHistory"] = serde_json::json!(history);
                audit.control["voltageArithmetic"] =
                    serde_json::json!("existing compensated F64 hi/lo");
                audit.control["maximumVoltageTailV"] =
                    serde_json::json!(voltage.iter().map(|v| v.lo.abs()).fold(0., f64::max));
                accepted = Some(audit);
                break;
            }
            if iteration == 8 {
                break;
            }
            if audit.score >= previous_score * 0.999 {
                stagnant += 1;
            } else {
                stagnant = 0;
            }
            if stagnant >= 3 {
                break;
            }
            previous_score = audit.score;
            // Reuse the same admitted LLT and gauges; only the RHS changes.
            let correction = dc.solve(&cells, &audit.correction_rhs)?;
            for (i, (value, delta)) in voltage.iter_mut().zip(correction).enumerate() {
                *value = value.add(Sum::from(delta));
                if !value.hi.is_finite()
                    || !value.lo.is_finite()
                    || (gauges.contains(&i) && (value.hi != 0. || value.lo != 0.))
                {
                    return Err(error(
                        "BENCH_DC_RESIDUAL",
                        "nonfinite refinement or changed numerical gauge",
                    ));
                }
            }
        }
        let audit = accepted.ok_or_else(|| error("BENCH_DC_RESIDUAL", &format!("drive {drive} bounded same-factor refinement did not pass unchanged 1e-8 gates; history={}", serde_json::json!(history))))?;
        controls.push(audit.control);
        columns.push(audit.copper);
    }
    let source_seconds = dc_started.elapsed().as_secs_f64();
    drop(dc);
    drop(cells);
    drop(physical_component);
    drop(loaded_parent);
    drop(gauges);
    // Factor and construction scratch are gone. Charge input globally and the
    // retained original full space plus bounded current/action slabs here.
    let action_base = bytes_add(
        space.planned_bytes,
        columns
            .len()
            .checked_mul(ni)
            .and_then(|v| v.checked_mul(64))
            .ok_or_else(|| error("MEMORY_LIMIT", "benchmark action slab overflow"))?,
    )?;
    budget.check(action_base)?;
    let magnetic_started = Instant::now();
    eprintln!("P7_ACTUAL_MAGNETIC_STAGE magneticPreparation sourceSeconds={source_seconds:.6}");
    let magnetic = volume_integral_magnetic::Streamed::new(
        &mesh.mesh,
        &space,
        12,
        action_base,
        budget,
        started,
        wall,
    )?;
    let prepare_seconds = magnetic_started.elapsed().as_secs_f64();
    let goals = magnetic.resistive_goals(&columns, omega)?;
    if goals.len() != columns.len() {
        return Err(error(
            "BENCH_MAGNETIC",
            "one guarded goal per physical current required",
        ));
    }
    for (control, &goal) in controls.iter_mut().zip(&goals) {
        control["absoluteActionGoal"] = serde_json::json!(goal);
    }
    let action_started = Instant::now();
    eprintln!("P7_ACTUAL_MAGNETIC_STAGE adaptiveAction columns={} preparationSeconds={prepare_seconds:.6}", columns.len());
    // Native agent supplies the shared adaptive batch implementation.
    let (actions, bounds, certificate) = magnetic.actions_certified(&columns, &goals)?;
    if actions.len() != columns.len() || bounds.len() != columns.len() {
        return Err(error("BENCH_MAGNETIC", "all physical columns required"));
    }
    let mut checked = Vec::new();
    for (k, (action, bound)) in actions.iter().zip(&bounds).enumerate() {
        if action.len() != ni
            || bound.len() != ni
            || action.iter().any(|x| !x.is_finite())
            || bound.iter().any(|x| !x.is_finite() || *x < 0.)
        {
            return Err(error(
                "BENCH_MAGNETIC",
                "finite full original action/enclosure required",
            ));
        }
        let maximum = bound.iter().copied().fold(0., f64::max);
        if maximum > goals[k] {
            return Err(error(
                "BENCH_MAGNETIC",
                &format!(
                    "drive{k} uncertainty={maximum:e} exceeds action goal={:e}",
                    goals[k]
                ),
            ));
        }
        checked.push(serde_json::json!({"drivePort":k,"maximumAbsoluteActionBound":maximum,"absoluteActionGoal":goals[k],"allOriginalRowsChecked":ni}));
    }
    Ok(
        serde_json::json!({"status":"pass","scope":"original full-board matched-DC physical currents queried by the magnetic operator at the authored frequency; not an AC channel response","coverage":"unverified","frequencyHz":frequency_hz,"fieldSolves":0,"magneticBatchCalls":1,"physicalDriveColumns":columns.len(),"physicalGalvanicComponents":space.components.len(),"loadedCircuitComponents":loaded_component_count,"originalCopperTetrahedra":space.cells.len(),"originalPotentialCoordinates":n,"originalCurrentCoordinates":ni,"sourceMappings":mesh.source_mapping.len(),"barrelIds":mesh.barrel_ids,"originalPorts":request.ports,"physicalSignature":board_mesh::physical_signature(&request),"controls":controls,"actionChecks":checked,"certificate":certificate,"operator":magnetic.evidence(),"timingSeconds":{"geometry":geometry_seconds,"mixedOwnership":ownership_seconds,"dcFactor":factor_seconds,"dcSourceTotal":source_seconds,"magneticPrepare":prepare_seconds,"magneticAction":action_started.elapsed().as_secs_f64(),"total":started.elapsed().as_secs_f64()},"memoryPlanBytes":{"inputAndOutputReserve":input_bytes,"geometryPreparation":mesh.mesh.planned_peak_bytes,"mixedPreparation":map_peak,"dcPeak":factor_peak,"magneticPeak":bytes_add(action_base, magnetic.owned_bytes)?,"admittedGlobalBudget":bytes_add(budget.max_bytes,input_bytes)?}}),
    )
}

#[test]
#[ignore = "requires saved full-board source and an explicit bounded supervisor"]
fn saved_stm32_physical_matched_dc_currents_adaptive_action() {
    let path = env::var("P7_SAVED_PROBLEM").expect("explicit original saved problem");
    let output =
        PathBuf::from(env::var("P7_TEST_EVIDENCE_DIR").expect("explicit evidence directory"));
    let started = Instant::now();
    let wall = Duration::from_secs(1800);
    let report = run_actual(&path, started, wall).unwrap_or_else(|failure| serde_json::json!({"status":"refused","error":failure,"seconds":started.elapsed().as_secs_f64(),"scope":"actual original-board physical matched-DC magnetic action benchmark; no AC response","coverage":"unverified"}));
    fs::create_dir_all(&output).unwrap();
    fs::write(
        output.join("comparison.json"),
        serde_json::to_vec_pretty(&report).unwrap(),
    )
    .unwrap();
    println!("P7_ACTUAL_ADAPTIVE_MAGNETIC {report}");
    assert_eq!(report["status"], "pass", "{report}");
}
