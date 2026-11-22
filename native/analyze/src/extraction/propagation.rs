//! Symmetric one/two conductor PEC lines: modal L/C, stable wave formulation.
//! No intermediate singular Z/Y, no artificial loss or lumped segmentation.
use super::*;
pub fn parameters(c: &[Vec<f64>], c0: &[Vec<f64>]) -> Result<Value> {
    let n = c.len();
    if n < 1
        || n > 2
        || c0.len() != n
        || c.iter()
            .chain(c0)
            .any(|r| r.len() != n || r.iter().any(|v| !v.is_finite()))
    {
        return Err(error("SECTION_MATRIX", "one/two conductors required"));
    }
    let mut l = vec![vec![0.; n]; n];
    // Solve C0 L = mu0*epsilon0 I; <=2 RHS closed elimination, no inverse array.
    if n == 1 {
        l[0][0] = MU0 * EPS0 / c0[0][0];
    } else {
        for k in 0..2 {
            let b0 = if k == 0 { MU0 * EPS0 } else { 0. };
            let b1 = if k == 1 { MU0 * EPS0 } else { 0. };
            let pivot = c0[1][1] - c0[1][0] * c0[0][1] / c0[0][0];
            if pivot <= 0. {
                return Err(error("SECTION_MATRIX", "C0 not SPD"));
            }
            l[1][k] = (b1 - c0[1][0] * b0 / c0[0][0]) / pivot;
            l[0][k] = (b0 - c0[0][1] * l[1][k]) / c0[0][0];
        }
    }
    let mut modes = vec![];
    for (i, name) in if n == 1 {
        vec!["single"]
    } else {
        vec!["even", "odd"]
    }
    .iter()
    .enumerate()
    {
        let sign = if i == 0 { 1. } else { -1. };
        let cm = c[0][0] + if n == 2 { sign * c[0][1] } else { 0. };
        let lm = l[0][0] + if n == 2 { sign * l[0][1] } else { 0. };
        if cm <= 0. || lm <= 0. || !cm.is_finite() || !lm.is_finite() {
            return Err(error("SECTION_MATRIX", "invalid modal energy"));
        }
        let impedance = (lm / cm).sqrt();
        let delay = (lm * cm).sqrt();
        if !impedance.is_finite() || !delay.is_finite() || impedance <= 0. || delay <= 0. {
            return Err(error(
                "SECTION_NONFINITE",
                "modal impedance/delay overflow or underflow; input is outside numerical range",
            ));
        }
        modes.push(json!({"name":name,"capacitanceFPerM":cm,"inductanceHPerM":lm,"impedanceOhm":impedance,"delaySPerM":delay}));
    }
    if n == 2 {
        for a in [c, c0, &l] {
            if (a[0][0] - a[1][1]).abs() > 1e-8 * a[0][0]
                || (a[0][1] - a[1][0]).abs() > 1e-8 * a[0][0]
            {
                return Err(error(
                    "SECTION_SYMMETRY",
                    "pair requires reflection-symmetric numerical matrices",
                ));
            }
        }
    }
    let z: Vec<f64> = modes
        .iter()
        .map(|m| m["impedanceOhm"].as_f64().unwrap())
        .collect();
    Ok(
        json!({"conductorOrder":if n==1{vec!["signal"]}else{vec!["positive","negative"]},"capacitanceFPerM":c,"vacuumCapacitanceFPerM":c0,"inductanceHPerM":l,"modes":modes,"differentialOhm":if n==2{Some(2.*z[1])}else{None},"commonOhm":if n==2{Some(z[0]/2.)}else{None},"resistanceOhmPerM":{"state":"assumed zero PEC","matrix":vec![vec![0.;n];n]},"conductanceSPerM":{"state":"assumed zero lossless dielectric","matrix":vec![vec![0.;n];n]}}),
    )
}
#[derive(Clone, Copy, Default)]
pub(super) struct Z {
    pub r: f64,
    pub i: f64,
}
impl Z {
    pub fn div(self, b: Self) -> Self {
        let d = b.r * b.r + b.i * b.i;
        Self {
            r: (self.r * b.r + self.i * b.i) / d,
            i: (self.i * b.r - self.r * b.i) / d,
        }
    }
    pub fn abs(self) -> f64 {
        self.r.hypot(self.i)
    }
    pub fn mul(self, b: Self) -> Self {
        Self {
            r: self.r * b.r - self.i * b.i,
            i: self.r * b.i + self.i * b.r,
        }
    }
    pub fn add(self, b: Self) -> Self {
        Self {
            r: self.r + b.r,
            i: self.i + b.i,
        }
    }
    pub fn sub(self, b: Self) -> Self {
        Self {
            r: self.r - b.r,
            i: self.i - b.i,
        }
    }
    pub fn scale(self, x: f64) -> Self {
        Self {
            r: self.r * x,
            i: self.i * x,
        }
    }
    pub fn sqrt(self) -> Self {
        // Stable passive principal root for Re/Im(zy)>=0 or Im(zy)>=0.
        let m = self.abs();
        if self.r >= 0. {
            let r = ((m + self.r) / 2.).sqrt();
            Self {
                r,
                i: self.i / (2. * r),
            }
        } else {
            let i = ((m - self.r) / 2.).sqrt().copysign(self.i);
            Self {
                r: self.i / (2. * i),
                i,
            }
        }
    }
    pub fn value(self) -> Value {
        json!({"real":self.r,"imaginary":self.i})
    }
}

