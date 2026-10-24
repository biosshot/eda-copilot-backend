//! Finite-front index-one descriptor transient; all source/load assembly precedes
//! initialization and charge accounting. Output cadence does not choose the step.
use super::*;
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Driver {
    pub port: usize,
    pub resistance_ohm: f64,
    pub waveform: Waveform,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Receiver {
    pub port: usize,
    pub resistance_ohm: Option<f64>,
    pub capacitance_f: f64,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Request {
    pub resource: Option<frequency::Resource>,
    pub topology: Option<composition::Topology>,
    pub drivers: Vec<Driver>,
    pub receivers: Vec<Receiver>,
    pub time: Time,
    pub initial: Option<Value>,
    pub numerics: Option<Numerics>,
    pub budget: Option<Budget>,
}
fn deadline(start: Instant, budget: Option<&Budget>) -> Result<()> {
    super::super::deadline(start, budget, "Descriptor time")
}
pub(crate) fn fastest_front(waveform: &Waveform) -> f64 {
    match waveform {
        Waveform::Step { from, to, rise, .. } => {
            if from == to {
                f64::INFINITY
            } else {
                *rise
            }
        }
        Waveform::Sine { period, .. } => period / 2.,
        Waveform::Pulse {
            low,
            high,
            rise,
            fall,
            ..
        } => {
            if low == high {
                f64::INFINITY
            } else {
                rise.min(*fall)
            }
        }
        Waveform::Pwl { points, .. } => points
            .windows(2)
            .filter(|v| v[0][1] != v[1][1])
            .map(|v| v[1][0] - v[0][0])
            .fold(f64::INFINITY, f64::min),
    }
}
fn source(system: &System, drivers: &[Driver], ports: usize, t: f64) -> Vec<f64> {
    let ip = terminal_source(drivers, &(0..ports).collect::<Vec<_>>(), ports, t);
    system.port_rhs(&ip)
}
pub(crate) fn terminal_source(
    drivers: &[Driver],
    nodes: &[usize],
    size: usize,
    t: f64,
) -> Vec<f64> {
    let mut ip = vec![0.; size];
    for d in drivers {
        ip[nodes[d.port]] += d.waveform.sample(t) / d.resistance_ohm;
    }
    ip
}
/// The same finite source/load contract and power convention serves descriptor
/// and delay-containing graphs. Port voltages are always positive to return.
pub(crate) fn validate_bindings(
    drivers: &[Driver],
    receivers: &[Receiver],
    ports: usize,
    prefix: &str,
) -> Result<()> {
    if drivers.is_empty()
        || drivers.len() > 16
        || receivers.len() > 32
        || drivers
            .iter()
            .any(|d| d.port >= ports || !d.resistance_ohm.is_finite() || d.resistance_ohm <= 0.)
        || receivers.iter().any(|r| {
            r.port >= ports
                || !r.capacitance_f.is_finite()
                || r.capacitance_f < 0.
                || r.resistance_ohm.is_some_and(|v| !v.is_finite() || v <= 0.)
        })
    {
        return Err(fail(
            &format!("{prefix}_BINDING"),
            "Finite Thevenin drivers and passive RC receivers required",
        ));
    }
    for d in drivers {
        d.waveform.validate()?;
        if d.waveform.discontinuous() {
            return Err(fail(
                &format!("{prefix}_FINITE_FRONT"),
                "Continuous finite fronts required; impulses/jump projection unsupported",
            ));
        }
    }
    Ok(())
}
pub(crate) fn terminal_admittances(
    drivers: &[Driver],
    receivers: &[Receiver],
) -> Vec<(usize, f64, f64)> {
    drivers
        .iter()
        .map(|d| (d.port, 1. / d.resistance_ohm, 0.))
        .chain(receivers.iter().map(|r| {
            (
                r.port,
                r.resistance_ohm.map(|v| 1. / v).unwrap_or(0.),
                r.capacitance_f,
            )
        }))
        .collect()
}
pub(crate) fn terminal_power(
    drivers: &[Driver],
    receivers: &[Receiver],
    voltage: &[f64],
    at: f64,
) -> (f64, f64) {
    let (mut work, mut loss) = (0., 0.);
    for d in drivers {
        let source = d.waveform.sample(at);
        let current = (source - voltage[d.port]) / d.resistance_ohm;
        work += source * current;
        loss += d.resistance_ohm * current * current;
    }
    for r in receivers {
        if let Some(resistance) = r.resistance_ohm {
            loss += voltage[r.port] * voltage[r.port] / resistance;
        }
    }
    (work, loss)
}
impl System {
    /// Preserve every stored charge/flux coordinate while satisfying algebraic
    /// equations. E's eigenspaces define the split; a fictitious ground does not.
    fn consistent(
        &mut self,
        initial: &[f64],
        rhs: &[f64],
        stats: &mut Counters,
    ) -> Result<Vec<f64>> {
        let n = self.linear.n;
        let scale = self.linear.e.values().map(|v| v.abs()).fold(0., f64::max);
        let eigen = Mat::from_fn(n, n, |i, j| {
            if scale > 0. {
                self.linear.e.get(&(i, j)).copied().unwrap_or(0.) / scale
            } else {
                0.
            }
        })
        .self_adjoint_eigen(Side::Lower)
        .map_err(|e| fail("COUPLED_INITIAL", format!("{e:?}")))?;
        let mut matrix = Matrix::new();
        let mut b = vec![0.; n];
        for row in 0..n {
            let value = eigen.S()[row];
            if value < -1e-12 {
                return Err(fail("COUPLED_INITIAL", "Indefinite storage matrix"));
            }
            // Near-zero unresolved storage is rejected instead of erased.
            if value > 0. && value < 1e-14 {
                return Err(fail("COUPLED_INITIAL", "Unresolved storage rank"));
            }
            if value > 0. {
                for j in 0..n {
                    let q = eigen.U()[(j, row)];
                    if q != 0. {
                        matrix.insert((row, j), q);
                    }
                    b[row] += q * initial[j];
                }
            } else {
                for (&(i, j), g) in &self.linear.g {
                    *matrix.entry((row, j)).or_default() += eigen.U()[(i, row)] * g;
                }
                b[row] = (0..n).map(|i| eigen.U()[(i, row)] * rhs[i]).sum();
            }
        }
        let gauges = self.constraints(false, initial)?;
        let k = gauges.len();
        b.resize(n + k, 0.);
        for (g, (mode, constraint, _)) in gauges.iter().enumerate() {
            for row in 0..n {
                if eigen.S()[row] <= 0. {
                    let coefficient = (0..self.nv)
                        .map(|i| eigen.U()[(i, row)] * mode[i])
                        .sum::<f64>();
                    if coefficient != 0. {
                        matrix.insert((row, n + g), coefficient);
                    }
                }
                if constraint[row] != 0. {
                    matrix.insert((n + g, row), constraint[row]);
                }
            }
        }
        let (x, res, kcl) = self.linear.solve(matrix, b, stats)?;
        self.residual = self.residual.max(res);
        self.kcl = self.kcl.max(kcl);
        let x = x[..n].to_vec();
        let difference: Vec<_> = x.iter().zip(initial).map(|(a, b)| a - b).collect();
        let held = multiply(&self.linear.e, &difference, n);
        let stored = multiply(&self.linear.e, initial, n);
        if held
            .iter()
            .zip(stored)
            .any(|(a, b)| a.abs() > 1e-20 + 1e-9 * b.abs())
        {
            return Err(fail(
                "COUPLED_INITIAL",
                "Algebraic initialization changed charge/flux",
            ));
        }
        Ok(x)
    }
    /// BE increment form prevents cancellation of E*x/h on a voltage rail.
    fn advance(
        &mut self,
        old: &[f64],
        rhs: Vec<f64>,
        h: f64,
        stats: &mut Counters,
    ) -> Result<Vec<f64>> {
        let n = self.linear.n;
        let gx = multiply(&self.linear.g, old, n);
        let mut b: Vec<_> = rhs.iter().zip(gx).map(|(a, b)| a - b).collect();
        let mut matrix = self.linear.matrix(1. / h, None);
        // Only exact zero-energy gauges are constrained during evolution.
        let gauges = self.constraints(false, old)?;
        b.resize(n + gauges.len(), 0.);
        for (g, (mode, row, _)) in gauges.iter().enumerate() {
            for i in 0..n {
                if i < self.nv && mode[i] != 0. {
                    matrix.insert((i, n + g), mode[i]);
                }
                if row[i] != 0. {
                    matrix.insert((n + g, i), row[i]);
                }
            }
        }
        let (delta, res, kcl) = self.linear.solve(matrix, b, stats)?;
        self.residual = self.residual.max(res);
        self.kcl = self.kcl.max(kcl);
        let next: Vec<_> = old.iter().zip(&delta).map(|(a, b)| a + b).collect();
        if next.iter().any(|v| !v.is_finite()) {
            return Err(fail("COUPLED_NONFINITE", "Time increment overflow"));
        }
        // Recheck original physical increment equations, without gauge currents.
        let ed = multiply(&self.linear.e, &delta, n);
        let gn = multiply(&self.linear.g, &next, n);
        if ed.iter().zip(gn).zip(rhs).any(|((e, g), b)| {
            (e / h + g - b).abs() > 1e-14 + 1e-8 * (e.abs() / h + g.abs() + b.abs())
        }) {
            return Err(fail(
                "COUPLED_KCL",
                "Time step failed original descriptor equations",
            ));
        }
        Ok(next)
    }
}
pub fn solve(request: Request) -> Result<Value> {
    let start = Instant::now();
    deadline(start, request.budget.as_ref())?;
    if request
        .resource
        .as_ref()
        .is_some_and(|r| r.data["format"] == "delayed-scattering")
        || request.topology.as_ref().is_some_and(|t| {
            t.instances
                .iter()
                .any(|i| i.model.data["format"] == "delayed-scattering")
        })
    {
        return delayed::time(request, start);
    }
    let built = match (&request.resource, &request.topology) {
        (Some(resource), None) => composition::resource(resource, request.budget.as_ref(), start)?,
        (None, Some(topology)) => composition::build(topology, request.budget.as_ref(), start)?,
        _ => {
            return Err(fail(
                "COUPLED_INPUT",
                "Choose exactly one source Resource or explicit topology",
            ))
        }
    };
    let composition::Built {
        model,
        fit,
        conditional,
        ports: port_names,
        band,
        source_memory,
    } = built;
    let ports = port_names.len();
    let identity =
        serde_json::to_string(&(&model, &port_names, &request.drivers, &request.receivers))
            .map_err(|e| fail("COUPLED_INPUT", e))?;
    validate_bindings(&request.drivers, &request.receivers, ports, "COUPLED")?;
    let fit_max = if let Some(report) = &fit {
        Some(
            report["frequenciesHz"]
                .as_array()
                .and_then(|f| f.last())
                .and_then(Value::as_f64)
                .ok_or_else(|| fail("TEMPORAL_BAND", "Missing fitted source band"))?,
        )
    } else {
        None
    };
    if let Some(maximum) = fit_max
        .into_iter()
        .chain(band.map(|v| v[1]))
        .reduce(f64::min)
    {
        if request
            .drivers
            .iter()
            .any(|d| fastest_front(&d.waveform) < 10. / maximum)
        {
            return Err(fail(
                "TEMPORAL_SOURCE_BAND",
                "Fastest declared front must be at least 10/fMax; closure stays conditional",
            ));
        }
    }
    let time = &request.time;
    if !time.stop_s.is_finite()
        || time.stop_s <= 0.
        || !time.sample_step_s.is_finite()
        || time.sample_step_s <= 0.
        || time.stop_s / time.sample_step_s > 100000.
    {
        return Err(fail(
            "COUPLED_TIME",
            "Invalid or oversized output time axis",
        ));
    }
    let matrix_budget = composition::remaining_budget(request.budget.as_ref(), source_memory)?;
    let mut system = System::compile(&model, ports, Some(&matrix_budget))?;
    deadline(start, request.budget.as_ref())?;
    for (port, conductance, capacitance) in
        terminal_admittances(&request.drivers, &request.receivers)
    {
        system.add_port_admittance(port, conductance, capacitance)?;
    }
    let n = system.linear.n;
    let mut stats = Counters::default();
    let (mut t, mut x) = match request.initial.as_ref() {
        None | Some(Value::String(_)) => {
            if request.initial.as_ref().is_some_and(|v| v != "steady") {
                return Err(fail("COUPLED_INITIAL", "Unknown initial condition"));
            }
            let rhs = source(&system, &request.drivers, ports, 0.);
            (0., system.solve(0., rhs, &vec![0.; n], &mut stats)?)
        }
        Some(state) => {
            if state["identity"] != identity {
                return Err(fail(
                    "COUPLED_INITIAL",
                    "Continuation identity differs from fully assembled model",
                ));
            }
            let at = state["timeS"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.)
                .ok_or_else(|| fail("COUPLED_INITIAL", "Missing physical time"))?;
            let x: Vec<f64> = serde_json::from_value(state["x"].clone())
                .map_err(|e| fail("COUPLED_INITIAL", e))?;
            if x.len() != n || x.iter().any(|v| !v.is_finite()) {
                return Err(fail("COUPLED_INITIAL", "Invalid physical state"));
            }
            let rhs = source(&system, &request.drivers, ports, at);
            (at, system.consistent(&x, &rhs, &mut stats)?)
        }
    };
    if t >= time.stop_s {
        return Err(fail(
            "COUPLED_TIME",
            "Stop must follow initial physical time",
        ));
    }
    let initial_time = t;
    deadline(start, request.budget.as_ref())?;
    let initial_energy = system.energy(&x);
    let samples = ((time.stop_s - t) / time.sample_step_s).ceil() as usize + 1;
    let output_plan = samples
        .saturating_mul(ports + request.drivers.len() + 4)
        .saturating_mul(64);
    if samples > 100001
        || (system.memory_plan + output_plan + source_memory) as f64
            > request
                .budget
                .as_ref()
                .and_then(|b| b.max_memory_mi_b)
                .unwrap_or(256.)
                * 1048576.
    {
        return Err(fail(
            "COUPLED_BUDGET",
            "Time output exceeds admitted memory",
        ));
    }
    let rel = request
        .numerics
        .as_ref()
        .and_then(|v| v.relative_tolerance)
        .unwrap_or(1e-5);
    let abs = request
        .numerics
        .as_ref()
        .and_then(|v| v.absolute_tolerance)
        .unwrap_or(1e-8);
    let limit = request
        .numerics
        .as_ref()
        .and_then(|v| v.max_iterations)
        .unwrap_or(100000);
    if !rel.is_finite()
        || rel <= 0.
        || rel > 1e-3
        || !abs.is_finite()
        || abs <= 0.
        || abs > 1e-5
        || limit == 0
        || limit > 1000000
    {
        return Err(fail("COUPLED_NUMERICS", "Invalid accuracy/step controls"));
    }
    let mut axis = vec![t];
    let mut outputs = vec![system.port_voltages(&x, ports)];
    let mut currents = vec![request
        .drivers
        .iter()
        .map(|d| (d.waveform.sample(t) - outputs[0][d.port]) / d.resistance_ohm)
        .collect::<Vec<_>>()];
    let mut energies = vec![initial_energy];
    let mut next_sample = 1;
    // Start from declared fastest fitted pole, and reset at every source corner.
    // For generic descriptors step doubling plus independent refinement is still
    // required by callers: a successful local estimate is not global convergence.
    let fastest = fit
        .as_ref()
        .and_then(|f| f["coefficients"].as_array())
        .into_iter()
        .flatten()
        .filter_map(|m| m["polesPerS"].as_array())
        .flatten()
        .filter_map(Value::as_f64)
        .fold(0., f64::max);
    let seed = ((time.stop_s - t) / 1000.).min(if fastest > 0. {
        0.1 / fastest
    } else {
        f64::INFINITY
    });
    let max_h = request
        .drivers
        .iter()
        .map(|d| d.waveform.max_step())
        .fold((time.stop_s - t) / 100., f64::min);
    let mut h = seed;
    let (mut work, mut loss, mut numerical, mut maxerr) = (0., 0., 0., 0_f64);
    while t < time.stop_s {
        deadline(start, request.budget.as_ref())?;
        if stats.accepted_steps + stats.rejected_steps >= limit {
            return Err(fail(
                "NETWORK_STEP_BUDGET",
                "Descriptor internal step limit; incomplete result",
            ));
        }
        let event = request
            .drivers
            .iter()
            .map(|d| d.waveform.next(t))
            .fold(time.stop_s, f64::min);
        h = h.min(max_h).min(event - t).min(time.stop_s - t);
        if h <= 32. * f64::EPSILON * t.abs().max(1e-30) {
            return Err(fail(
                "NETWORK_STEP_UNDERFLOW",
                "Cannot resolve descriptor dynamics/front",
            ));
        }
        let full = system.advance(
            &x,
            source(&system, &request.drivers, ports, t + h),
            h,
            &mut stats,
        )?;
        let half = system.advance(
            &x,
            source(&system, &request.drivers, ports, t + h / 2.),
            h / 2.,
            &mut stats,
        )?;
        let fine = system.advance(
            &half,
            source(&system, &request.drivers, ports, t + h),
            h / 2.,
            &mut stats,
        )?;
        let err = full
            .iter()
            .zip(&fine)
            .zip(&x)
            .map(|((a, b), c)| (a - b).abs() / (abs + rel * b.abs().max(c.abs())))
            .fold(0., f64::max);
        if err > 1. {
            stats.rejected_steps += 1;
            h *= (0.9 / err.sqrt()).clamp(0.1, 0.5);
            continue;
        }
        maxerr = maxerr.max(err);
        stats.accepted_steps += 1;
        let mut previous = &x;
        for (y, at) in [(&half, t + h / 2.), (&fine, t + h)] {
            let voltage = system.port_voltages(y, ports);
            let (p, external_loss) =
                terminal_power(&request.drivers, &request.receivers, &voltage, at);
            let dissipated = system.joule_loss(y) + external_loss;
            work += p * h / 2.;
            loss += dissipated * h / 2.;
            let delta: Vec<_> = y.iter().zip(previous).map(|(a, b)| a - b).collect();
            numerical += system.energy(&delta);
            previous = y;
        }
        while next_sample < samples {
            let out = (initial_time + next_sample as f64 * time.sample_step_s).min(time.stop_s);
            if out > t + h {
                break;
            }
            let (a, b, fraction) = if out <= t + h / 2. {
                (&x, &half, (out - t) / (h / 2.))
            } else {
                (&half, &fine, (out - t - h / 2.) / (h / 2.))
            };
            let state: Vec<_> = a
                .iter()
                .zip(b)
                .map(|(a, b)| a + fraction * (b - a))
                .collect();
            let voltage = system.port_voltages(&state, ports);
            currents.push(
                request
                    .drivers
                    .iter()
                    .map(|d| (d.waveform.sample(out) - voltage[d.port]) / d.resistance_ohm)
                    .collect(),
            );
            energies.push(system.energy(&state));
            axis.push(out);
            outputs.push(voltage);
            next_sample += 1;
        }
        t += h;
        x = fine;
        h *= if err == 0. {
            2.
        } else {
            (0.9 / err.sqrt()).clamp(0.5, 2.)
        };
        if (t - event).abs() <= 16. * f64::EPSILON * event.abs() {
            h = h.min(seed);
        }
    }
    let energy_change = system.energy(&x) - initial_energy;
    deadline(start, request.budget.as_ref())?;
    let balance = work - loss - energy_change - numerical;
    if balance.abs()
        > 1e-14 + 1e-7 * (work.abs() + loss.abs() + energy_change.abs() + numerical.abs())
    {
        return Err(fail(
            "COUPLED_ENERGY",
            "BE discrete work/loss/storage/numerical-dissipation balance failed",
        ));
    }
    Ok(
        json!({"status":"complete","diagnostics":[],"axis":axis,"portVoltages":outputs,"driverCurrentsA":currents,"energyJ":energies,"fit":fit,"conditional":conditional,"counters":stats,"quality":{"relativeResidual":system.residual,"kclAbsA":system.kcl,"maximumLocalErrorRatio":maxerr,"workJ":work,"dissipatedJ":loss,"storedEnergyChangeJ":energy_change,"beNumericalDissipationJ":numerical,"energyBalanceJ":balance},"finalState":{"kind":"coupled-state","identity":identity,"timeS":t,"x":x},"elapsedMs":start.elapsed().as_secs_f64()*1000.,"memoryPlanBytes":system.memory_plan+output_plan+source_memory,"assumptions":["Full assembled descriptor; charge evolves with excitation; no per-island neutrality imposed","Continuous finite fronts, finite Thevenin drivers and passive RC receiver loads","Finite-band front screening is conditional and does not prove absence of source spectrum outside the authored band","Backward Euler step doubling; local error control is not independent global time convergence","Output is interpolation over accepted half steps; sampled extrema are not continuous extrema"]}),
    )
}
pub fn run(request: Request) -> Value {
    match solve(request) {
        Ok(value) => value,
        Err(error) => {
            json!({"status":"rejected","diagnostics":[{"code":error.split(':').next().unwrap_or("COUPLED_ERROR"),"severity":"error","message":error}]})
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn balanced_currents_evolve_island_charges_and_preserve_nonzero_total_charge() {
        let c1 = 2.225300110895741e-13;
        let c2 = 4.450600221791482e-13;
        let model:Model=serde_json::from_value(json!({"format":"coupled-rlc","referenceTopology":"infinity","potentialNodes":["inner","middle","outer"],"currentDofs":[],"incidence":[],"portIncidence":[[0,0,1.],[2,0,-1.],[1,1,1.],[2,1,-1.]],"resistanceOhm":[],"inductanceH":[],"capacitanceF":[[0,0,c1],[0,1,-c1],[1,0,-c1],[1,1,c1+c2],[1,2,-c2],[2,1,-c2],[2,2,2.*c2]]})).unwrap();
        let mut system = System::compile(&model, 2, None).unwrap();
        let charge = 1e-12;
        let old = vec![charge / c2; 3];
        let rhs = system.port_rhs(&[0.1, 0.2]);
        assert!(system
            .solve(0., rhs.clone(), &old, &mut Counters::default())
            .unwrap_err()
            .starts_with("COUPLED_DC_INCOMPATIBLE"));
        let dt = 1e-12;
        let x = system
            .advance(&old, rhs, dt, &mut Counters::default())
            .unwrap();
        let expected = [
            old[0] + 0.3 * dt / c2 + 0.1 * dt / c1,
            old[0] + 0.3 * dt / c2,
            old[0],
        ];
        for (actual, expected) in x.iter().zip(expected) {
            assert!((actual - expected).abs() < 1e-12);
        }
        // NᵀB is nonzero for individual isolated conductors; these charges must
        // accumulate, while the balanced system's TOTAL nonzero charge is held.
        let q = multiply(&system.linear.e, &x, 3);
        assert!((q[0] - 0.1 * dt).abs() < 1e-25);
        assert!((q[1] - 0.2 * dt).abs() < 1e-25);
        assert!((q.iter().sum::<f64>() - charge).abs() < 1e-25);
        assert!((x[2] - old[2]).abs() < 1e-12);
    }
}
