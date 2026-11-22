use super::*;
use network::{Element, Network};
use std::collections::BTreeMap;
fn checkpoint(r: &Request, start: Instant) -> Result<()> {
    let seconds = r
        .budget
        .as_ref()
        .and_then(|b| b.max_seconds)
        .unwrap_or(120.);
    if !seconds.is_finite() || seconds <= 0. || start.elapsed().as_secs_f64() > seconds {
        return Err(fail("NETWORK_DEADLINE", "Native time budget exhausted"));
    }
    Ok(())
}
fn complex(real: f64, imaginary: f64) -> Value {
    json!({"real":real,"imaginary":imaginary})
}
/// N-API/JSON may represent an integral F64 (20.0) as integer 20 on return.
/// Compare the same physical F64 values, retaining exact structure/identity.
fn same_state(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_state(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(k, v)| b.get(k).is_some_and(|b| same_state(v, b)))
        }
        _ => a == b,
    }
}
/// Validate identity against the retained state before trusting its time for admission.
pub(super) fn initial_time(n: &Network, r: &Request) -> Result<f64> {
    if r.initial.as_ref().and_then(Value::as_str) == Some("steady") {
        return Ok(0.);
    }
    let state = r
        .initial
        .as_ref()
        .ok_or_else(|| fail("NETWORK_STATE", "initial required"))?;
    if state["version"] != 1
        || state["contextId"] != r.context_id
        || state["identity"] != n.identity
        || !n
            .final_state
            .as_ref()
            .is_some_and(|old| same_state(old, state))
    {
        return Err(fail("NETWORK_STATE", "State belongs to another model/context, is unknown/modified, or is no longer the latest state"));
    }
    state["timeS"]
        .as_f64()
        .filter(|t| t.is_finite() && *t >= 0.)
        .ok_or_else(|| fail("NETWORK_STATE", "Invalid physical time"))
}
pub(super) fn output_samples(time: &Time, start: f64) -> Result<usize> {
    if !time.stop_s.is_finite()
        || time.stop_s <= 0.
        || !time.sample_step_s.is_finite()
        || time.sample_step_s <= 0.
    {
        return Err(fail("NETWORK_NUMERICS", "Invalid time/output step"));
    }
    if time.stop_s <= start {
        return Err(fail(
            "NETWORK_STATE",
            "stopS is an absolute end strictly after state time",
        ));
    }
    let ratio = (time.stop_s - start) / time.sample_step_s;
    // Same output count for admission and emission, without double-emitting stop.
    let intervals = (ratio - 32. * f64::EPSILON * ratio.abs().max(1.)).ceil();
    if !intervals.is_finite() || intervals >= 100_000. {
        return Err(fail("NETWORK_BUDGET", "Output sample budget"));
    }
    Ok((intervals as usize).max(1) + 1)
}
pub fn run(n: &mut Network, r: &Request, s: &mut Counters, start: Instant) -> Result<Value> {
    if r.numerics.as_ref().is_some_and(|v| v.max_step_s.is_some()) {
        return Err(fail(
            "NETWORK_NUMERICS",
            "maxStepS requires coupled descriptor SI backend",
        ));
    }
    let mut normalized = 0f64;
    let mut kcl = 0f64;
    if r.mode == "dc" {
        if r.numerics.as_ref().is_some_and(|v| {
            v.relative_tolerance.is_some()
                || v.absolute_tolerance.is_some()
                || v.max_iterations.is_some()
        }) {
            return Err(fail(
                "NETWORK_NUMERICS",
                "DC uses fixed 1e-8 residual/physical balance guards",
            ));
        }
        let (x, res, k) = n.solve(n.matrix(0., None), n.rhs(None), s)?;
        checkpoint(r, start)?;
        let current = n.currents(&x, None);
        let (_, loss, power) = n.energy_loss(&x, None);
        let error = (power - loss).abs();
        if error > 1e-12 + 1e-8 * power.abs().max(loss) {
            return Err(fail("NETWORK_POWER_BALANCE", error));
        }
        return Ok(
            json!({"status":"complete","mode":"dc","nodeVoltages":n.nodes.keys().map(|id|(id,json!(n.v(&x,id)))).collect::<BTreeMap<_,_>>(),"currents":n.circuit.elements.iter().enumerate().map(|(i,e)|(e.id(),json!(current[i]))).collect::<BTreeMap<_,_>>(),"losses":loss,"quality":{"relativeResidual":res,"kclAbsA":k,"sourcePowerW":power,"dissipationW":loss,"powerErrorW":power-loss},"diagnostics":[]}),
        );
    }
    if r.mode == "frequency" {
        if r.numerics.as_ref().is_some_and(|v| {
            v.relative_tolerance.is_some()
                || v.absolute_tolerance.is_some()
                || v.max_iterations.is_some()
        }) {
            return Err(fail(
                "NETWORK_NUMERICS",
                "AC uses fixed 1e-8 residual/physical balance guards",
            ));
        }
        if r.frequencies.is_empty()
            || r.frequencies.len() > 100_000
            || r.frequencies.iter().any(|f| !f.is_finite() || *f <= 0.)
            || r.frequencies.windows(2).any(|w| w[1] <= w[0])
            || n.circuit.ports.is_empty()
        {
            return Err(fail(
                "NETWORK_INPUT",
                "Strictly increasing positive frequencies and ports required",
            ));
        }
        let mut impedances: BTreeMap<String, Vec<Value>> = n
            .circuit
            .ports
            .iter()
            .map(|p| (p.id.clone(), vec![]))
            .collect();
        let mut power_error = 0f64;
        let mut node_voltages: BTreeMap<String, Vec<Value>> =
            n.nodes.keys().map(|id| (id.clone(), vec![])).collect();
        let mut currents: BTreeMap<String, Vec<Value>> = n
            .circuit
            .elements
            .iter()
            .map(|e| (e.id().to_owned(), vec![]))
            .collect();
        for (frequency_index, f) in r.frequencies.iter().enumerate() {
            checkpoint(r, start)?;
            let w = 2. * std::f64::consts::PI * f;
            if !r.ac_sources.is_empty() {
                let (v, i, res, k, p) = ac::response(n, r, frequency_index, s)?;
                normalized = normalized.max(res);
                kcl = kcl.max(k);
                power_error = power_error.max(p);
                for (id, value) in v {
                    node_voltages.get_mut(&id).unwrap().push(value);
                }
                for (id, value) in i {
                    currents.get_mut(&id).unwrap().push(value);
                }
            }
            for p in n.circuit.ports.clone() {
                let mut b = vec![0.; 2 * n.n];
                if let Some(a) = n.nodes[&p.positive] {
                    b[a] += 1.;
                }
                if let Some(a) = n.nodes[&p.negative] {
                    b[a] -= 1.;
                }
                let (x, res, k) = n.solve(n.matrix(0., Some(w)), b, s)?;
                normalized = normalized.max(res);
                kcl = kcl.max(k);
                let re = n.v(&x, &p.positive) - n.v(&x, &p.negative);
                let im = n.v(&x[n.n..], &p.positive) - n.v(&x[n.n..], &p.negative);
                impedances.get_mut(&p.id).unwrap().push(complex(re, im));
                // Peak phasors: real test-source work equals R dissipation; imaginary equals reactive power.
                let (mut real_power, mut imag_power, mut scale) = (0., 0., 0.);
                for (i, e) in n.circuit.elements.iter().enumerate() {
                    let vr = n.v(&x, &e.nodes()[0]) - n.v(&x, &e.nodes()[1]);
                    let vi = n.v(&x[n.n..], &e.nodes()[0]) - n.v(&x[n.n..], &e.nodes()[1]);
                    let (ir, ii) = if let Some(k) = n.branch[i] {
                        (x[k], x[n.n + k])
                    } else if let Element::Resistor { r, .. } = e {
                        (vr / r, vi / r)
                    } else {
                        (0., 0.)
                    };
                    let pr = vr * ir + vi * ii;
                    let pi = vi * ir - vr * ii;
                    real_power += pr;
                    imag_power += pi;
                    scale += pr.abs() + pi.abs();
                }
                let err = (real_power - re).hypot(imag_power - im);
                power_error = power_error.max(err / scale.max(re.hypot(im)).max(1e-12));
                if err > 1e-12 + 1e-8 * scale.max(re.hypot(im)) {
                    return Err(fail("NETWORK_POWER_BALANCE", err));
                }
            }
        }
        let mut report = json!({"status":"complete","mode":"frequency","axis":r.frequencies,"impedances":impedances,"quality":{"relativeResidual":normalized,"kclAbsA":kcl,"relativeComplexPowerError":power_error},"diagnostics":[],"excitation":"Each impedance port separately: +1 A injected into positive, extracted from negative. Independent V=0, I=0. No matched loads; Z=Vpositive-Vnegative."});
        if !r.ac_sources.is_empty() {
            report["nodeVoltages"] = json!(node_voltages);
            report["currents"] = json!(currents);
            report["acExcitation"] = json!({"sources":r.ac_sources,"convention":"Peak phasors Re(V exp(+j omega t)); independent prescribed response, separate from impedance tests"});
        }
        return Ok(report);
    }
    if r.mode != "time" {
        return Err(fail("NETWORK_INPUT", "Unknown analysis mode"));
    }
    transient(n, r, s, start)
}
fn transient(n: &mut Network, r: &Request, s: &mut Counters, start: Instant) -> Result<Value> {
    let time = r
        .time
        .as_ref()
        .ok_or_else(|| fail("NETWORK_INPUT", "time required"))?;
    let rel = r
        .numerics
        .as_ref()
        .and_then(|v| v.relative_tolerance)
        .unwrap_or(1e-6);
    let abs = r
        .numerics
        .as_ref()
        .and_then(|v| v.absolute_tolerance)
        .unwrap_or(1e-9);
    let limit = r
        .numerics
        .as_ref()
        .and_then(|v| v.max_iterations)
        .unwrap_or(200_000);
    if !rel.is_finite()
        || rel <= 0.
        || rel > 0.1
        || !abs.is_finite()
        || abs <= 0.
        || abs > 0.1
        || limit == 0
        || limit > 1_000_000
        || !time.stop_s.is_finite()
        || time.stop_s <= 0.
        || !time.sample_step_s.is_finite()
        || time.sample_step_s <= 0.
    {
        return Err(fail(
            "NETWORK_NUMERICS",
            "Invalid time/tolerances/step budget",
        ));
    }
    let t0 = initial_time(n, r)?;
    let samples = output_samples(time, t0)?;
    let (mut t, mut x) = if r.initial.as_ref().and_then(Value::as_str) == Some("steady") {
        (0., n.solve(n.matrix(0., None), n.rhs(Some(0.)), s)?.0)
    } else {
        let state = r
            .initial
            .as_ref()
            .ok_or_else(|| fail("NETWORK_STATE", "initial required"))?;
        let t = state["timeS"]
            .as_f64()
            .ok_or_else(|| fail("NETWORK_STATE", "Missing physical time"))?;
        let x: Vec<f64> =
            serde_json::from_value(state["x"].clone()).map_err(|e| fail("NETWORK_STATE", e))?;
        if !t.is_finite() || t < 0. || x.len() != n.n || x.iter().any(|v| !v.is_finite()) {
            return Err(fail("NETWORK_STATE", "Invalid state vector"));
        }
        (t, x)
    };
    let mut axis = vec![t];
    let mut states = vec![x.clone()];
    // Two equally over-damped large BE steps can falsely satisfy step doubling.
    // Seed from conservative physical RC/LR/LC scales at start/source corners,
    // independently of output cadence, then allow error control to grow h.
    let mut resistances = Vec::new();
    let mut inductances = Vec::new();
    let mut capacitances = Vec::new();
    for e in &n.circuit.elements {
        match e {
            Element::Resistor { r, .. } => resistances.push(*r),
            Element::Inductor { r, l, .. } => {
                if *r > 0. {
                    resistances.push(*r);
                }
                inductances.push(*l);
            }
            Element::Capacitor { c, r, l, .. } => {
                capacitances.push(*c);
                if *r > 0. {
                    resistances.push(*r);
                }
                if *l > 0. {
                    inductances.push(*l);
                }
            }
            _ => {}
        }
    }
    let min = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
    let (cr, lr, lc) = (
        min(&capacitances) * min(&resistances) / resistances.len().max(1) as f64,
        min(&inductances) / resistances.iter().sum::<f64>(),
        min(&inductances).sqrt() * min(&capacitances).sqrt() / n.circuit.elements.len() as f64,
    );
    let seed = [cr, lr, lc]
        .into_iter()
        .filter(|v| v.is_finite())
        .fold(f64::INFINITY, f64::min)
        / 10.;
    let mut h = ((time.stop_s - t) / 100.).min(seed);
    let max_h = n
        .circuit
        .elements
        .iter()
        .filter_map(Element::waveform)
        .map(Waveform::max_step)
        .fold((time.stop_s - t) / 20., f64::min);
    let mut residual = 0f64;
    let mut kcl = 0f64;
    let mut work = 0.;
    let mut dissipated = 0.;
    let mut exchanged = 0.;
    let initial_energy = n.energy_loss(&x, Some(t)).0;
    let mut max_local_error = 0f64;
    let mut min_h = f64::INFINITY;
    let mut max_accepted_h = 0f64;
    let mut next_sample = 1usize;
    let step = |n: &mut Network,
                x: &[f64],
                at: f64,
                h: f64,
                s: &mut Counters|
     -> Result<(Vec<f64>, f64, f64)> {
        // Equivalent increment form avoids subtracting large E*x/h charges to
        // recover tiny capacitor currents on an absolute voltage rail.
        let mut b = n.rhs(Some(at));
        for (&(i, j), &v) in &n.g {
            b[i] -= v * x[j];
        }
        let (delta, res, kcl) = n.solve(n.matrix(1. / h, None), b, s)?;
        let next = x.iter().zip(delta).map(|(a, b)| a + b).collect::<Vec<_>>();
        if next.iter().any(|v| !v.is_finite()) {
            return Err(fail("NETWORK_NONFINITE", "Time-state increment overflow"));
        }
        Ok((next, res, kcl))
    };
    while t < time.stop_s {
        checkpoint(r, start)?;
        if s.accepted_steps + s.rejected_steps >= limit {
            return Err(fail(
                "NETWORK_STEP_BUDGET",
                "Internal step budget exhausted; result is incomplete",
            ));
        }
        let event = n
            .circuit
            .elements
            .iter()
            .filter_map(Element::waveform)
            .map(|w| w.next(t))
            .fold(time.stop_s, f64::min);
        h = h.min(max_h).min(event - t).min(time.stop_s - t);
        if h <= 32. * f64::EPSILON * t.abs().max(1e-30) {
            return Err(fail(
                "NETWORK_STEP_UNDERFLOW",
                "Cannot resolve source/dynamic time scale",
            ));
        }
        let reaches_event = (t + h - event).abs() <= 16. * f64::EPSILON * event.abs();
        let at = if reaches_event && event > 0. {
            f64::from_bits(event.to_bits() - 1)
        } else {
            t + h
        };
        let (full, _, _) = step(n, &x, at, h, s)?;
        let (half, res1, k1) = step(n, &x, t + h / 2., h / 2., s)?;
        let (mut fine, res2, k2) = step(n, &half, at, h / 2., s)?;
        let err = full
            .iter()
            .zip(&fine)
            .zip(&x)
            .enumerate()
            .map(|(i, ((a, b), old))| {
                let physical_floor = if i < n.nv || n.cap.contains(&Some(i)) {
                    1. /* V */
                } else {
                    1. /* A */
                };
                (a - b).abs() / (abs * physical_floor + rel * b.abs().max(old.abs()))
            })
            .fold(0., f64::max);
        if err > 1. {
            s.rejected_steps += 1;
            h *= (0.9 / err.sqrt()).clamp(0.1, 0.5);
            continue;
        }
        s.accepted_steps += 1;
        max_local_error = max_local_error.max(err);
        min_h = min_h.min(h / 2.);
        max_accepted_h = max_accepted_h.max(h / 2.);
        residual = residual.max(res1).max(res2);
        kcl = kcl.max(k1).max(k2);
        // Accepted BE half-steps: endpoint quadrature matches discrete equations;
        // imbalance retains (does not clamp) numerical dissipation of C/L.
        for (y, at) in [(&half, t + h / 2.), (&fine, at)] {
            let (_, loss, power) = n.energy_loss(y, Some(at));
            work += power * h / 2.;
            dissipated += loss * h / 2.;
            exchanged += (power.abs() + loss) * h / 2.;
        }
        // Arrive on the left, then impose right-hand algebraic constraints
        // while preserving every capacitor voltage/inductor (incl. ESL) current.
        // This prevents interpolation from smearing a current-source jump.
        let right = if reaches_event
            && n.circuit
                .elements
                .iter()
                .filter_map(Element::waveform)
                .any(Waveform::discontinuous)
        {
            let mut m = n.matrix(0., None);
            let mut b = n.rhs(Some(event));
            for (i, e) in n.circuit.elements.iter().enumerate() {
                let mut held = vec![];
                match e {
                    Element::Inductor { .. } => held.push(n.branch[i].unwrap()),
                    Element::Capacitor { l, .. } => {
                        held.push(n.cap[i].unwrap());
                        if *l > 0. {
                            held.push(n.branch[i].unwrap());
                        }
                    }
                    _ => {}
                }
                for row in held {
                    m.retain(|&(r, _), _| r != row);
                    m.insert((row, row), 1.);
                    b[row] = fine[row];
                }
            }
            Some(n.solve(m, b, s).map_err(|e|fail("NETWORK_IMPULSE_UNSUPPORTED",format!("Event conflicts with preserved C/L state; specify a finite front. {e}")))?.0)
        } else {
            None
        };
        // Dense output is piecewise-linear over accepted half steps. sampleStepS
        // never bounds h and does not determine internal adaptive dynamics.
        while next_sample < samples {
            let mut out = if next_sample == samples - 1 {
                time.stop_s
            } else {
                (t0 + next_sample as f64 * time.sample_step_s).min(time.stop_s)
            };
            if (out - event).abs() <= 16. * f64::EPSILON * event.abs() {
                out = event;
            }
            if out > t + h + 16. * f64::EPSILON * time.stop_s {
                break;
            }
            let (a, b, weight) = if out <= t + h / 2. {
                (&x, &half, (out - t) / (h / 2.))
            } else {
                (&half, &fine, (out - t - h / 2.) / (h / 2.))
            };
            let y = if (out - event).abs() <= 16. * f64::EPSILON * event.abs() && right.is_some() {
                right.as_ref().unwrap().clone()
            } else {
                a.iter()
                    .zip(b)
                    .map(|(a, b)| a + (b - a) * weight.clamp(0., 1.))
                    .collect()
            };
            states.push(y);
            axis.push(out);
            next_sample += 1;
        }
        if let Some(right) = right {
            fine = right;
        }
        t = if reaches_event { event } else { t + h };
        x = fine;
        h *= if err == 0. {
            2.
        } else {
            (0.9 / err.sqrt()).clamp(0.5, 2.)
        };
        if reaches_event {
            h = h.min(seed);
        }
    }
    let final_energy = n.energy_loss(&x, Some(t)).0;
    let energy_error = work - dissipated - (final_energy - initial_energy);
    if energy_error.abs() > 1e-15 + 0.005 * exchanged {
        return Err(fail("NETWORK_ENERGY_BALANCE",format!("Work-loss-stored mismatch {energy_error} J for {exchanged} J exchange; refine time numerics")));
    }
    let mut nodes: BTreeMap<String, Vec<f64>> =
        n.nodes.keys().map(|id| (id.clone(), vec![])).collect();
    let mut currents: BTreeMap<String, Vec<f64>> = n
        .circuit
        .elements
        .iter()
        .map(|e| (e.id().to_owned(), vec![]))
        .collect();
    let mut losses = vec![];
    for (y, at) in states.iter().zip(&axis) {
        for (id, values) in &mut nodes {
            values.push(n.v(y, id));
        }
        for (i, v) in n.currents(y, Some(*at)).iter().enumerate() {
            currents
                .get_mut(n.circuit.elements[i].id())
                .unwrap()
                .push(*v);
        }
        losses.push(n.energy_loss(y, Some(*at)).1);
    }
    let storage=n.circuit.elements.iter().enumerate().filter_map(|(i,e)|match e {Element::Capacitor{..}=>Some((e.id(),json!({"capacitorVoltageV":x[n.cap[i].unwrap()],"seriesCurrentA":x[n.branch[i].unwrap()]}))),Element::Inductor{..}=>Some((e.id(),json!({"inductorCurrentA":x[n.branch[i].unwrap()]}))),_=>None}).collect::<BTreeMap<_,_>>();
    Ok(
        json!({"status":"complete","mode":"time","axis":axis,"nodeVoltages":nodes,"currents":currents,"losses":losses,"finalState":{"version":1,"contextId":r.context_id,"identity":n.identity,"timeS":t,"x":x,"storage":storage},"quality":{"relativeResidual":residual,"kclAbsA":kcl,"sourceWorkJ":work,"dissipationJ":dissipated,"initialEnergyJ":initial_energy,"finalEnergyJ":final_energy,"energyErrorJ":energy_error,"relativeEnergyError":energy_error.abs()/exchanged.max(1e-15),"exchangedEnergyJ":exchanged,"maxAcceptedLocalError":max_local_error,"minInternalStepS":min_h,"maxInternalStepS":max_accepted_h,"relativeTolerance":rel,"absoluteTolerance":abs},"diagnostics":[],"interpolation":"Output only: piecewise-linear on accepted BE half-steps. Sampled extrema may miss peaks."}),
    )
}