/// Stable lossy modal waves; no positive exponential, cosh or sinh.
pub fn lossy_network(samples: &[Value], length: f64, reference: f64) -> Result<Value> {
    if !length.is_finite()
        || length <= 0.
        || !reference.is_finite()
        || reference <= 0.
        || samples.is_empty()
    {
        return Err(error(
            "SECTION_INPUT",
            "positive length/reference and samples required",
        ));
    }
    let n = samples[0]["modes"]
        .as_array()
        .ok_or_else(|| error("SECTION_INPUT", "missing lossy modes"))?
        .len();
    if !(1..=2).contains(&n) {
        return Err(error("SECTION_INPUT", "one/two lossy modes required"));
    }
    let mut all = vec![];
    let mut max_passivity: f64 = 0.;
    let mut max_reciprocity: f64 = 0.;
    let mut max_power: f64 = 0.;
    let mut power_balances = vec![];
    for sample in samples {
        let modes = sample["modes"].as_array().unwrap();
        let mut out = vec![vec![Z::default(); 2 * n]; 2 * n];
        for (k, m) in modes.iter().enumerate() {
            let z = Z {
                r: m["characteristicImpedanceOhm"]["real"].as_f64().unwrap(),
                i: m["characteristicImpedanceOhm"]["imaginary"]
                    .as_f64()
                    .unwrap(),
            };
            let alpha = m["alphaNpPerM"].as_f64().unwrap();
            let angle = m["betaRadPerM"].as_f64().unwrap() * length;
            if !angle.is_finite() || angle.abs() > 1e9 {
                return Err(error(
                    "SECTION_APPLICABILITY",
                    "phase exceeds validated argument range",
                ));
            }
            let attenuation = (-alpha * length).exp();
            let (s, c) = angle.sin_cos();
            let q = Z {
                r: attenuation * c,
                i: -attenuation * s,
            };
            let one = Z { r: 1., i: 0. };
            let zr = Z {
                r: reference,
                i: 0.,
            };
            let r = z.sub(zr).div(z.add(zr));
            let q2 = q.mul(q);
            let r2 = r.mul(r);
            let den = one.sub(r2.mul(q2));
            let refl = r.mul(one.sub(q2)).div(den);
            let trans = one.sub(r2).mul(q).div(den);
            let resistance = m["resistanceOhmPerM"]
                .as_f64()
                .ok_or_else(|| error("SECTION_INPUT", "missing modal R"))?;
            let conductance = m["conductanceSPerM"]
                .as_f64()
                .ok_or_else(|| error("SECTION_INPUT", "missing modal G"))?;
            let a = one
                .add(refl)
                .scale(reference.sqrt())
                .add(z.mul(one.sub(refl).scale(1. / reference.sqrt())))
                .scale(0.5);
            let d = trans
                .scale(reference.sqrt())
                .sub(z.mul(trans.scale(1. / reference.sqrt())))
                .scale(0.5);
            let decay_integral = if alpha == 0. {
                length
            } else {
                -(-2. * alpha * length).exp_m1() / (2. * alpha)
            };
            let beta = m["betaRadPerM"].as_f64().unwrap();
            let cross = 2.
                * a.mul(Z { r: d.r, i: -d.i }).r
                * attenuation
                * if beta == 0. {
                    length
                } else {
                    angle.sin() / beta
                };
            let energy = (a.abs().powi(2) + d.abs().powi(2)) * decay_integral;
            let loss = 0.5
                * (resistance * (energy - cross) / z.abs().powi(2)
                    + conductance * (energy + cross));
            let absorbed = 0.5 * (1. - refl.abs().powi(2) - trans.abs().powi(2));
            let residual = (loss - absorbed).abs() / 0.5;
            if !residual.is_finite() || residual > 1e-8 {
                return Err(error(
                    "SECTION_POWER_BALANCE",
                    format!("mode {k}: residual {residual}"),
                ));
            }
            max_power = max_power.max(residual);
            power_balances.push(json!({"frequencyHz":sample["frequencyHz"],"mode":k,"incidentW":0.5,"reflectedW":0.5*refl.abs().powi(2),"transmittedW":0.5*trans.abs().powi(2),"integratedDissipatedW":loss,"normalizedResidual":residual,"normalization":"unit peak power wave, incident average power 0.5 W; full modal R includes all signal/reference surfaces"}));
            // The orthonormal conductor transform preserves full mutual loss.
            for i in 0..n {
                for j in 0..n {
                    let w = if n == 1 {
                        1.
                    } else if k == 1 && i != j {
                        -0.5
                    } else {
                        0.5
                    };
                    for (a, b, v) in [
                        (i, j, refl),
                        (i + n, j + n, refl),
                        (i + n, j, trans),
                        (i, j + n, trans),
                    ] {
                        out[a][b] = out[a][b].add(v.scale(w));
                    }
                }
            }
            // Each symmetric modal two-port diagonalizes under [1,+/-1].
            // Largest singular value is max(|r+t|,|r-t|); orthonormal pair
            // transformation leaves singular values unchanged.
            max_passivity =
                max_passivity.max(refl.add(trans).abs().max(refl.sub(trans).abs()) - 1.);
        }
        for i in 0..2 * n {
            for j in 0..2 * n {
                let v = out[i][j];
                if !v.r.is_finite() || !v.i.is_finite() {
                    return Err(error("SECTION_NONFINITE", "nonfinite lossy scattering"));
                }
                max_reciprocity = max_reciprocity.max(v.sub(out[j][i]).abs());
            }
        }
        all.push(
            out.iter()
                .map(|r| r.iter().map(|v| v.value()).collect::<Vec<_>>())
                .collect::<Vec<_>>(),
        );
    }
    if max_passivity > 1e-10 || max_reciprocity > 1e-10 {
        return Err(error(
            "SECTION_PROPAGATION",
            format!("passivity {max_passivity}, reciprocity {max_reciprocity}"),
        ));
    }
    Ok(
        json!({"ports":if n==1{vec!["input","output"]}else{vec!["positiveInput","negativeInput","positiveOutput","negativeOutput"]},"parameters":all,"referenceOhm":reference,"maxPassivityExcess":max_passivity,"maxReciprocityError":max_reciprocity,"maxPowerBalanceResidual":max_power,"powerBalances":power_balances,"phaseConvention":"exp(+j omega t), passive gamma, transmission exp(-gamma length), currents into both ends"}),
    )
}
pub fn network(p: &Value, frequencies: &[f64], length: f64, reference: f64) -> Result<Value> {
    if !length.is_finite() || length <= 0. || !reference.is_finite() || reference <= 0. {
        return Err(error("SECTION_INPUT", "positive length/reference required"));
    }
    let modes = p["modes"]
        .as_array()
        .ok_or_else(|| error("SECTION_INPUT", "missing modes"))?;
    let n = modes.len();
    if !(1..=2).contains(&n) {
        return Err(error("SECTION_INPUT", "one/two modal waves required"));
    }
    let mut all = vec![];
    let mut unitary: f64 = 0.;
    for &f in frequencies {
        let mut out = vec![vec![Z::default(); 2 * n]; 2 * n];
        for (k, m) in modes.iter().enumerate() {
            let z = m["impedanceOhm"].as_f64().unwrap();
            let delay = m["delaySPerM"].as_f64().unwrap();
            if !z.is_finite() || !delay.is_finite() || z <= 0. || delay <= 0. {
                return Err(error("SECTION_NONFINITE", "invalid modal wave"));
            }
            let angle = 2. * std::f64::consts::PI * f * length * delay;
            if !angle.is_finite() || angle.abs() > 1e9 {
                return Err(error(
                    "SECTION_APPLICABILITY",
                    "phase exceeds validated argument range",
                ));
            }
            let (s, c) = angle.sin_cos();
            let den = Z {
                r: 2. * c,
                i: (z / reference + reference / z) * s,
            };
            let refl = Z {
                r: 0.,
                i: (z / reference - reference / z) * s,
            }
            .div(den);
            let trans = Z { r: 2., i: 0. }.div(den);
            for i in 0..n {
                for j in 0..n {
                    let weight = if n == 1 {
                        1.
                    } else if k == 1 && i != j {
                        -0.5
                    } else {
                        0.5
                    };
                    for (a, b, v) in [
                        (i, j, refl),
                        (i + n, j + n, refl),
                        (i + n, j, trans),
                        (i, j + n, trans),
                    ] {
                        out[a][b].r += weight * v.r;
                        out[a][b].i += weight * v.i;
                    }
                }
            }
        }
        if out
            .iter()
            .flatten()
            .any(|z| !z.r.is_finite() || !z.i.is_finite())
        {
            return Err(error(
                "SECTION_NONFINITE",
                "nonfinite scattering coefficient",
            ));
        }
        for i in 0..2 * n {
            for j in 0..2 * n {
                let mut dot = Z::default();
                for row in &out {
                    dot.r += row[i].r * row[j].r + row[i].i * row[j].i;
                    dot.i += row[i].r * row[j].i - row[i].i * row[j].r;
                }
                dot.r -= if i == j { 1. } else { 0. };
                unitary = unitary.max(dot.abs());
            }
        }
        all.push(
            out.iter()
                .map(|r| {
                    r.iter()
                        .map(|z| json!({"real":z.r,"imaginary":z.i}))
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>(),
        );
    }
    if unitary > 1e-10 || !unitary.is_finite() {
        return Err(error("SECTION_PROPAGATION", "lossless unitarity failed"));
    }
    Ok(
        json!({"ports":if n==1{vec!["input","output"]}else{vec!["positiveInput","negativeInput","positiveOutput","negativeOutput"]},"parameters":all,"referenceOhm":reference,"maxUnitarityError":unitary,"phaseConvention":"exp(+j omega t), matched transmission exp(-j omega delay), currents into both ends"}),
    )
}
